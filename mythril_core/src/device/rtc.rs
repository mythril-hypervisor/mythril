use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::convert::TryInto;
use derive_try_from_primitive::TryFromPrimitive;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
enum CmosRegister {
    Seconds = 0x00,
    SecondsAlarm = 0x01,
    Minutes = 0x02,
    MinutesAlarm = 0x03,
    Hours = 0x04,
    HoursAlarm = 0x05,
    DayOfWeek = 0x06,
    DayOfMonth = 0x07,
    Month = 0x08,
    Year = 0x09,
    StatusRegisterA = 0x0a,
    StatusRegisterB = 0x0b,
    StatusRegisterC = 0x0c,
    StatusRegisterD = 0x0d,
    DiagnosticStatus = 0x0e,
    ShutdownStatus = 0x0f,
    DisketteDriveType = 0x10,
    FixedDiskDriveType = 0x12,
    Equipment = 0x14,
    BaseSystemMemoryLsb = 0x15,
    BaseSystemMemoryMsb = 0x16,
    TotalExtendedMemoryLsb = 0x17,
    TotalExtendedMemoryMsb = 0x18,
    DriveCExtension = 0x19,
    DriveDExtension = 0x1a,
    CmosChecksumMsb = 0x2e,
    CmosChecksumLsb = 0x2f,
    ExtendedPostMemLsb = 0x30,
    ExtendedPostMemMsb = 0x31,
    BcdCenturyDate = 0x32,
    InfoFlags = 0x33,

    // The follow fields appear to be qemu extensions used by OVMF
    //
    // From OvmfPkg/PlatformPei/MemDetect.c:
    // CMOS 0x34/0x35 specifies the system memory above 16 MB.
    // * CMOS(0x35) is the high byte
    // * CMOS(0x34) is the low byte
    // * The size is specified in 64kb chunks
    QemuMemAbove16MbLsb = 0x34,
    QemuMemAbove16MbMsb = 0x35,

    // CMOS 0x5b-0x5d specifies the system memory above 4GB MB.
    // * CMOS(0x5d) is the most significant size byte
    // * CMOS(0x5c) is the middle size byte
    // * CMOS(0x5b) is the least significant size byte
    // * The size is specified in 64kb chunks
    QemuMemAbove4GbLsb = 0x5b,
    QemuMemAbove4GbMmsb = 0x5c,
    QemuMemAbove4GbMsb = 0x5d,

    // Not in any spec, used to represent unknown values
    Unknown = 0xff,
}

#[derive(Debug)]
pub struct CmosRtc {
    addr: CmosRegister,
}

impl CmosRtc {
    const RTC_ADDRESS: Port = 0x0070;
    const RTC_DATA: Port = 0x0071;

    pub fn new() -> Box<Self> {
        Box::new(Self {
            addr: CmosRegister::Seconds, // For now, just set the default reg as seconds
        })
    }
}

//TODO: support the NMI masking stuff
impl EmulatedDevice for CmosRtc {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(Self::RTC_ADDRESS..=Self::RTC_DATA)]
    }

    fn on_port_read(&mut self, port: Port, val: &mut PortIoValue) -> Result<()> {
        match port {
            Self::RTC_ADDRESS => {
                *val = (self.addr as u8).into();
            }
            Self::RTC_DATA => {
                match self.addr {
                    CmosRegister::ShutdownStatus => {
                        val.copy_from_u32(0u32); // For now, always report soft reset
                    }
                    CmosRegister::QemuMemAbove16MbMsb => {
                        //FIXME: for now just report 80MB available
                        val.copy_from_u32(0x5);
                    }
                    _ => {
                        val.copy_from_u32(0x00);
                    }
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        match port {
            Self::RTC_ADDRESS => {
                // OVMF expects to be able to read pretty much any address
                // (and just get zeros for meaningless ones)
                self.addr =
                    CmosRegister::try_from(val.try_into()?).unwrap_or(CmosRegister::Unknown);
            }
            _ => {
                match self.addr {
                    CmosRegister::ShutdownStatus => {
                        // It's not clear what's supposed to happen here, just ignore
                        // it for now
                    }
                    addr => {
                        return Err(Error::NotImplemented(format!(
                            "Write to RTC address ({:?}) not yet supported: {:?}",
                            addr, val
                        )))
                    }
                }
            }
        }
        Ok(())
    }
}
