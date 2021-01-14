pub mod idt;

pub const UART_VECTOR: u8 = 36;
pub const TIMER_VECTOR: u8 = 48;
pub const IPC_VECTOR: u8 = 49;

pub unsafe fn enable_interrupts() {
    asm!("sti");
}

pub unsafe fn disable_interrupts() {
    asm!("cli");
}
