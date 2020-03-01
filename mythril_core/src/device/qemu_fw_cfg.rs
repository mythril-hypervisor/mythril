use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::convert::TryInto;

// This is _almost_ an enum, but there are 'file' selectors
// between 0x20 and 0x7fff that make it impractical to actually
// enumerate the selectors.
#[allow(non_snake_case)]
#[allow(dead_code)]
mod FwCfgSelector {
    pub const SIGNATURE: u16 = 0x00;
    pub const ID: u16 = 0x01;
    pub const UUID: u16 = 0x02;
    pub const RAM_SIZE: u16 = 0x03;
    pub const NOGRAPHIC: u16 = 0x04;
    pub const NB_CPUS: u16 = 0x05;
    pub const MACHINE_ID: u16 = 0x06;
    pub const KERNEL_ADDR: u16 = 0x07;
    pub const KERNEL_SIZE: u16 = 0x08;
    pub const KERNEL_CMDLINE: u16 = 0x09;
    pub const INITRD_ADDR: u16 = 0x0a;
    pub const INITRD_SIZE: u16 = 0x0b;
    pub const BOOT_DEVICE: u16 = 0x0c;
    pub const NUMA: u16 = 0x0d;
    pub const BOOT_MENU: u16 = 0x0e;
    pub const MAX_CPUS: u16 = 0x0f;
    pub const KERNEL_ENTRY: u16 = 0x10;
    pub const KERNEL_DATA: u16 = 0x11;
    pub const INITRD_DATA: u16 = 0x12;
    pub const CMDLINE_ADDR: u16 = 0x13;
    pub const CMDLINE_SIZE: u16 = 0x14;
    pub const CMDLINE_DATA: u16 = 0x15;
    pub const SETUP_ADDR: u16 = 0x16;
    pub const SETUP_SIZE: u16 = 0x17;
    pub const SETUP_DATA: u16 = 0x18;
    pub const FILE_DIR: u16 = 0x19;
    pub const FILE_FIRST: u16 = 0x20;
    pub const FILE_LAST: u16 = 0x7fff;
    pub const X86_ACPI_TABLES: u16 = 0x8000;
    pub const X86_SMBIOS_TABLES: u16 = 0x8001;
    pub const X86_IRQ0_OVERRIDES: u16 = 0x8002;
    pub const X86_E820_TABLE: u16 = 0x8003;
    pub const X86_HEPT_DATA: u16 = 0x8003;
}

const FW_CFG_MAX_FILE_NAME: usize = 55;

#[repr(C)]
struct FWCfgFile {
    size: u32,
    select: u16,
    _reserved: u16,
    name: [u8; FW_CFG_MAX_FILE_NAME + 1], // +1 for NULL terminator
}

pub struct QemuFwCfgBuilder {
    file_info: Vec<FWCfgFile>,
    file_data: BTreeMap<u16, Vec<u8>>,
}

impl QemuFwCfgBuilder {
    pub fn new() -> Self {
        Self {
            file_info: vec![],
            file_data: BTreeMap::new(),
        }
    }

    pub fn build(self) -> Box<QemuFwCfg> {
        // Now that we are done building the fwcfg device, we need to make the
        // FileDir buffer, which has the following structure:
        //
        // From QEMU docs:
        //
        // struct FWCfgFiles {      /* the entire file directory fw_cfg item */
        //    uint32_t count;       /* number of entries, in big-endian format */
        //    struct FWCfgFile f[]; /* array of file entries */
        // };
        let info_len = (self.file_info.len() as u32).to_be_bytes();
        let mut buffer = vec![0u8; 4 + self.file_info.len() * core::mem::size_of::<FWCfgFile>()];

        // Copy the count
        buffer[..4].copy_from_slice(&info_len);

        // And now the file entries
        unsafe {
            core::ptr::copy(
                self.file_info.as_ptr() as *const u8,
                buffer[4..].as_mut_ptr(),
                buffer.len() - 4,
            );
        }

        Box::new(QemuFwCfg {
            selector: FwCfgSelector::SIGNATURE,
            signature: [0x51, 0x45, 0x4d, 0x55], // QEMU
            rev: [0x01, 0x00, 0x00, 0x00],
            smp_cpu: [0x01, 0x00, 0x00, 0x00],
            file_info: buffer.into_boxed_slice(),
            file_data: self.file_data,
            file_data_idx: 0,
            file_info_idx: 0,
        })
    }

    fn next_file_selector(&self) -> u16 {
        self.file_data
            .keys()
            .copied()
            .max()
            .unwrap_or(FwCfgSelector::FILE_FIRST)
            + 1
    }

    pub fn add_file(&mut self, name: impl AsRef<str>, data: &[u8]) -> Result<()> {
        if name.as_ref().len() > FW_CFG_MAX_FILE_NAME {
            return Err(Error::InvalidValue(format!(
                "qemu_fw_cfg: file name too long: {}",
                name.as_ref()
            )));
        }
        let selector = self.next_file_selector();
        if selector > FwCfgSelector::FILE_LAST {
            return Err(Error::InvalidValue("qemu_fw_cfg: too many files".into()));
        }

        let name = name.as_ref().as_bytes();
        let mut info = FWCfgFile {
            size: (data.len() as u32).to_be(),
            select: selector.to_be(),
            _reserved: 0,
            name: [0u8; FW_CFG_MAX_FILE_NAME + 1],
        };

        info.name[..name.len()].copy_from_slice(name);

        self.file_info.push(info);
        self.file_data.insert(selector, data.to_vec());
        Ok(())
    }
}

pub struct QemuFwCfg {
    selector: u16,
    signature: [u8; 4],
    rev: [u8; 4],
    smp_cpu: [u8; 4],
    file_info: Box<[u8]>,
    file_data: BTreeMap<u16, Vec<u8>>,

    file_data_idx: usize,
    file_info_idx: usize,
}

impl QemuFwCfg {
    const FW_CFG_PORT_SEL: Port = 0x510;
    const FW_CFG_PORT_DATA: Port = 0x511;
    const _FW_CFG_PORT_DMA: Port = 0x514;
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
                    FwCfgSelector::SIGNATURE => {
                        val.as_mut_slice().copy_from_slice(&self.signature[..len]);
                        self.signature.rotate_left(len);
                    }
                    FwCfgSelector::ID => {
                        val.as_mut_slice().copy_from_slice(&self.rev[..len]);
                        self.rev.rotate_left(len);
                    }
                    FwCfgSelector::NB_CPUS => {
                        val.as_mut_slice().copy_from_slice(&self.smp_cpu[..len]);
                        self.smp_cpu.rotate_left(len);
                    }
                    FwCfgSelector::FILE_DIR => {
                        val.as_mut_slice().copy_from_slice(
                            &self.file_info[self.file_info_idx..self.file_info_idx + len],
                        );
                        self.file_info_idx += len;
                    }
                    selector
                        if selector >= FwCfgSelector::FILE_FIRST
                            && selector <= FwCfgSelector::FILE_LAST =>
                    {
                        let data = &self.file_data[&(self.selector)];
                        val.as_mut_slice()
                            .copy_from_slice(&data[self.file_data_idx..self.file_data_idx + len]);
                        self.file_data_idx += len;
                    }
                    selector => {
                        info!("Attempt to read from selector: 0x{:x}", selector);

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
                self.selector = val.try_into()?;
                self.file_data_idx = 0;
                self.file_info_idx = 0;
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
