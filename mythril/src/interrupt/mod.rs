pub mod idt;

pub unsafe fn enable_interrupts() {
    llvm_asm!("sti" :::: "volatile");
}

pub unsafe fn disable_interrupts() {
    llvm_asm!("cli" :::: "volatile");
}
