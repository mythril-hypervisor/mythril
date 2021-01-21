pub mod idt;

pub mod vector {
    pub const UART: u8 = 36;
    pub const TIMER: u8 = 48;
    pub const IPC: u8 = 49;
}

pub mod gsi {
    pub const PIT: u32 = 0;
    pub const UART: u32 = 4;
}

pub unsafe fn enable_interrupts() {
    asm!("sti", options(nomem, nostack));
}

pub unsafe fn disable_interrupts() {
    asm!("cli", options(nomem, nostack));
}
