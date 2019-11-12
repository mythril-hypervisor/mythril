use crate::device::{DeviceRegion, EmulatedDevice};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Default, Debug)]
pub struct PicState {
    imr: u8,
}

#[derive(Default, Debug)]
pub struct Pic8259 {
    master_state: PicState,
    slave_state: PicState,
}

impl Pic8259 {
    const PIC_MASTER_COMMAND: u16 = 0x0020;
    const PIC_MASTER_DATA: u16 = Self::PIC_MASTER_COMMAND + 1;
    const PIC_SLAVE_COMMAND: u16 = 0x00a0;
    const PIC_SLAVE_DATA: u16 = Self::PIC_SLAVE_COMMAND + 1;

    pub fn new() -> Box<Self> {
        Box::new(Pic8259::default())
    }
}

impl EmulatedDevice for Pic8259 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::PIC_MASTER_COMMAND..=Self::PIC_MASTER_DATA),
            DeviceRegion::PortIo(Self::PIC_SLAVE_COMMAND..=Self::PIC_SLAVE_DATA),
        ]
    }

    fn on_port_read(&mut self, port: u16, val: &mut [u8]) -> Result<()> {
        let data = match port {
            Self::PIC_MASTER_DATA => self.master_state.imr,
            Self::PIC_SLAVE_DATA => self.master_state.imr,
            _ => {
                return Err(Error::NotImplemented(
                    "Read of PIC command port not yet supported".into(),
                ))
            }
        }
        .to_be_bytes();
        val.copy_from_slice(&data);
        Ok(())
    }

    fn on_port_write(&mut self, port: u16, val: &[u8]) -> Result<()> {
        match port {
            Self::PIC_MASTER_DATA => self.master_state.imr = val[0],
            Self::PIC_SLAVE_DATA => self.master_state.imr = val[0],
            _ => {
                return Err(Error::NotImplemented(
                    "Write to PIC command port not yet supported".into(),
                ))
            }
        }
        Ok(())
    }
}
