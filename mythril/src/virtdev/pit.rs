use crate::error::{Error, Result};
use crate::interrupt;
use crate::physdev::pit::*;
use crate::time;
use crate::virtdev::{
    DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port, PortReadRequest,
    PortWriteRequest,
};

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryFrom;
use spin::RwLock;

#[derive(Debug)]
enum OperatingModeState {
    Mode0 {
        start_counter: Option<u16>,
        timer: Option<time::TimerId>,
        start_time: Option<time::Instant>,
    },
    Mode2 {
        start_counter: Option<u16>,
        timer: Option<time::TimerId>,
        start_time: Option<time::Instant>,
    },
}

#[derive(Debug)]
enum AccessModeState {
    LatchCount,
    LoByte,
    HiByte,
    Word { lo_byte: Option<u8> },
}

#[derive(Debug)]
struct ChannelState {
    mode: OperatingModeState,
    access: AccessModeState,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            mode: OperatingModeState::Mode0 {
                start_counter: None,
                timer: None,
                start_time: None,
            },
            access: AccessModeState::LoByte,
        }
    }
}

#[derive(Default, Debug)]
pub struct Pit8254 {
    channel0: ChannelState,
    // channel1 is not supported
    channel2: ChannelState,
}

impl Pit8254 {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Pit8254::default()))
    }

    fn on_port_read(
        &mut self,
        port: Port,
        mut val: PortReadRequest,
    ) -> Result<()> {
        match port {
            //FIXME: much of this 'PS2' handling is a hack. I'm not aware of
            // a good source for exactly what's supposed to happen here.
            PIT_PS2_CTRL_B => {
                if let OperatingModeState::Mode0 {
                    start_time,
                    start_counter,
                    ..
                } = self.channel2.mode
                {
                    match (start_time, start_counter) {
                        (Some(start_time), Some(start_counter)) => {
                            let duration = time::now() - start_time;
                            let ticks =
                                duration.as_nanos() / PIT_NS_PER_TICK as u128;
                            if ticks as u16 > start_counter {
                                val.copy_from_u32(1 << 5);
                            }
                        }
                        _ => (),
                    }
                }
            }
            _ => {
                info!("PIT read from unsupported port");
            }
        }
        Ok(())
    }

    fn on_port_write(
        &mut self,
        port: Port,
        val: PortWriteRequest,
    ) -> Result<()> {
        match port {
            PIT_MODE_CONTROL => {
                let val = u8::try_from(val)?;
                let channel = (0b11000000 & val) >> 6;
                let access = (0b00110000 & val) >> 4;
                let operating = (0b00001110 & val) >> 1;

                if val & 0b00000001 != 0 {
                    return Err(Error::InvalidValue(
                        "PIT BCD mode is not supported".into(),
                    ));
                }

                let operating_state = match operating {
                    0b000 => OperatingModeState::Mode0 {
                        start_counter: None,
                        timer: None,
                        start_time: None,
                    },
                    0b010 => OperatingModeState::Mode2 {
                        start_counter: None,
                        timer: None,
                        start_time: None,
                    },
                    value => {
                        return Err(Error::InvalidValue(format!(
                            "Invalid PIT operating state '0x{:x}'",
                            value
                        )))
                    }
                };

                let access_state = match access {
                    0b00 => AccessModeState::LatchCount,
                    0b01 => AccessModeState::LoByte,
                    0b10 => AccessModeState::HiByte,
                    0b11 => AccessModeState::Word { lo_byte: None },
                    value => {
                        return Err(Error::InvalidValue(format!(
                            "Invalid PIT access state '0x{:x}'",
                            value
                        )))
                    }
                };

                let channel_state = ChannelState {
                    mode: operating_state,
                    access: access_state,
                };

                let current_channel = match channel {
                    0b00 => &mut self.channel0,
                    0b10 => &mut self.channel2,
                    value => {
                        return Err(Error::InvalidValue(format!(
                            "Invalid PIT channel '0x{:x}'",
                            value
                        )))
                    }
                };

                // Stop any running timers
                match &current_channel.mode {
                    OperatingModeState::Mode0 { ref timer, .. } => timer,
                    OperatingModeState::Mode2 { ref timer, .. } => timer,
                }
                .as_ref()
                .map(|id| time::cancel_timer(id));

                *current_channel = channel_state;
            }
            port @ PIT_COUNTER_0..=PIT_COUNTER_2 => {
                let val = u8::try_from(val)?;
                let channel_state = match port {
                    PIT_COUNTER_0 => &mut self.channel0,
                    PIT_COUNTER_1 => {
                        return Err(Error::InvalidValue(format!(
                            "Invalid PIT port '0x{:x}'",
                            port
                        )))
                    }
                    PIT_COUNTER_2 => &mut self.channel2,
                    _ => unreachable!(),
                };

                let counter = match channel_state.access {
                    AccessModeState::LoByte => val as u16,
                    AccessModeState::HiByte => (val as u16) << 8,
                    AccessModeState::Word { ref mut lo_byte } => {
                        if let Some(lo_byte) = lo_byte {
                            ((val as u16) << 8) | (*lo_byte as u16)
                        } else {
                            // We are just setting the low byte in word access mode.
                            // There is nothing else to do, so return.
                            *lo_byte = Some(val);
                            return Ok(());
                        }
                    }
                    _ => unreachable!(),
                };

                if counter == 0 {
                    warn!("PIT: ignoring counter set to 0");
                    return Ok(());
                }

                let duration = core::time::Duration::from_nanos(
                    PIT_NS_PER_TICK * counter as u64,
                );

                match channel_state.mode {
                    OperatingModeState::Mode0 {
                        ref mut start_counter,
                        ref mut timer,
                        ref mut start_time,
                    } => {
                        *start_counter = Some(counter);
                        *start_time = Some(time::now());

                        // Only channel 0 produces timer interrupts
                        if port == PIT_COUNTER_0 {
                            *timer = Some(time::set_oneshot_timer(
                                duration,
                                time::TimerInterruptType::GSI(
                                    interrupt::gsi::PIT,
                                ),
                            ));
                        }
                    }

                    OperatingModeState::Mode2 {
                        ref mut start_counter,
                        ref mut timer,
                        ref mut start_time,
                    } => {
                        *start_counter = Some(counter);
                        *start_time = Some(time::now());

                        if port == PIT_COUNTER_0 {
                            *timer = Some(time::set_periodic_timer(
                                duration,
                                time::TimerInterruptType::GSI(
                                    interrupt::gsi::PIT,
                                ),
                            ));
                        }
                    }
                };
            }
            _ => {
                info!("PIT: write to unsupported port: 0x{:x}", port);
            }
        }

        Ok(())
    }
}

impl EmulatedDevice for Pit8254 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(PIT_COUNTER_0..=PIT_MODE_CONTROL),
            DeviceRegion::PortIo(PIT_PS2_CTRL_B..=PIT_PS2_CTRL_B),
        ]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(port, val) => self.on_port_read(port, val)?,
            DeviceEvent::PortWrite(port, val) => {
                self.on_port_write(port, val)?
            }
            _ => (),
        }
        Ok(())
    }
}
