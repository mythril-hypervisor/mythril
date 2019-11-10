use crate::error::{Error, Result};
use crate::memory::GuestPhysAddr;
use alloc::boxed::Box;
use alloc::string::String;
use core::convert::TryInto;

pub mod pci;
pub mod pic;

pub trait EmulatedDevice {
    fn services_address(&self, _addr: GuestPhysAddr) -> bool {
        false
    }
    fn on_mem_read(&mut self, _addr: GuestPhysAddr, _data: &mut [u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "MemoryMapped device does not support reading".into(),
        ))
    }
    fn on_mem_write(&mut self, _addr: GuestPhysAddr, _data: &[u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "MemoryMapped device does not support writing".into(),
        ))
    }
    fn services_port(&self, _port: u16) -> bool {
        false
    }
    fn on_port_read(&mut self, _port: u16, _val: &mut [u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "PortIo device does not support reading".into(),
        ))
    }
    fn on_port_write(&mut self, _port: u16, _val: &[u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "PortIo device does not support writing".into(),
        ))
    }
}

pub struct ComDevice {
    port: u16,
}

impl ComDevice {
    pub fn new(port: u16) -> Box<dyn EmulatedDevice> {
        Box::new(Self { port })
    }
}

impl EmulatedDevice for ComDevice {
    fn services_port(&self, port: u16) -> bool {
        self.port == port
    }

    fn on_port_read(&mut self, _port: u16, val: &mut [u8]) -> Result<()> {
        // This is a magical value (called BOCHS_DEBUG_PORT_MAGIC by edk2)
        // FIXME: this should only be returned for a special 'debug' com device
        val[0] = 0xe9;
        Ok(())
    }

    fn on_port_write(&mut self, _port: u16, val: &[u8]) -> Result<()> {
        // TODO: I'm not sure what the correct behavior is here for a Com device.
        //       For now, just print each byte (except NUL because that crashes)
        let s: String = String::from_utf8_lossy(val)
            .into_owned()
            .chars()
            .filter(|c| *c != (0 as char))
            .collect();
        info!("{}", s);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_memmap_write_to_portio_fails() {
        let mut com = ComDevice::new(0);
        let addr = GuestPhysAddr::new(0);
        assert_eq!(com.on_mem_write(addr, &[0, 0, 0, 0]).is_err(), true);
    }
}
