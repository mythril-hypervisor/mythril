#![no_std]
#![no_main]
#![feature(llvm_asm)]
#![feature(never_type)]
#![feature(const_fn)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

mod allocator;
mod services;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use mythril_core::*;
use spin::RwLock;

extern "C" {
    static AP_STARTUP_ADDR: u16;
    static mut AP_STACK_ADDR: u64;
    static mut AP_READY: u8;
}

// Temporary helper function to create a vm for a single core
fn default_vm(
    core: usize,
    mem: u64,
    services: &mut impl vm::VmServices,
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

    //TODO: this should actually be per-vcpu
    device_map
        .register_device(device::lapic::LocalApic::new())
        .unwrap();

    let mut fw_cfg_builder = device::qemu_fw_cfg::QemuFwCfgBuilder::new();

    // The 'linuxboot' file is an option rom that loads the linux kernel
    // via qemu_fw_cfg
    fw_cfg_builder
        .add_file(
            "genroms/linuxboot_dma.bin",
            services.read_file("linuxboot_dma.bin").unwrap(),
        )
        .unwrap();

    // Passing the bootorder file automatically selects the option rom
    // as the default boot device
    fw_cfg_builder
        .add_file(
            "bootorder",
            "/rom@genroms/linuxboot_dma.bin\nHALT".as_bytes(),
        )
        .unwrap();

    linux::load_linux(
        "kernel",
        "initramfs",
        "earlyprintk=serial,0x3f8,115200 console=ttyS0 debug nokaslr noapic mitigations=off\0"
            .as_bytes(),
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
    let multiboot_info =
        [(info.start_address() as u64, info.end_address() as u64)];
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
    } else if max_excluded.1 > largest_region.0
        && max_excluded.1 < largest_region.1
    {
        (max_excluded.1, largest_region.1)
    } else {
        panic!("Unable to find suitable global alloc region")
    }
}

#[no_mangle]
pub extern "C" fn ap_entry() -> ! {
    unsafe { interrupt::idt::ap_init() };

    let local_apic =
        apic::LocalApic::init().expect("Failed to initialize local APIC");

    info!(
        "X2APIC:\tid={}\tbase=0x{:x}\tversion=0x{:x}",
        local_apic.id(),
        local_apic.raw_base(),
        local_apic.version()
    );

    vcpu::mp_entry_point(local_apic.id())
}

static LOGGER: logger::DirectLogger = logger::DirectLogger::new();

#[no_mangle]
pub extern "C" fn kmain(multiboot_info_addr: usize) -> ! {
    // Setup the actual interrupt handlers
    unsafe { interrupt::idt::init() };

    // Setup our (com0) logger
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .expect("Failed to set logger");

    // Calibrate the global time source
    unsafe {
        time::init_global_time().expect("Failed to init global timesource")
    }

    let multiboot_info = unsafe { multiboot2::load(multiboot_info_addr) };

    let alloc_region = global_alloc_region(&multiboot_info);

    info!(
        "Allocating from 0x{:x}-{:x}",
        alloc_region.0, alloc_region.1
    );

    unsafe {
        allocator::Allocator::allocate_from(alloc_region.0, alloc_region.1)
    }

    // Locate the RSDP and start ACPI parsing
    let rsdp = acpi::rsdp::RSDP::find().expect("Failed to find the RSDP");
    info!("{:?}", rsdp);

    let rsdt = match rsdp.rsdt() {
        Ok(rsdt) => rsdt,
        Err(e) => panic!("Failed to create the RSDT: {:?}", e),
    };
    info!("{:?}", rsdt);

    for entry in rsdt.entries() {
        match entry {
            Ok(sdt) => info!("{:?}", sdt),
            Err(e) => info!("Malformed SDT: {:?}", e),
        }
    }

    let mut multiboot_services =
        services::Multiboot2Services::new(multiboot_info);

    let local_apic =
        apic::LocalApic::init().expect("Failed to initialize local APIC");

    let madt_sdt = rsdt.find_entry(b"APIC").expect("No MADT found");
    let madt = acpi::madt::MADT::new(&madt_sdt);

    let hpet_sdt = rsdt.find_entry(b"HPET").expect("No HPET found");
    let hpet = acpi::hpet::HPET::new(&hpet_sdt)
        .unwrap_or_else(|e| panic!("Failed to create the HPET: {:?}", e));

    info!("{:?}", hpet);

    let apic_ids = madt
        .structures()
        .filter_map(|ics| match ics {
            // TODO(dlrobertson): Check the flags to ensure we can acutally
            // use this APIC.
            Ok(acpi::madt::Ics::LocalApic { apic_id, .. })
                if apic_id != local_apic.id() as u8 =>
            {
                Some(apic_id as u32)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut map = BTreeMap::new();
    map.insert(local_apic.id(), default_vm(0, 256, &mut multiboot_services));
    for apic_id in apic_ids.iter() {
        map.insert(
            *apic_id as usize,
            default_vm(*apic_id as usize, 256, &mut multiboot_services),
        );
    }
    unsafe {
        vm::VM_MAP = Some(map);
    }

    debug!("AP_STARTUP address: 0x{:x}", unsafe { AP_STARTUP_ADDR });

    for ap_apic_id in apic_ids.into_iter() {
        unsafe {
            // Allocate a stack for the AP
            let stack = vec![0u8; 100 * 1024];

            // Get the the bottom of the stack and align
            let stack_bottom = (stack.as_ptr() as u64 + stack.len() as u64)
                & 0xFFFFFFFFFFFFFFF0;

            core::mem::forget(stack);

            core::ptr::write_volatile(
                &mut AP_STACK_ADDR as *mut u64,
                stack_bottom,
            );
        }

        // mfence to ensure that the APs see the new stack address
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        debug!("Send INIT to AP id={}", ap_apic_id);
        local_apic.send_ipi(
            ap_apic_id,
            apic::DstShorthand::NoShorthand,
            apic::TriggerMode::Edge,
            apic::Level::Assert,
            apic::DstMode::Physical,
            apic::DeliveryMode::Init,
            0,
        );

        debug!("Send SIPI to AP id={}", ap_apic_id);
        local_apic.send_ipi(
            ap_apic_id,
            apic::DstShorthand::NoShorthand,
            apic::TriggerMode::Edge,
            apic::Level::Assert,
            apic::DstMode::Physical,
            apic::DeliveryMode::StartUp,
            unsafe { (AP_STARTUP_ADDR >> 12) as u8 },
        );

        // Wait until the AP reports that it is done with startup
        while unsafe { core::ptr::read_volatile(&AP_READY as *const u8) != 1 } {
        }

        // Once the AP is done, clear the ready flag
        unsafe {
            core::ptr::write_volatile(&mut AP_READY as *mut u8, 0);
        }
    }

    vcpu::mp_entry_point(local_apic.id())
}
