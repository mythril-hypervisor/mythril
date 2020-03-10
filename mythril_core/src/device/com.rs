use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::Result;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::convert::TryInto;

#[derive(Default)]
pub struct ComDevice {
    base_port: Port,
    buff: Vec<u8>,
    divisor: u16,
    interrupt_enable_register: u8,
    interrupt_identification_register: u8,
    line_control_register: u8,
    modem_control_register: u8,
    line_status_register: u8,
    modem_status_register: u8,
    scratch_register: u8,
}

const SEROFF_DATA: u16 = 0;
const SEROFF_DLL: u16 = 0;
const SEROFF_IER: u16 = 1;
const SEROFF_DLH: u16 = 1;
const SEROFF_IIR: u16 = 2;
const SEROFF_LCR: u16 = 3;
const SEROFF_LSR: u16 = 5;
const SEROFF_MSR: u16 = 6;

impl ComDevice {
    pub fn new(base_port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self {
            base_port,
            buff: vec![],

            // For now, transmitter holding register is always empty
            interrupt_identification_register: 0x02,

            ..Default::default()
        })
    }

    fn divisor_latch_bit_set(&self) -> bool {
        self.line_status_register & (1 << 7) != 0
    }
}

impl EmulatedDevice for ComDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.base_port..=self.base_port + 7)]
    }

    fn on_port_read(&mut self, port: Port, val: &mut PortIoValue) -> Result<()> {
        if port - self.base_port == SEROFF_DATA && self.divisor_latch_bit_set() {
            val.copy_from_u32((self.divisor & 0xff).into());
        } else if port - self.base_port == SEROFF_DLL && self.divisor_latch_bit_set() {
            val.copy_from_u32((self.divisor >> 8).into());
        }

        if port - self.base_port == SEROFF_IIR {
            val.copy_from_u32(self.interrupt_identification_register as u32);
        } else if port - self.base_port == SEROFF_IER {
            val.copy_from_u32(self.interrupt_enable_register as u32);
        }

        if port - self.base_port == SEROFF_LSR {
            val.copy_from_u32(0x20);
        }

        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        if port - self.base_port == SEROFF_DATA && self.divisor_latch_bit_set() {
            let val: u8 = val.try_into()?;
            self.divisor &= 0xff00 | val as u16;
        } else if port - self.base_port == SEROFF_DLL && self.divisor_latch_bit_set() {
            let val: u8 = val.try_into()?;
            self.divisor = (self.divisor & 0xff) | (val as u16) << 8;
        }

        if port - self.base_port == SEROFF_IIR {
            self.interrupt_identification_register = val.try_into()?;
        } else if port - self.base_port == SEROFF_IER {
            self.interrupt_enable_register = val.try_into()?;
        }

        let c: u8 = val.try_into()?;
        info!("com on_port_write: 0x{:x}, {}", port - self.base_port, c as char);

        Ok(())
    }
}
