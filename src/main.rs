#![no_std]
#![no_main]
#![feature(asm)]
#![feature(never_type)]
#![feature(const_fn)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

#[no_mangle]
pub static _fltused: u32 = 0;

use uefi::prelude::*;

mod efialloc;
mod error;
mod memory;
mod percpu;
mod registers;
mod vm;
#[allow(dead_code)]
mod vmcs;
mod vmx;

#[entry]
fn efi_main(_handle: Handle, system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&system_table).expect_success("Failed to initialize utilities");

    let mut alloc = efialloc::EfiAllocator::new(system_table.boot_services());

    let mut vmx = vmx::Vmx::enable(&mut alloc).expect("Failed to enable vmx");

    let config = vm::VirtualMachineConfig::new(memory::GuestPhysAddr::new(0), 1);
    let vm = vm::VirtualMachine::new(&mut vmx, &mut alloc, config).expect("Failed to create vm");

    info!("Constructed VM!");

    info!("addr: 0x{:x}", vmx::vmexit_handler_wrapper as u64);

    //FIXME: Skip launching the vm for now
    vm.launch(vmx).expect("Failed to launch vm");

    loop {}
}
