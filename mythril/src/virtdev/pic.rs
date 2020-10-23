use crate::error::Result;
use crate::virtdev::{DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryInto;
use spin::RwLock;

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
    const PIC_MASTER_COMMAND: Port = 0x0020;
    const PIC_MASTER_DATA: Port = Self::PIC_MASTER_COMMAND + 1;
    const PIC_SLAVE_COMMAND: Port = 0x00a0;
    const PIC_SLAVE_DATA: Port = Self::PIC_SLAVE_COMMAND + 1;
    const PIC_ECLR_COMMAND: Port = 0x4d0;
    const PIC_ECLR_DATA: Port = Self::PIC_ECLR_COMMAND + 1;

    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Pic8259::default()))
    }
}

impl EmulatedDevice for Pic8259 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(
                Self::PIC_MASTER_COMMAND..=Self::PIC_MASTER_DATA,
            ),
            DeviceRegion::PortIo(
                Self::PIC_SLAVE_COMMAND..=Self::PIC_SLAVE_DATA,
            ),
            DeviceRegion::PortIo(Self::PIC_ECLR_COMMAND..=Self::PIC_ECLR_DATA),
        ]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(port, mut val) => {
                let data = match port {
                    Self::PIC_MASTER_DATA => self.master_state.imr,
                    Self::PIC_SLAVE_DATA => self.master_state.imr,
                    _ => {
                        return Ok(());
                    }
                };
                val.copy_from_u32(data as u32);
            }
            DeviceEvent::PortWrite(port, val) => match port {
                Self::PIC_MASTER_DATA => {
                    self.master_state.imr = val.try_into()?;
                }
                Self::PIC_SLAVE_DATA => {
                    self.master_state.imr = val.try_into()?;
                }
                _ => (),
            },
            _ => (),
        }
        Ok(())
    }
}
