use crate::device::{DeviceRegion, EmulatedDevice, Port};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::array::FixedSizeArray;
use core::convert::TryInto;
use derive_try_from_primitive::TryFromPrimitive;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
enum FwCfgSelector {
    Signature = 0x0000,
    Id = 0x0001,
    FileDir = 0x0019,
}

#[derive(Debug)]
pub struct QemuFwCfg {
    selector: FwCfgSelector,
}

impl QemuFwCfg {
    const FW_CFG_PORT_SEL: Port = 0x510;
    const FW_CFG_PORT_DATA: Port = 0x511;
    const FW_CFG_PORT_DMA: Port = 0x514;

    pub fn new() -> Box<Self> {
        Box::new(Self {
            selector: FwCfgSelector::Signature,
        })
    }
}

impl EmulatedDevice for QemuFwCfg {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::FW_CFG_PORT_SEL..=Self::FW_CFG_PORT_DATA), // No Support for DMA right now
        ]
    }

    fn on_port_read(&mut self, port: Port, val: &mut [u8]) -> Result<()> {
        match port {
            Self::FW_CFG_PORT_SEL => {
                let data = (self.selector as u16).to_be_bytes();
                val.copy_from_slice(data.as_slice());
            }
            Self::FW_CFG_PORT_DATA => {
                // For now, we don't support the fwcfg, so just return zeros
                let data = 0u32.to_be_bytes();
                val.copy_from_slice(&data[..val.len()]);
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: &[u8]) -> Result<()> {
        match port {
            Self::FW_CFG_PORT_SEL => {
                let val: [u8; 2] = val.try_into().map_err(|_| {
                    Error::InvalidValue("Insufficient qemu fw cfg selector write bytes".into())
                })?;
                let val = u16::from_be_bytes(val);

                self.selector = FwCfgSelector::try_from(val).ok_or(Error::InvalidValue(format!(
                    "Unknown FwCfgSelector value: 0x{:x}",
                    val
                )))?
            }
            _ => {
                return Err(Error::NotImplemented(
                    "Write to QEMU FW CFG data port not yet supported".into(),
                ))
            }
        }
        Ok(())
    }
}
