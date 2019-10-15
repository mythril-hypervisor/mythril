use crate::error::{self, Error, Result};
use crate::memory::{self, GuestAddressSpace, GuestPhysAddr};

trait MemoryMappedIoDevice {
    fn start_address(&self) -> GuestPhysAddr;
    fn len(&self) -> usize;
    fn on_read(&self, addr: GuestPhysAddr) -> Result<&[u8]>;
    fn on_write(&mut self, addr: GuestPhysAddr, bytes: &[u8]) -> Result<()>;
}

trait PortIoDevice {
    fn port(&self) -> u16;
    fn on_read(&self) -> Result<u32>;
    fn on_write(&mut self, val: &[u8]) -> Result<()>;
}

struct ComDevice {
    port: u16
}

impl PortIoDevice for ComDevice {
    fn port(&self) -> u16 {
        self.port
    }

    fn on_read(&self) -> Result<u32> {
        Ok(0)
    }

    fn on_write(&mut self, val: &[u8]) -> Result<()> {
        for v in val {
            info!("{}", *v as char);
        }
        Ok(())
    }
}
