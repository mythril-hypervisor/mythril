use crate::error::{self, Error, Result};
use crate::memory::{self, GuestAddressSpace, GuestPhysAddr};
use crate::vmx::ExitReason;
use alloc::boxed::Box;

pub enum EmulatedDevice {
    Mmap(Box<dyn MemoryMappedIoDevice>),
    Port(Box<dyn PortIoDevice>),
}

trait MemoryMappedIoDevice {
    fn start_address(&self) -> GuestPhysAddr;
    fn len(&self) -> usize;
    fn on_read(&mut self, addr: GuestPhysAddr) -> Result<&[u8]>;
    fn on_write(&mut self, addr: GuestPhysAddr, bytes: &[u8]) -> Result<()>;
}

trait PortIoDevice {
    fn port(&self) -> u16;
    fn on_read(&mut self) -> Result<u32>;
    fn on_write(&mut self, val: &[u8]) -> Result<()>;
}

pub struct ComDevice {
    port: u16,
}

impl ComDevice {
    pub fn new(port: u16) -> EmulatedDevice {
        EmulatedDevice::Port(Box::new(Self { port }))
    }
}

impl PortIoDevice for ComDevice {
    fn port(&self) -> u16 {
        self.port
    }

    fn on_read(&mut self) -> Result<u32> {
        Ok(0)
    }

    fn on_write(&mut self, val: &[u8]) -> Result<()> {
        for v in val {
            info!("{}", *v as char);
        }
        Ok(())
    }
}
