#![no_std]
#![no_main]
#![feature(asm)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

#[no_mangle]
pub static _fltused: u32 = 0;

use uefi::prelude::*;

mod efialloc;
mod error;
mod registers;
mod memory;
mod vm;
#[allow(dead_code)]
mod vmcs;
mod vmx;

#[entry]
fn efi_main(_handle: Handle, system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&system_table).expect_success("Failed to initialize utilities");

    let mut alloc = efialloc::EfiAllocator::new(system_table.boot_services());

    let mut vmx = vmx::Vmx::enable(&mut alloc).expect("Failed to enable vmx");
    let vmcs = vmcs::Vmcs::new(&mut alloc).expect("Failed to allocate vmcs");
    let vmcs = vmcs.activate(vmx).expect("Failed to activate vmcs");

    use memory::EptPml4Table;
    use x86_64::structures::paging::FrameAllocator;
    let mut ept_pml4_frame = alloc
        .allocate_frame()
        .expect("Failed to allocate pml4 frame");
    let mut ept_pml4 = EptPml4Table::new(&mut ept_pml4_frame).expect("Failed to create pml4 table");

    let mut host_frame = alloc
        .allocate_frame()
        .expect("Failed to allocate host frame");

    use x86_64::VirtAddr;
    memory::map_guest_memory(
        &mut alloc,
        &mut ept_pml4,
        memory::GuestPhysAddr::new(0),
        host_frame,
        false
    ).expect("Failed to map guest physical address");
    info!("We didn't crash!");

    if !memory::map_guest_memory(
        &mut alloc,
        &mut ept_pml4,
        memory::GuestPhysAddr::new(0),
        host_frame,
        false
    ).is_ok() {
        info!("Failed to map page twice (YAY!)");
    } else {
        panic!("Allowed duplicate page mapping")
    }
    loop {}
}
