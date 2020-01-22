#![no_std]
#![no_main]
#![feature(asm)]
#![feature(never_type)]
#![feature(const_fn)]
#![feature(abi_efiapi)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

mod allocator;
mod efiutils;
mod logger;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use mythril_core::vm::VmServices;
use mythril_core::*;
use spin::RwLock;
use uefi::prelude::*;

fn default_vm(core: usize, services: &mut impl VmServices) -> Arc<RwLock<vm::VirtualMachine>> {
    let mut config = vm::VirtualMachineConfig::new(vec![core as u8], 1024);

    // FIXME: When `load_image` may return an error, log the error.
    //
    // Map OVMF directly below the 4GB boundary
    config
        .load_image(
            "OVMF.fd".into(),
            memory::GuestPhysAddr::new((4 * 1024 * 1024 * 1024) - (2 * 1024 * 1024)),
        )
        .unwrap_or(());
    config
        .device_map()
        .register_device(device::com::ComDevice::new(core as u64, 0x3F8))
        .unwrap();
    config
        .device_map()
        .register_device(device::com::ComDevice::new(core as u64, 0x402))
        .unwrap(); // The qemu debug port
    config
        .device_map()
        .register_device(device::pci::PciRootComplex::new())
        .unwrap();
    config
        .device_map()
        .register_device(device::pic::Pic8259::new())
        .unwrap();
    config
        .device_map()
        .register_device(device::pit::Pit8254::new())
        .unwrap();
    config
        .device_map()
        .register_device(device::pos::ProgrammableOptionSelect::new())
        .unwrap();
    config
        .device_map()
        .register_device(device::rtc::CmosRtc::new())
        .unwrap();
    config
        .device_map()
        .register_device(device::qemu_fw_cfg::QemuFwCfg::new())
        .unwrap();

    vm::VirtualMachine::new(config, services).expect("Failed to create vm")
}

#[entry]
fn efi_main(handle: Handle, system_table: SystemTable<Boot>) -> Status {
    unsafe { allocator::Allocator::init(system_table.boot_services()) };

    // Currently the logger requires allocation, so wait until here
    log::set_logger(&logger::EfiLogger {})
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .expect("Failed to set logger");

    //TODO: load files from storage (and in the future maybe pci probing, etc.)
    info!("Pre exit");

    let bsp_bt = system_table.boot_services();
    let mut bsp_services = efiutils::EfiVmServices::new(bsp_bt);

    let mut map = BTreeMap::new();
    map.insert(0usize, default_vm(0, &mut bsp_services));
    let map: &'static _ = Box::leak(Box::new(map));

    let mut mem_map = vec![0u8; 1024 * 1024];
    let res = system_table
        .exit_boot_services(handle, &mut mem_map)
        .expect_success("Failed to exit boot services");
    unsafe { allocator::Allocator::allocate_from(res.1) };

    info!("Post exit");

    // // Double box because we need to pass a void* to the EFI AP startup
    // // but Box<dyn Fn> is a fat pointer.
    // efiutils::run_on_all_aps(
    //     bsp_bt,
    //     Box::new(Box::new(move || {
    //         let ap_bt = system_table.boot_services();
    //         let ap_services = efiutils::EfiVmServices::new(ap_bt);
    //         vcpu::smp_entry_point(map, ap_services)
    //     })),
    // )
    // .expect("Failed to start APs");

    vcpu::smp_entry_point(map)
}
