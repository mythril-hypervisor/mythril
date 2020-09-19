use crate::error::Result;
use crate::logger;
use crate::memory::GuestAddressSpaceViewMut;
use crate::physdev::com::*;
use crate::virtdev::{
    DeviceMessage, DeviceRegion, EmulatedDevice, InterruptArray, Port,
    PortReadRequest, PortWriteRequest,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryInto;
use spin::RwLock;

pub struct Uart8250 {
    id: u64,
    base_port: Port,
    is_newline: bool,
    divisor: u16,
    receive_buffer: Option<u8>,
    interrupt_enable_register: IerFlags,
    interrupt_identification_register: u8,
    _line_control_register: u8,
    _modem_control_register: u8,
    _line_status_register: LsrFlags,
    _modem_status_register: u8,
    _scratch_register: u8,
}

impl Uart8250 {
    pub fn new(vmid: u64, base_port: Port) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            id: vmid,
            base_port: base_port,
            divisor: 0,
            is_newline: true,
            receive_buffer: None,
            interrupt_identification_register: 0x01,
            interrupt_enable_register: IerFlags::empty(),
            _line_control_register: 0,
            _modem_control_register: 0,
            _line_status_register: LsrFlags::empty(),
            _modem_status_register: 0,
            _scratch_register: 0,
        }))
    }

    fn divisor_latch_bit_set(&self) -> bool {
        self._line_control_register & (1 << 7) != 0
    }

    /// Insert a byte to the receive buffer of the UART. This will be
    /// _read_ by the VM.
    pub fn write(&mut self, data: u8) {
        self.receive_buffer = Some(data);
        self.interrupt_identification_register = 0b100;
    }
}

impl EmulatedDevice for Uart8250 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.base_port..=self.base_port + 7)]
    }

    fn on_event(
        &mut self,
        event: &DeviceMessage,
        _space: GuestAddressSpaceViewMut,
        interrupts: &mut InterruptArray,
    ) -> Result<()> {
        match event {
            DeviceMessage::UartKeyPressed(key) => {
                interrupts.push(52);
                self.write(*key)
            }
        }

        Ok(())
    }

    fn on_port_read(
        &mut self,
        port: Port,
        mut val: PortReadRequest,
        _space: GuestAddressSpaceViewMut,
        _interrupts: &mut InterruptArray,
    ) -> Result<()> {
        if port - self.base_port == SerialOffset::DATA
            && !self.divisor_latch_bit_set()
        {
            if let Some(receive_buffer) = self.receive_buffer {
                val.copy_from_u32(receive_buffer.into());
                self.receive_buffer = None;
                self.interrupt_identification_register = 1;
            }
        } else if port - self.base_port == SerialOffset::DATA
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
            val.copy_from_u32(self.interrupt_enable_register.bits() as u32);
        }

        if port - self.base_port == SerialOffset::LSR {
            let mut flags = LsrFlags::EMPTY_TRANSMIT_HOLDING_REGISTER
                | LsrFlags::EMPTY_DATA_HOLDING_REGISTER;

            if self.receive_buffer.is_some() {
                flags.insert(LsrFlags::DATA_READY);
            }

            val.copy_from_u32(flags.bits() as u32);
        }
        Ok(())
    }

    fn on_port_write(
        &mut self,
        port: Port,
        val: PortWriteRequest,
        _space: GuestAddressSpaceViewMut,
        interrupts: &mut InterruptArray,
    ) -> Result<()> {
        let val: u8 = val.try_into()?;
        if port - self.base_port == SerialOffset::DATA {
            if self.divisor_latch_bit_set() {
                self.divisor &= 0xff00 | val as u16;
            } else {
                if self.is_newline {
                    logger::write_console(&format!("GUEST{}: ", self.id));
                }

                let buff = &[val];
                let s = String::from_utf8_lossy(buff);
                logger::write_console(&s);

                self.is_newline = val == 10;

                if self
                    .interrupt_enable_register
                    .contains(IerFlags::THR_EMPTY_INTERRUPT)
                {
                    interrupts.push(52);
                }
                self.interrupt_identification_register = 0b10;
            }
        } else if port - self.base_port == SerialOffset::DLL
            && self.divisor_latch_bit_set()
        {
            self.divisor = (self.divisor & 0xff) | (val as u16) << 8;
        }

        if port - self.base_port == SerialOffset::IER {
            self.interrupt_enable_register = IerFlags::from_bits_truncate(val);
        }

        Ok(())
    }
}
