#![no_std]
#![no_main]
#![feature(asm)]
#![feature(never_type)]
#![feature(const_fn)]
#![feature(global_asm)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

mod allocator;
mod services;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use mythril_core::vm::VmServices;
use mythril_core::*;
use spin::RwLock;

// Temporary helper function to create a vm for a single core
fn default_vm(
    core: usize,
    mem: u64,
    services: &mut impl VmServices,
) -> Arc<RwLock<vm::VirtualMachine>> {
    let mut config = vm::VirtualMachineConfig::new(vec![core as u8], mem);

    // FIXME: When `map_bios` may return an error, log the error.
    config.map_bios("seabios.bin".into()).unwrap_or(());

    let device_map = config.device_map();
    device_map
        .register_device(device::acpi::AcpiRuntime::new(0xb000).unwrap())
        .unwrap();
    device_map
        .register_device(device::com::ComDevice::new(core as u64, 0x3F8))
        .unwrap();
    device_map
        .register_device(device::com::ComDevice::new(core as u64, 0x2F8))
        .unwrap();
    device_map
        .register_device(device::com::ComDevice::new(core as u64, 0x3E8))
        .unwrap();
    device_map
        .register_device(device::com::ComDevice::new(core as u64, 0x2E8))
        .unwrap();
    device_map
        .register_device(device::debug::DebugPort::new(core as u64, 0x402))
        .unwrap();
    device_map
        .register_device(device::vga::VgaController::new())
        .unwrap();
    device_map
        .register_device(device::dma::Dma8237::new())
        .unwrap();
    device_map
        .register_device(device::ignore::IgnoredDevice::new())
        .unwrap();
    device_map
        .register_device(device::pci::PciRootComplex::new())
        .unwrap();
    device_map
        .register_device(device::pic::Pic8259::new())
        .unwrap();
    device_map
        .register_device(device::keyboard::Keyboard8042::new())
        .unwrap();
    device_map
        .register_device(device::pit::Pit8254::new())
        .unwrap();
    device_map
        .register_device(device::pos::ProgrammableOptionSelect::new())
        .unwrap();
    device_map
        .register_device(device::rtc::CmosRtc::new(mem))
        .unwrap();

    let mut fw_cfg_builder = device::qemu_fw_cfg::QemuFwCfgBuilder::new();

    // The 'linuxboot' file is an option rom that loads the linux kernel
    // via qemu_fw_cfg
    fw_cfg_builder
        .add_file(
            "genroms/linuxboot.bin",
            services.read_file("linuxboot.bin").unwrap(),
        )
        .unwrap();

    // Passing the bootorder file automatically selects the option rom
    // as the default boot device
    fw_cfg_builder
        .add_file("bootorder", "/rom@genroms/linuxboot.bin\nHALT".as_bytes())
        .unwrap();

    linux::load_linux(
        "kernel",
        "initramfs",
        "earlyprintk=serial,0x3f8,115200 debug nokaslr\0".as_bytes(),
        mem,
        &mut fw_cfg_builder,
        services,
    )
    .unwrap();
    device_map.register_device(fw_cfg_builder.build()).unwrap();

    vm::VirtualMachine::new(config, services).expect("Failed to create vm")
}

fn global_alloc_region(info: &multiboot2::BootInformation) -> (u64, u64) {
    let mem_tag = info
        .memory_map_tag()
        .expect("Missing multiboot memory map tag");

    let available = mem_tag
        .memory_areas()
        .map(|area| (area.start_address(), area.end_address()));

    debug!("Modules:");
    let modules = info.module_tags().map(|module| {
        debug!(
            "  0x{:x}-0x{:x}",
            module.start_address(),
            module.end_address()
        );
        (module.start_address() as u64, module.end_address() as u64)
    });

    let sections_tag = info
        .elf_sections_tag()
        .expect("Missing multiboot elf sections tag");

    debug!("Elf sections:");
    let sections = sections_tag.sections().map(|section| {
        debug!(
            "  0x{:x}-0x{:x}",
            section.start_address(),
            section.end_address()
        );
        (section.start_address(), section.end_address())
    });

    // Avoid allocating over the BootInformation structure itself
    let multiboot_info = [(info.start_address() as u64, info.end_address() as u64)];
    debug!(
        "Multiboot Info: 0x{:x}-0x{:x}",
        info.start_address(),
        info.end_address()
    );

    let excluded = modules
        .chain(sections)
        .chain(multiboot_info.iter().copied());

    // TODO: For now, we just use the portion of the largest available
    // region that is above the highest excluded region.
    let max_excluded = excluded
        .max_by(|left, right| left.1.cmp(&right.1))
        .expect("No max excluded region");

    let largest_region = available
        .max_by(|left, right| (left.1 - left.0).cmp(&(right.1 - right.0)))
        .expect("No largest region");

    if largest_region.0 > max_excluded.1 {
        largest_region
    } else if max_excluded.1 > largest_region.0 && max_excluded.1 < largest_region.1 {
        (max_excluded.1, largest_region.1)
    } else {
        panic!("Unable to find suitable global alloc region")
    }
}

#[no_mangle]
pub extern "C" fn kmain(multiboot_info_addr: usize) -> ! {
    // Setup the actual interrupt handlers
    unsafe { interrupt::idt::init() };

    // Setup our (com0) logger
    log::set_logger(&logger::DirectLogger {})
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .expect("Failed to set logger");

    // Calibrate the timers
    unsafe { tsc::calibrate().expect("Failed to calibrate TSC") };

    let multiboot_info = unsafe { multiboot2::load(multiboot_info_addr) };

    let alloc_region = global_alloc_region(&multiboot_info);

    info!(
        "Allocating from 0x{:x}-{:x}",
        alloc_region.0, alloc_region.1
    );

    unsafe { allocator::Allocator::allocate_from(alloc_region.0, alloc_region.1) }

    let mut multiboot_services = services::Multiboot2Services::new(multiboot_info);
    let mut map = BTreeMap::new();
    map.insert(0usize, default_vm(0, 512, &mut multiboot_services));
    let map: &'static _ = Box::leak(Box::new(map));

    vcpu::smp_entry_point(map)
}
