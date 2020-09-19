use crate::error::Result;
use bitflags::bitflags;
use x86::io::{inb, outb};

// https://en.wikibooks.org/wiki/Serial_Programming/8250_UART_Programming

#[allow(non_snake_case)]
#[allow(dead_code)]
pub mod SerialOffset {
    pub const DATA: u16 = 0;
    pub const DLL: u16 = 0;
    pub const IER: u16 = 1;
    pub const DLH: u16 = 1;
    pub const IIR: u16 = 2;
    pub const LCR: u16 = 3;
    pub const LSR: u16 = 5;
    pub const MSR: u16 = 6;
}

bitflags! {
    pub struct IerFlags: u8 {
        const RECV_DATA_AVAIL_INTERRUPT = 1 << 0;
        const THR_EMPTY_INTERRUPT = 1 << 1;
        const RECEIVER_LINE_STATUS_INTERRUPT = 1 << 2;
        const MODEM_STATUS_INTERRUPT = 1 << 3;
        const SLEEP_MODE = 1 << 4;
        const LOW_POWER_MODE = 1 << 5;
        const RESERVED_1 = 1 << 6;
        const RESERVED_2 = 1 << 7;
    }
}

bitflags! {
    pub struct LsrFlags: u8 {
        const DATA_READY = 1 << 0;
        const OVERRUN_ERROR = 1 << 1;
        const PARITY_ERROR = 1 << 2;
        const FRAMING_ERROR = 1 << 3;
        const BREAK_INTERRUPT = 1 << 4;
        const EMPTY_TRANSMIT_HOLDING_REGISTER = 1 << 5;
        const EMPTY_DATA_HOLDING_REGISTER = 1 << 6;
        const RECV_FIFO_ERROR = 1 << 7;
    }
}

pub struct Uart8250 {
    base: u16,
}

impl Uart8250 {
    pub fn new(base: u16) -> Result<Self> {
        let mut uart = Self { base };
        uart.write_ier(IerFlags::RECV_DATA_AVAIL_INTERRUPT);
        info!("{:?}", uart.read_ier());
        Ok(uart)
    }

    pub fn base_port(&self) -> u16 {
        self.base
    }

    fn read_ier(&self) -> IerFlags {
        IerFlags::from_bits_truncate(unsafe {
            inb(self.base + SerialOffset::IER)
        })
    }

    fn write_ier(&mut self, ier: IerFlags) {
        unsafe {
            outb(self.base + SerialOffset::IER, ier.bits());
        }
    }

    fn read_lsr(&self) -> LsrFlags {
        LsrFlags::from_bits_truncate(unsafe {
            inb(self.base + SerialOffset::LSR)
        })
    }

    pub fn read(&self) -> u8 {
        unsafe { inb(self.base + SerialOffset::DATA) }
    }

    pub fn write(&mut self, data: u8) {
        while !self
            .read_lsr()
            .contains(LsrFlags::EMPTY_TRANSMIT_HOLDING_REGISTER)
        {}
        unsafe {
            outb(self.base + SerialOffset::DATA, data);
        }
    }
}
