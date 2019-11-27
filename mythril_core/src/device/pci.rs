use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use core::convert::TryInto;
use core::mem::{size_of, transmute};
use ux;

#[repr(C)]
#[repr(packed)]
#[derive(Default)]
struct PciNonBridgeHeader {
    vendor_id: u16,
    device_id: u16,
    command: u16,
    status: u16,
    revision_id: u8,
    prog_if: u8,
    subclass: u8,
    class: u8,
    cache_line_size: u8,
    latency_timer: u8,
    header_type: u8,
    bist: u8,
    bar_0: u32,
    bar_1: u32,
    bar_2: u32,
    bar_3: u32,
    bar_4: u32,
    bar_5: u32,
    cardbus_cis: u32,
    subsystem_vendor_id: u16,
    subsystem_id: u16,
    expansion_rom_addr: u32,
    capabilities: u8,
    _reserved: [u8; 7],
    interrupt_line: u8,
    interrupt_pin: u8,
    min_grant: u8,
    max_latency: u8,
}

#[repr(C)]
#[repr(packed)]
struct PciToPciBridgeHeader {}

#[repr(C)]
#[repr(packed)]
struct PciToCardbusBridgeHeader {}

enum PciHeader {
    Type0(PciNonBridgeHeader),
    Type1(PciToPciBridgeHeader),
    Type2(PciToCardbusBridgeHeader),
}

impl PciHeader {
    fn read_offset(&self, offset: u8) -> u16 {
        // Only 16 ranges can be addressed
        assert!(offset & 1 == 0);

        match self {
            PciHeader::Type0(header) => {
                let data: &[u16; size_of::<PciNonBridgeHeader>() / 2] =
                    unsafe { transmute(header) };
                data[(offset / 2) as usize]
            }
            _ => panic!("Not implemented yet"),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct PciBdf {
    bus: u8,
    device: ux::u5,
    function: ux::u3,
}

impl From<u16> for PciBdf {
    fn from(bytes: u16) -> Self {
        Self {
            bus: ((bytes & 0xff00) >> 8) as u8,
            device: ux::u5::new(((bytes & 0b11111000) >> 3) as u8),
            function: ux::u3::new((bytes & 0b111) as u8),
        }
    }
}

impl Into<u16> for PciBdf {
    fn into(self) -> u16 {
        (self.bus as u16) << 8 | (u16::from(self.device) << 3) | u16::from(self.function)
    }
}

pub struct PciDevice {
    header: PciHeader,
    bdf: PciBdf,
}

pub struct PciRootComplex {
    current_address: u32,
    devices: BTreeMap<u16, PciDevice>,
}

impl PciRootComplex {
    const PCI_CONFIG_ADDRESS: Port = 0xcf8;
    const PCI_CONFIG_DATA: Port = 0xcfc;
    const PCI_CONFIG_DATA_MAX: Port = Self::PCI_CONFIG_DATA + 256;

    pub fn new() -> Box<Self> {
        let mut devices = BTreeMap::new();

        let host_bridge = PciDevice {
            bdf: PciBdf::from(0x0000),
            header: PciHeader::Type0(PciNonBridgeHeader {
                device_id: 0x29c0,
                ..PciNonBridgeHeader::default()
            }),
        };

        devices.insert(host_bridge.bdf.into(), host_bridge);

        Box::new(Self {
            current_address: 0,
            devices: devices,
        })
    }
}

impl EmulatedDevice for PciRootComplex {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::PCI_CONFIG_ADDRESS..=Self::PCI_CONFIG_ADDRESS),
            DeviceRegion::PortIo(Self::PCI_CONFIG_DATA..=Self::PCI_CONFIG_DATA_MAX),
        ]
    }
    fn on_port_read(&mut self, port: Port, val: &mut PortIoValue) -> Result<()> {
        match port {
            Self::PCI_CONFIG_ADDRESS => {
                // For now, always set the enable bit
                let addr = 0x80000000 | self.current_address;
                *val = addr.into();
            }
            _ => {
                // TODO: what is the expected behavior when val.len() != 2?

                let bdf = ((self.current_address & 0xffff00) >> 8) as u16;
                let offset =
                    (self.current_address & 0xff) as u8 | (port - Self::PCI_CONFIG_DATA) as u8;

                match self.devices.get(&bdf) {
                    Some(device) => {
                        info!("Query for real device: {}", bdf);
                        let res = device.header.read_offset(offset);
                        info!("  res = {:?}", res);
                        *val = res.into();
                    }
                    None => {
                        info!("Query for missing device = {}", bdf);
                        // If no device is present, just return all 0xFFs
                        let res = 0xffffffffu32;
                        *val = res.into();
                    }
                }
            }
        }
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        match port {
            Self::PCI_CONFIG_ADDRESS => self.current_address = val.try_into()?,
            _ => (),
        }
        Ok(())
    }
}
