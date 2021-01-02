use crate::error::{Error, Result};
use crate::memory::{
    GuestAccess, GuestAddressSpaceView, GuestPhysAddr, GuestVirtAddr,
    PrivilegeLevel,
};
use crate::virtdev::{
    DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port, PortReadRequest,
    PortWriteRequest,
};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::convert::TryInto;
use spin::RwLock;

// This is _almost_ an enum, but there are 'file' selectors
// between 0x20 and 0x7fff inclusive that make it impractical to actually
// enumerate the selectors.
#[allow(non_snake_case)]
pub mod FwCfgSelector {
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

#[repr(C)]
struct RawFWCfgDmaAccess {
    be_control: u32,
    be_length: u32,
    be_address: u64,
}

impl From<FWCfgDmaAccess> for RawFWCfgDmaAccess {
    fn from(dma: FWCfgDmaAccess) -> Self {
        let control = dma.control.bits() as u32;
        Self {
            be_control: control.to_be(),
            be_length: dma.length.to_be(),
            be_address: dma.address.to_be(),
        }
    }
}

#[derive(Debug)]
struct FWCfgDmaAccess {
    control: DmaControlFlags,
    select: u16,
    length: u32,
    address: u64,
}

bitflags! {
    pub struct DmaControlFlags: u16 {
        const ERROR =  1 << 0;
        const READ =   1 << 1;
        const SKIP =   1 << 2;
        const SELECT = 1 << 3;
        const WRITE =  1 << 4;
    }
}

impl From<RawFWCfgDmaAccess> for FWCfgDmaAccess {
    fn from(raw: RawFWCfgDmaAccess) -> Self {
        let control = u32::from_be(raw.be_control);
        let select = (control >> 16) as u16;
        let control = DmaControlFlags::from_bits_truncate(control as u16);

        let length = u32::from_be(raw.be_length);
        let address = u64::from_be(raw.be_address);

        Self {
            control,
            select,
            length,
            address,
        }
    }
}

pub struct QemuFwCfgBuilder {
    file_info: Vec<FWCfgFile>,
    data: BTreeMap<u16, Vec<u8>>,
}

impl QemuFwCfgBuilder {
    pub fn new() -> Self {
        let mut s = Self {
            data: BTreeMap::new(),
            file_info: vec![],
        };

        s.add_i32(FwCfgSelector::SIGNATURE, 0x554d4551); // QEMU
        s.add_i32(FwCfgSelector::ID, 0b11);

        s
    }

    pub fn build(mut self) -> Arc<RwLock<QemuFwCfg>> {
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
        let mut buffer = vec![
            0u8;
            4 + self.file_info.len()
                * core::mem::size_of::<FWCfgFile>()
        ];

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

        self.data.insert(FwCfgSelector::FILE_DIR, buffer);

        Arc::new(RwLock::new(QemuFwCfg {
            selector: FwCfgSelector::SIGNATURE,
            data: self.data,
            data_idx: 0,
            dma_addr: 0,
        }))
    }

    fn next_file_selector(&self) -> u16 {
        self.data
            .keys()
            .copied()
            .filter(|&s| {
                s >= FwCfgSelector::FILE_FIRST && s <= FwCfgSelector::FILE_LAST
            })
            .max()
            .unwrap_or(FwCfgSelector::FILE_FIRST - 1)
            + 1
    }

    pub fn add_file(
        &mut self,
        name: impl AsRef<str>,
        data: &[u8],
    ) -> Result<()> {
        if name.as_ref().len() > FW_CFG_MAX_FILE_NAME {
            return Err(Error::InvalidValue(format!(
                "qemu_fw_cfg: file name too long: {}",
                name.as_ref()
            )));
        }
        let selector = self.next_file_selector();
        if selector > FwCfgSelector::FILE_LAST {
            return Err(Error::InvalidValue(
                "qemu_fw_cfg: too many files".into(),
            ));
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
        self.data.insert(selector, data.to_vec());
        Ok(())
    }

    pub fn add_i32(&mut self, selector: u16, data: i32) {
        self.data.insert(selector, data.to_le_bytes().to_vec());
    }

    pub fn add_bytes(&mut self, selector: u16, data: &[u8]) {
        self.data.insert(selector, data.to_vec());
    }
}

pub struct QemuFwCfg {
    selector: u16,
    data: BTreeMap<u16, Vec<u8>>,
    data_idx: usize,
    dma_addr: u64,
}

impl QemuFwCfg {
    const FW_CFG_PORT_SEL: Port = 0x510;
    const FW_CFG_PORT_DATA: Port = 0x511;
    const FW_CFG_PORT_DMA_HIGH: Port = 0x514;
    const FW_CFG_PORT_DMA_LOW: Port = 0x518;

