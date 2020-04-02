use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::convert::TryInto;
use derive_try_from_primitive::TryFromPrimitive;

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum VgaRegister {
    HorizontalTotalChars = 0x00,
    HorizontalCharsPerLine = 0x01,
    HorizontalSyncPosition = 0x02,
    HorizontalSyncWidthInChars = 0x03,
    VirticalTotalLines = 0x04,
    VirticalTotalAdjust = 0x05,
    VirticalDisplayedRows = 0x06,
    VirticalSyncPosition = 0x07,
    InterlaceMode = 0x08,
    MaxScanLineAddr = 0x09,
    CursorStart = 0x0a,
    CursorEnd = 0x0b,
    StartAddrMsb = 0x0c,
    StartAddrLsb = 0x0d,
    CursorAddrMsb = 0x0e,
    CursorAddrLsb = 0x0f,
}

#[derive(Debug)]
pub struct VgaController {
    index: VgaRegister,

    registers: [u8; 0x10],
}

#[allow(dead_code)]
impl VgaController {
    const VGA_INDEX: Port = 0x03D4;
    const VGA_DATA: Port = 0x03D5;

    pub fn new() -> Box<Self> {
        Box::new(Self {
            index: VgaRegister::HorizontalTotalChars,

            registers: [
                0x61, // HorizontalTotalChars
                0x50, // HorizontalCharsPerLine
                0x52, // HorizontalSyncPosition
                0x0f, // HorizontalSyncWidthInChars
                0x19, // VirticalTotalLines
                0x06, // VirticalTotalAdjust
                0x19, // VirticalDisplayedRows
                0x19, // VirticalSyncPosition
                0x02, // InterlaceMode
                0x0d, // MaxScanLineAddr
                0x0b, // CursorStart
                0x0c, // CursorEnd
                0x00, // StartAddrMsb
                0x00, // StartAddrLsb
                0x00, // CursorAddrMsb
                0x00, // CursorAddrLsb
            ],
        })
    }
}

impl EmulatedDevice for VgaController {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            // vga stuff
            DeviceRegion::PortIo(Self::VGA_INDEX..=Self::VGA_DATA),
        ]
    }

    fn on_port_read(
        &mut self,
        port: Port,
        val: &mut PortIoValue,
    ) -> Result<()> {
        match port {
            Self::VGA_DATA => {
                val.copy_from_u32(self.registers[self.index as usize] as u32);
            }
            _ => {
                return Err(Error::NotImplemented(format!(
                    "Unsupported attempt to read from vga port 0x{:x}",
                    port
                )))
            }
        }
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        match port {
            Self::VGA_INDEX => match val {
                PortIoValue::OneByte(b) => {
                    self.index =
                        VgaRegister::try_from(b[0]).ok_or_else(|| {
                            Error::InvalidValue(format!(
                                "Invalid vga register 0x{:x}",
                                b[0]
                            ))
                        })?
                }

                // The VGA controller allows a register update and data write
                // in one operation (and linux actually does this), so handle
                // that here
                PortIoValue::TwoBytes(bytes) => {
                    let index = bytes[1];
                    let data = bytes[0];
                    self.index =
                        VgaRegister::try_from(index).ok_or_else(|| {
                            Error::InvalidValue(format!(
                                "Invalid vga register 0x{:x}",
                                index
                            ))
                        })?;
                    self.registers[self.index as usize] = data;
                }
                _ => {
                    return Err(Error::InvalidValue(format!(
                        "Invalid port write to VGA index register: {:?}",
                        val
                    )))
                }
            },
            Self::VGA_DATA => {
                self.registers[self.index as usize] = val.try_into()?;
            }
            _ => {
                return Err(Error::NotImplemented(format!(
                    "Unsupported attempt to write to vga port 0x{:x}",
                    port
                )))
            }
        }
        Ok(())
    }
}
