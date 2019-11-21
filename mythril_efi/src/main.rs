#![no_std]
#![no_main]
#![feature(asm)]
#![feature(never_type)]
#![feature(const_fn)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

use mythril_core::vm::VmServices;
use mythril_core::*;
use uefi::prelude::*;
mod efiutils;

#[entry]
fn efi_main(_handle: Handle, system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&system_table).expect_success("Failed to initialize utilities");

    let mut services = efiutils::EfiVmServices::new(system_table.boot_services());

    let mut vmx = vmx::Vmx::enable(services.allocator()).expect("Failed to enable vmx");

    let mut config = vm::VirtualMachineConfig::new(1024);

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

    let vm = vm::VirtualMachine::new(&mut vmx, config, &mut services).expect("Failed to create vm");

    info!("Constructed VM!");

    vm.launch(vmx).expect("Failed to launch vm");
}