    fn perform_dma_transfer(
        &mut self,
        space: GuestAddressSpaceView,
    ) -> Result<()> {
        let bytes = space.read_bytes(
            GuestVirtAddr::NoPaging(GuestPhysAddr::new(self.dma_addr)),
            core::mem::size_of::<RawFWCfgDmaAccess>(),
            GuestAccess::Read(PrivilegeLevel(0)),
        )?;
        let request: RawFWCfgDmaAccess = unsafe {
            core::ptr::read(bytes.as_ptr() as *const RawFWCfgDmaAccess)
        };
        let mut request: FWCfgDmaAccess = request.into();

        if request.control.contains(DmaControlFlags::SELECT) {
            self.selector = request.select;
            self.data_idx = 0;
            request.control &= !DmaControlFlags::SELECT;
        }

        if request.control.contains(DmaControlFlags::SKIP) {
            self.data_idx = request.length as usize;
            request.control &= !DmaControlFlags::SKIP;
        }

        if request.control.contains(DmaControlFlags::READ) {
            match self.read_selector(request.length as usize) {
                Some(data) => {
                    space.write_bytes(
                        GuestVirtAddr::NoPaging(GuestPhysAddr::new(
                            request.address,
                        )),
                        data,
                        GuestAccess::Read(PrivilegeLevel(0)),
                    )?;
                }
                None => request.control = DmaControlFlags::ERROR.into(),
            }
            request.control &= !DmaControlFlags::READ;
        }

        // We don't support writes at all
        if request.control.contains(DmaControlFlags::WRITE) {
            request.control &= !DmaControlFlags::WRITE;
            request.control |= DmaControlFlags::ERROR;
        }

        let request: RawFWCfgDmaAccess = request.into();
        let data = unsafe {
            core::slice::from_raw_parts(
                (&request as *const RawFWCfgDmaAccess) as *const u8,
                core::mem::size_of::<RawFWCfgDmaAccess>(),
            )
        };
        space.write_bytes(
            GuestVirtAddr::NoPaging(GuestPhysAddr::new(self.dma_addr)),
            data,
            GuestAccess::Read(PrivilegeLevel(0)),
        )?;

        Ok(())
    }

    fn read_selector(&mut self, length: usize) -> Option<&[u8]> {
        if self.data.contains_key(&self.selector) {
            let data = &self.data[&(self.selector)];
            let slice = &data[self.data_idx..self.data_idx + length];
            self.data_idx += length;
            return Some(slice);
        } else {
            None
        }
    }

    fn on_port_read(
        &mut self,
        port: Port,
        mut val: PortReadRequest,
    ) -> Result<()> {
        let len = val.len();
        match port {
            Self::FW_CFG_PORT_SEL => {
                val.copy_from_u32(self.selector as u16 as u32);
            }
            Self::FW_CFG_PORT_DATA => {
                let data = self.read_selector(len);
                match data {
                    Some(data) => {
                        val.as_mut_slice().copy_from_slice(data);
                    }
                    None => {
                        info!(
                            "Attempt to read from selector: 0x{:x}",
                            self.selector
                        );

                        // For now, just return zeros for other fields
                        val.copy_from_u32(0);
                    }
                }
            }
            Self::FW_CFG_PORT_DMA_LOW => {
                val.copy_from_u32(0x20434647); // " CFG"
            }
            Self::FW_CFG_PORT_DMA_HIGH => {
                val.copy_from_u32(0x51454d55) // "QEMU"
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn on_port_write(
        &mut self,
        port: Port,
        val: PortWriteRequest,
        space: GuestAddressSpaceView,
    ) -> Result<()> {
        match port {
            Self::FW_CFG_PORT_SEL => {
                self.selector = val.try_into()?;
                self.data_idx = 0;
            }
            Self::FW_CFG_PORT_DATA => {
                return Err(Error::NotImplemented(
                    "Write to QEMU FW CFG data port not yet supported".into(),
                ))
            }
            Self::FW_CFG_PORT_DMA_LOW => {
                let low = u32::from_be(val.try_into()?);
                self.dma_addr |= low as u64;

                self.perform_dma_transfer(space)?;
                self.dma_addr = 0;
            }
            Self::FW_CFG_PORT_DMA_HIGH => {
                let high = u32::from_be(val.try_into()?);
                self.dma_addr = (high as u64) << 32;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

impl EmulatedDevice for QemuFwCfg {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(
                Self::FW_CFG_PORT_SEL..=Self::FW_CFG_PORT_DATA,
            ),
            DeviceRegion::PortIo(
                Self::FW_CFG_PORT_DMA_HIGH..=Self::FW_CFG_PORT_DMA_LOW,
            ),
        ]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(port, val) => self.on_port_read(port, val)?,
            DeviceEvent::PortWrite(port, val) => {
                self.on_port_write(port, val, event.space)?
            }
            _ => (),
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_next_file_selector_first() {
        let builder = QemuFwCfgBuilder::new();
        let selector = builder.next_file_selector();
        assert!(selector >= FwCfgSelector::FILE_FIRST);
        assert!(selector <= FwCfgSelector::FILE_LAST);
    }

    #[test]
    fn test_next_file_selector_last() {
        let mut builder = QemuFwCfgBuilder::new();
        builder.add_i32(FwCfgSelector::FILE_LAST + 1, 0x0);
        let selector = builder.next_file_selector();
        assert!(selector >= FwCfgSelector::FILE_FIRST);
        assert!(selector <= FwCfgSelector::FILE_LAST);
    }
}
