use crate::device::{DeviceRegion, EmulatedDevice, Port};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub struct ComDevice {
    port: Port,
    buff: Vec<u8>,
}

impl ComDevice {
    pub fn new(port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self { port, buff: vec![] })
    }
}

impl EmulatedDevice for ComDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_port_read(&mut self, _port: Port, val: &mut [u8]) -> Result<()> {
        // This is a magical value (called BOCHS_DEBUG_PORT_MAGIC by edk2)
        // FIXME: this should only be returned for a special 'debug' com device
        val[0] = 0xe9;
        Ok(())
    }

    fn on_port_write(&mut self, _port: Port, val: &[u8]) -> Result<()> {
        self.buff.extend_from_slice(val);

        // Flush on newlines
        if val.iter().filter(|b| **b == 10).next().is_some() {
            // TODO: I'm not sure what the correct behavior is here for a Com device.
            //       For now, just print each byte (except NUL because that crashes)
            let s: String = String::from_utf8_lossy(&self.buff)
                .into_owned()
                .chars()
                .filter(|c| *c != (0 as char))
                .collect();

            // FIXME: for now print guest output with some newlines to make
            //        it a bit more visible
            info!("GUEST: {}", s);
            self.buff.clear();
        }

        Ok(())
    }
}
