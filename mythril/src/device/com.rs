use crate::device::{
    DeviceRegion, EmulatedDevice, InterruptArray, Port, PortReadRequest,
    PortWriteRequest,
};
use crate::error::Result;
use crate::logger;
use crate::memory::GuestAddressSpaceViewMut;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::convert::TryInto;

#[derive(Default)]
pub struct ComDevice {
    id: u64,
    base_port: Port,
    buff: Vec<u8>,
    divisor: u16,
    interrupt_enable_register: u8,
    interrupt_identification_register: u8,
    _line_control_register: u8,
    _modem_control_register: u8,
    line_status_register: u8,
    _modem_status_register: u8,
    _scratch_register: u8,
}

#[allow(non_snake_case)]
#[allow(dead_code)]
mod SerialOffset {
    pub const DATA: u16 = 0;
    pub const DLL: u16 = 0;
    pub const IER: u16 = 1;
    pub const DLH: u16 = 1;
    pub const IIR: u16 = 2;
    pub const LCR: u16 = 3;
    pub const LSR: u16 = 5;
    pub const MSR: u16 = 6;
}

impl ComDevice {
    pub fn new(vmid: u64, base_port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self {
            id: vmid,
            base_port,
            buff: vec![],

            interrupt_identification_register: 0x01,

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

    fn on_port_read(
        &mut self,
        port: Port,
        mut val: PortReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<InterruptArray> {
        if port - self.base_port == SerialOffset::DATA
            && self.divisor_latch_bit_set()
        {
            val.copy_from_u32((self.divisor & 0xff).into());
        } else if port - self.base_port == SerialOffset::DLL
            && self.divisor_latch_bit_set()
        {
            val.copy_from_u32((self.divisor >> 8).into());
        }

        if port - self.base_port == SerialOffset::IIR {
            val.copy_from_u32(self.interrupt_identification_register as u32);

            // Reading the IIR clears it (the LSB = 1 indicates there is _not_ a
            // pending interrupt)
            self.interrupt_identification_register = 1;
        } else if port - self.base_port == SerialOffset::IER {
            val.copy_from_u32(self.interrupt_enable_register as u32);
        }

        if port - self.base_port == SerialOffset::LSR {
            val.copy_from_u32(0x60);
        }
        Ok(InterruptArray::default())
    }

    fn on_port_write(
        &mut self,
        port: Port,
        val: PortWriteRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<InterruptArray> {
        let val: u8 = val.try_into()?;
        if port - self.base_port == SerialOffset::DATA {
            if self.divisor_latch_bit_set() {
                self.divisor &= 0xff00 | val as u16;
            } else {
                self.buff.push(val);
                if val == 10 {
                    let s = String::from_utf8_lossy(&self.buff);
                    logger::write_console(&format!("GUEST{}: {}", self.id, s));
                    self.buff.clear();
                }
                let mut arr = InterruptArray::default();
                if self.interrupt_enable_register & 0b00000010 == 0b10 {
                    arr.push(52);
                }
                self.interrupt_identification_register = 0b10;
                return Ok(arr);
            }
        } else if port - self.base_port == SerialOffset::DLL
            && self.divisor_latch_bit_set()
        {
            self.divisor = (self.divisor & 0xff) | (val as u16) << 8;
        }

        if port - self.base_port == SerialOffset::IIR {
            self.interrupt_identification_register = val;
        } else if port - self.base_port == SerialOffset::IER {
            self.interrupt_enable_register = val;
        }

        Ok(InterruptArray::default())
    }
}
