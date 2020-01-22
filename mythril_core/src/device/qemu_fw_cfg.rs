use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::convert::TryInto;
use derive_try_from_primitive::TryFromPrimitive;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
enum FwCfgSelector {
    Signature = 0x0000,
    InterfaceVersion = 0x0001,
    SystemUuid = 0x0002,
    RamSize = 0x0003,
    GraphicsEnabled = 0x0004,
    SmpCpuCount = 0x0005,
    MachineId = 0x0006,
    KernelAddress = 0x0007,
    KernelSize = 0x0008,
    KernelCommandLine = 0x0009,
    InitrdAddress = 0x000a,
    InitrdSize = 0x000b,
    BootDevice = 0x000c,
    NumaData = 0x000d,
    BootMenu = 0x000e,
    MaximumCpuCount = 0x000f,
    KernelEntry = 0x0010,
    KernelData = 0x0011,
    InitrdData = 0x0012,
    CommandLineAddress = 0x0013,
    CommandLineSize = 0x0014,
    CommandLineData = 0x0015,
    KernelSetupAddress = 0x0016,
    KernelSetupSize = 0x0017,
    KernelSetupData = 0x0018,
    FileDir = 0x0019,

    X86AcpiTables = 0x8000,
    X86SmbiosTables = 0x8001,
    X86Irq0Override = 0x8002,
    X86E820Table = 0x8003,
    X86HpetData = 0x8004,
}

#[derive(Debug)]
pub struct QemuFwCfg {
    selector: FwCfgSelector,
    signature: [u8; 4],
    rev: [u8; 4],
    smp_cpu: [u8; 4],
}

impl QemuFwCfg {
    const FW_CFG_PORT_SEL: Port = 0x510;
    const FW_CFG_PORT_DATA: Port = 0x511;
    const _FW_CFG_PORT_DMA: Port = 0x514;

    pub fn new() -> Box<Self> {
        Box::new(Self {
            selector: FwCfgSelector::Signature,
            signature: [0x51, 0x45, 0x4d, 0x55], // QEMU
            rev: [0x01, 0x00, 0x00, 0x00],
            smp_cpu: [0x01, 0x00, 0x00, 0x00],
        })
    }
}

impl EmulatedDevice for QemuFwCfg {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::FW_CFG_PORT_SEL..=Self::FW_CFG_PORT_DATA), // No Support for DMA right now
        ]
    }

    fn on_port_read(&mut self, port: Port, val: &mut PortIoValue) -> Result<()> {
        let len = val.len();
        match port {
            Self::FW_CFG_PORT_SEL => {
                val.copy_from_u32(self.selector as u16 as u32);
            }
            Self::FW_CFG_PORT_DATA => {
                match self.selector {
                    FwCfgSelector::Signature => {
                        val.as_mut_slice().copy_from_slice(&self.signature[..len]);
                        self.signature.rotate_left(len);
                    }
                    FwCfgSelector::InterfaceVersion => {
                        val.as_mut_slice().copy_from_slice(&self.rev[..len]);
                        self.rev.rotate_left(len);
                    }
                    FwCfgSelector::SmpCpuCount => {
                        val.as_mut_slice().copy_from_slice(&self.smp_cpu[..len]);
                        self.smp_cpu.rotate_left(len);
                    }
                    FwCfgSelector::FileDir => {
                        val.copy_from_u32(0);
                    }
                    _ => {
                        // For now, just return zeros for other fields
                        val.copy_from_u32(0);
                    }
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        match port {
            Self::FW_CFG_PORT_SEL => {
                self.selector =
                    FwCfgSelector::try_from(val.try_into()?).ok_or(Error::InvalidValue(format!(
                        "Unknown FwCfgSelector value: 0x{:x}",
                        val.as_u32()
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
