use crate::error::Result;
use crate::interrupt;
use crate::physdev::com::*;
use crate::virtdev::{
    DeviceEvent, DeviceEventResponse, DeviceRegion, EmulatedDevice, Event, Port,
};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryInto;
use spin::RwLock;

pub struct Uart8250 {
    base_port: Port,
    divisor: u16,
    receive_buffer: Option<u8>,
    interrupt_enable_register: IerFlags,
    interrupt_identification_register: u8,
    _line_control_register: u8,
    _modem_control_register: u8,
    _line_status_register: LsrFlags,
    _modem_status_register: u8,
    _scratch_register: u8,

    ctrl_a_count: u8,
}

impl Uart8250 {
    pub fn new(base_port: Port) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            base_port: base_port,
            divisor: 0,
            receive_buffer: None,
            interrupt_identification_register: 0x01,
            interrupt_enable_register: IerFlags::empty(),
            _line_control_register: 0,
            _modem_control_register: 0,
            _line_status_register: LsrFlags::empty(),
            _modem_status_register: 0,
            _scratch_register: 0,
            ctrl_a_count: 0,
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

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::HostUartReceived(key) => {
                event
                    .responses
                    .push(DeviceEventResponse::GSI(interrupt::gsi::UART));
                if key == 0x01 {
                    // ctrl+a
                    self.ctrl_a_count += 1;
                }
                if self.ctrl_a_count == 3 {
                    event.responses.push(DeviceEventResponse::NextConsole);
                    self.ctrl_a_count = 0;
                }
                self.write(key)
            }
            DeviceEvent::PortRead(port, mut val) => {
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
                    val.copy_from_u32(
                        self.interrupt_identification_register as u32,
                    );

                    // Reading the IIR clears it (the LSB = 1 indicates there is _not_ a
                    // pending interrupt)
                    self.interrupt_identification_register = 1;
                } else if port - self.base_port == SerialOffset::IER {
                    val.copy_from_u32(
                        self.interrupt_enable_register.bits() as u32
                    );
                }

                if port - self.base_port == SerialOffset::LSR {
                    let mut flags = LsrFlags::EMPTY_TRANSMIT_HOLDING_REGISTER
                        | LsrFlags::EMPTY_DATA_HOLDING_REGISTER;

                    if self.receive_buffer.is_some() {
                        flags.insert(LsrFlags::DATA_READY);
                    }

                    val.copy_from_u32(flags.bits() as u32);
                }
            }
            DeviceEvent::PortWrite(port, val) => {
                let val: u8 = val.try_into()?;
                if port - self.base_port == SerialOffset::DATA {
                    if self.divisor_latch_bit_set() {
                        self.divisor &= 0xff00 | val as u16;
                    } else {
                        event.responses.push(
                            DeviceEventResponse::GuestUartTransmitted(val),
                        );

                        if self
                            .interrupt_enable_register
                            .contains(IerFlags::THR_EMPTY_INTERRUPT)
                        {
                            event.responses.push(DeviceEventResponse::GSI(
                                interrupt::gsi::UART,
                            ));
                        }
                        self.interrupt_identification_register = 0b10;
                    }
                } else if port - self.base_port == SerialOffset::DLL
                    && self.divisor_latch_bit_set()
                {
                    self.divisor = (self.divisor & 0xff) | (val as u16) << 8;
                }

                if port - self.base_port == SerialOffset::IER {
                    self.interrupt_enable_register =
                        IerFlags::from_bits_truncate(val);
                }
            }
            _ => (),
        }
        Ok(())
    }
}
