use crate::error::{Error, Result};
use crate::memory::GuestPhysAddr;
use crate::virtdev::{DeviceEvent, DeviceRegion};
use crate::virtdev::{EmulatedDevice, Event};
use crate::virtdev::{MemReadRequest, MemWriteRequest};
use alloc::vec::Vec;

use byteorder::{ByteOrder, NativeEndian};

#[derive(Debug)]
enum Request<'a> {
    Read(u8, MemReadRequest<'a>),
    Write(u8, MemWriteRequest<'a>),
}

#[derive(Clone, Copy, Debug)]
enum RequestState {
    Pending,
    RegSel(u8),
}

impl RequestState {
    fn reset(&mut self) {
        *self = RequestState::Pending;
    }
}

impl Default for RequestState {
    fn default() -> RequestState {
        RequestState::Pending
    }
}

#[derive(Default)]
pub struct IoApic {
    state: RequestState,
    version: u32,
}

impl IoApic {
    pub fn new() -> Result<Self> {
        Ok(IoApic {
            state: RequestState::Pending,
            version: 0x11,
        })
    }
}

impl EmulatedDevice for IoApic {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::MemIo(
            GuestPhysAddr::new(0xfec00000)..=GuestPhysAddr::new(0xfec010f0),
        )]
    }

    fn on_event<'a>(&mut self, event: Event) -> Result<()> {
        // Parse the event into a register request based on the address given,
        // input, and the current request state.
        let mut req = match self.state {
            RequestState::Pending => match event.kind {
                DeviceEvent::MemWrite(addr, val)
                    if addr.as_u64() == 0xfec00000 =>
                {
                    if val.as_slice().len() != 4 {
                        Err(Error::NotSupported)
                    } else {
                        let reg = NativeEndian::read_u32(val.as_slice());
                        self.state = RequestState::RegSel(reg as u8);
                        return Ok(());
                    }
                }
                _ => Err(Error::NotSupported),
            },
            RequestState::RegSel(reg) => match event.kind {
                DeviceEvent::MemWrite(addr, val)
                    if addr.as_u64() == 0xfec00010 =>
                {
                    if val.as_slice().len() != 4 {
                        Err(Error::NotSupported)
                    } else {
                        self.state.reset();
                        Ok(Request::Write(reg, val))
                    }
                }
                DeviceEvent::MemRead(addr, val)
                    if addr.as_u64() == 0xfec00010 =>
                {
                    if val.as_slice().len() != 4 {
                        Err(Error::NotSupported)
                    } else {
                        self.state.reset();
                        Ok(Request::Read(reg, val))
                    }
                }
                _ => Err(Error::NotSupported),
            },
        }?;

        info!("I/O APIC Request {:?}", req);
        match req {
            Request::Read(0, ref mut data) => {
                NativeEndian::write_u32(data.as_mut_slice(), self.version);
                Ok(())
            }
            Request::Write(0, ref data) => {
                self.version = NativeEndian::read_u32(data.as_slice());
                Ok(())
            }
            _ => {
                info!("I/O APIC Unsupported Request: {:?}", req);
                Err(Error::NotSupported)
            }
        }
    }
}
