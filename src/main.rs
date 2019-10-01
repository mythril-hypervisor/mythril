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
use x86_64::registers::model_specific::Msr;

mod efialloc;
mod error;
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
    let vmcs = vmcs.activate(&mut vmx).expect("Failed to activate vmcs");

    const IA32_VMX_CR0_FIXED0_MSR: u32 = 0x486;
    const IA32_VMX_CR4_FIXED0_MSR: u32 = 0x488;
    let cr0_fixed = Msr::new(IA32_VMX_CR0_FIXED0_MSR);
    let cr4_fixed = Msr::new(IA32_VMX_CR4_FIXED0_MSR);

    let (new_cr0, new_cr4) = unsafe {
        use x86_64::registers::control::Cr0;

        //FIXME: x86_64 does not currently support cr4, so asm here
        let mut current_cr4: u64;
        asm!("movq %cr4, %rax;"
             : "={rax}"(current_cr4)
             ::: "volatile");

        (
            cr0_fixed.read() | Cr0::read().bits(),
            cr4_fixed.read() | current_cr4,
        )
    };

    vmcs.write_field(vmcs::HOST_CR0 as u64, new_cr0).unwrap();
    vmcs.write_field(vmcs::HOST_CR4 as u64, new_cr4).unwrap();

    vmcs.write_field(vmcs::HOST_ES_SELECTOR as u64, 0x10)
        .unwrap();
    let var = vmcs.read_field(vmcs::HOST_ES_SELECTOR as u64).unwrap();

    info!("{}", var);

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
        memory::GuestPhysAddr(VirtAddr::new(0)),
        host_frame,
    ).expect("Failed to map guest physical address");
    info!("We didn't crash!");

    memory::map_guest_memory(
        &mut alloc,
        &mut ept_pml4,
        memory::GuestPhysAddr(VirtAddr::new(0)),
        host_frame,
    )
    .expect("Failed to map page twice (YAY!)");
    loop {}
}
