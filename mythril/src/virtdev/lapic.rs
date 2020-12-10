use crate::error::{Error, Result};
use core::convert::TryFrom;
use core::ops::RangeInclusive;
use num_enum::TryFromPrimitive;

#[derive(Debug)]
enum ApicRegisterOffset {
    Simple(ApicRegisterSimpleOffset),
    InterruptRequest(u16),
    InterruptCommand(u16),
    TriggerMode(u16),
    InService(u16),
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u16)]
enum ApicRegisterSimpleOffset {
    ApicId = 0x20,
    ApicVersion = 0x30,
    TaskPriority = 0x80,
    ArbitrationPriority = 0x90,
    ProcessorPriority = 0xa0,
    EndOfInterrupt = 0xb0,
    RemoteRead = 0xc0,
    LogicalDestination = 0xd0,
    DestinationFormat = 0xe0,
    SpuriousInterruptVector = 0xf0,
    ErrorStatus = 0x280,
    LvtCorrectMachineCheckInterrupt = 0x2f0,
    LvtTimer = 0x320,
    LvtThermalSensor = 0x330,
    LvtPerformanceMonitoringCounter = 0x340,
    LvtLINT0 = 0x350,
    LvtLINT1 = 0x360,
    LvtError = 0x370,
    TimerInitialCount = 0x380,
    TimerCurrentCount = 0x390,
    TimerDivideConfig = 0x3e0,
}

impl TryFrom<u16> for ApicRegisterOffset {
    type Error = Error;

    fn try_from(value: u16) -> Result<ApicRegisterOffset> {
        if value & 0b1111 != 0 {
            return Err(Error::InvalidValue(format!(
                "APIC register offset not aligned: 0x{:x}",
                value
            )));
        }

        if let Ok(simple_reg) = ApicRegisterSimpleOffset::try_from(value) {
            return Ok(ApicRegisterOffset::Simple(simple_reg));
        }

        let res = match value {
            0x100..=0x170 => {
                ApicRegisterOffset::InService((value - 0x100) >> 4)
            }
            0x180..=0x1f0 => {
                ApicRegisterOffset::TriggerMode((value - 0x180) >> 4)
            }
            0x200..=0x270 => {
                ApicRegisterOffset::InterruptRequest((value - 0x200) >> 4)
            }
            0x300..=0x310 => {
                ApicRegisterOffset::InterruptCommand((value - 0x300) >> 4)
            }
            offset => {
                return Err(Error::InvalidValue(format!(
                    "Invalid APIC register offset: 0x{:x}",
                    offset
                )))
            }
        };

        Ok(res)
    }
}

#[derive(Default)]
pub struct LocalApic;

impl LocalApic {
    pub fn new() -> Self {
        LocalApic {}
    }

    pub fn register_read(&mut self, offset: u16) -> Result<u32> {
        info!(
            "Read from virtual local apic: {:?}",
            ApicRegisterOffset::try_from(offset)
        );
        Ok(0)
    }

    pub fn register_write(&mut self, offset: u16, _value: u32) -> Result<()> {
        info!(
            "Write to virtual local apic: {:?}",
            ApicRegisterOffset::try_from(offset)
        );
        Ok(())
    }
}
