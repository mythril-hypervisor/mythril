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

use mythril_core::vm::VmServices;
use mythril_core::*;
use uefi::prelude::*;
mod efiutils;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::RwLock;

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
        .register_device(device::com::ComDevice::new(0x3F8))
        .unwrap();
    config
        .device_map()
        .register_device(device::com::ComDevice::new(0x402))
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
fn efi_main(_handle: Handle, system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&system_table).expect_success("Failed to initialize utilities");

    let system_table: &'static _ = Box::leak(Box::new(system_table));
    let bsp_bt = system_table.boot_services();
    let mut bsp_services = efiutils::EfiVmServices::new(bsp_bt);

    let mut map = BTreeMap::new();
    map.insert(0usize, default_vm(0, &mut bsp_services));
    map.insert(1usize, default_vm(1, &mut bsp_services));
    let map: &'static _ = Box::leak(Box::new(map));

    // Double box because we need to pass a void* to the EFI AP startup
    // but Box<dyn Fn> is a fat pointer.
    efiutils::run_on_all_aps(
        bsp_bt,
        Box::new(Box::new(move || {
            let ap_bt = system_table.boot_services();
            let ap_services = efiutils::EfiVmServices::new(ap_bt);
            vcpu::smp_entry_point(map, ap_services)
        })),
    )
    .expect("Failed to start APs");

    vcpu::smp_entry_point(map, bsp_services)
}
