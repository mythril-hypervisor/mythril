use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub struct ComDevice {
    id: u64,
    port: Port,
    buff: Vec<u8>,
}

impl ComDevice {
    pub fn new(vmid: u64, port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self {
            port,
            buff: vec![],
            id: vmid,
        })
    }
}

impl EmulatedDevice for ComDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_port_read(&mut self, _port: Port, val: &mut PortIoValue) -> Result<()> {
        // This is a magical value (called BOCHS_DEBUG_PORT_MAGIC by edk2)
        // FIXME: this should only be returned for a special 'debug' com device
        *val = 0xe9u8.into();
        Ok(())
    }

    fn on_port_write(&mut self, _port: Port, val: PortIoValue) -> Result<()> {
        self.buff.extend_from_slice(val.as_slice());

        // Flush on newlines
        if val.as_slice().iter().filter(|b| **b == 10).next().is_some() {
            // TODO: I'm not sure what the correct behavior is here for a Com device.
            //       For now, just print each byte (except NUL because that crashes)
            let s: String = String::from_utf8_lossy(&self.buff)
                .into_owned()
                .chars()
                .filter(|c| *c != (0 as char))
                .collect();

            // FIXME: for now print guest output with some newlines to make
            //        it a bit more visible
            info!("GUEST{}: {}", self.id, s);
            self.buff.clear();
        }

        Ok(())
    }
}
