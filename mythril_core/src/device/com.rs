use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::Result;
use alloc::boxed::Box;
use alloc::vec::Vec;

pub struct ComDevice {
    port: Port,
}

impl ComDevice {
    pub fn new(port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self { port })
    }
}

impl EmulatedDevice for ComDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port + 7)]
    }

    fn on_port_read(&mut self, _port: Port, _val: &mut PortIoValue) -> Result<()> {
        Ok(())
    }

    fn on_port_write(&mut self, _port: Port, _val: PortIoValue) -> Result<()> {
        Ok(())
    }
}
