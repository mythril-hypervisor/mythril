use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Default, Debug)]
pub struct Pit8254;

impl Pit8254 {
    const PIT_COUNTER_0: Port = 0x0040;
    const PIT_COUNTER_1: Port = 0x0041;
    const PIT_COUNTER_2: Port = 0x0042;
    const PIT_MODE_CONTROL: Port = 0x0043;

    pub fn new() -> Box<Self> {
        Box::new(Pit8254::default())
    }
}

impl EmulatedDevice for Pit8254 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(
            Self::PIT_COUNTER_0..=Self::PIT_MODE_CONTROL,
        )]
    }

    fn on_port_read(&mut self, port: Port, val: &mut PortIoValue) -> Result<()> {
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        Ok(())
    }
}
