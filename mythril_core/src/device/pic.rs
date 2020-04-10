use crate::device::{
    DeviceRegion, EmulatedDevice, Port, PortReadRequest, PortWriteRequest,
};
use crate::error::{Error, Result};
use crate::memory::GuestAddressSpace;
use crate::vcpu::VCpu;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::convert::TryInto;

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

    pub fn new() -> Box<Self> {
        Box::new(Pic8259::default())
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

    fn on_port_read(
        &mut self,
        _vcpu: &VCpu,
        port: Port,
        mut val: PortReadRequest,
        _space: &mut GuestAddressSpace,
    ) -> Result<()> {
        let data = match port {
            Self::PIC_MASTER_DATA => self.master_state.imr,
            Self::PIC_SLAVE_DATA => self.master_state.imr,
            _ => {
                return Err(Error::NotImplemented(
                    "Read of PIC command port not yet supported".into(),
                ))
            }
        };
        val.copy_from_u32(data as u32);
        Ok(())
    }

    fn on_port_write(
        &mut self,
        _vcpu: &VCpu,
        port: Port,
        val: PortWriteRequest,
        _space: &mut GuestAddressSpace,
    ) -> Result<()> {
        match port {
            Self::PIC_MASTER_DATA => {
                info!("Set master PIC data: {}", val);
                self.master_state.imr = val.try_into()?;
            }
            Self::PIC_SLAVE_DATA => {
                info!("Set slave PIC data: {}", val);
                self.master_state.imr = val.try_into()?;
            }
            port => {
                info!(
                    "Write to PIC command port not yet supported (port 0x{:x} = {})",
                    port, val
                );
                // return Err(Error::NotImplemented(
                //     format!("Write to PIC command port not yet supported (port 0x{:x} = {:?})", port, val),
                // ))
            }
        }
        Ok(())
    }
}
