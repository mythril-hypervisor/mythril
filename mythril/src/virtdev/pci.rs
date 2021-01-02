use crate::error::{Error, Result};
use crate::virtdev::{DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port};
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryInto;
use num_enum::TryFromPrimitive;
use spin::RwLock;
use ux;

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u16)]
enum VendorId {
    Intel = 0x8086,
}

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u16)]
enum DeviceId {
    // NOTE: this device ID is referred to as Q35 by QEMU, but this is
    // not correct. The Q35 chipset has integrated graphics (among other
    // differences). We use the correct name P35.
    P35Mch = 0x29c0,
    Ich9 = 0x2918,
}

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
struct PciNonBridgeSpace {
    header: PciNonBridgeHeader,
    _data: [u32; 48],
}

impl PciNonBridgeSpace {
    fn new(header: PciNonBridgeHeader) -> Self {
        Self {
            header,
            _data: [0u32; 48],
        }
    }
}

#[repr(C)]
#[repr(packed)]
struct PciToPciBridgeSpace {
    _data: [u32; 64],
}

#[repr(C)]
#[repr(packed)]
struct PciToCardbusBridgeSpace {
    _data: [u32; 64],
}

#[allow(dead_code)]
enum PciConfigSpace {
    Type0(PciNonBridgeSpace),
    Type1(PciToPciBridgeSpace),
    Type2(PciToCardbusBridgeSpace),
}

impl PciConfigSpace {
    fn as_registers(&self) -> &[u32; 64] {
        match self {
            PciConfigSpace::Type0(space) => unsafe {
                core::mem::transmute(space)
            },
            PciConfigSpace::Type1(space) => unsafe {
                core::mem::transmute(space)
            },
            PciConfigSpace::Type2(space) => unsafe {
                core::mem::transmute(space)
            },
        }
    }

    fn read_register(&self, register: u8) -> u32 {
        self.as_registers()[register as usize]
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
        (self.bus as u16) << 8
            | (u16::from(self.device) << 3)
            | u16::from(self.function)
    }
}

pub struct PciDevice {
    config_space: PciConfigSpace,
    bdf: PciBdf,
}

pub struct PciRootComplex {
    current_address: u32,
    devices: BTreeMap<u16, PciDevice>,
}

impl PciRootComplex {
    const PCI_CONFIG_ADDRESS: Port = 0xcf8;
    const PCI_CONFIG_TYPE: Port = 0xcfb;
    const PCI_CONFIG_DATA: Port = 0xcfc;
    const PCI_CONFIG_DATA_MAX: Port = Self::PCI_CONFIG_DATA + 3;

    pub fn new() -> Arc<RwLock<Self>> {
        let mut devices = BTreeMap::new();

        let host_bridge = PciDevice {
            bdf: PciBdf::from(0x0000),
            config_space: PciConfigSpace::Type0(PciNonBridgeSpace::new(
                PciNonBridgeHeader {
                    vendor_id: VendorId::Intel as u16,
                    device_id: DeviceId::P35Mch as u16,
                    ..PciNonBridgeHeader::default()
                },
            )),
        };
        devices.insert(host_bridge.bdf.into(), host_bridge);

        let ich9 = PciDevice {
            bdf: PciBdf::from(0b1000),
            config_space: PciConfigSpace::Type0(PciNonBridgeSpace::new(
                PciNonBridgeHeader {
                    vendor_id: VendorId::Intel as u16,
                    device_id: DeviceId::Ich9 as u16,
                    ..PciNonBridgeHeader::default()
                },
            )),
        };
        devices.insert(ich9.bdf.into(), ich9);

        Arc::new(RwLock::new(Self {
            current_address: 0,
            devices: devices,
        }))
    }
}

impl EmulatedDevice for PciRootComplex {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(
                Self::PCI_CONFIG_ADDRESS..=Self::PCI_CONFIG_ADDRESS,
            ),
            DeviceRegion::PortIo(
                Self::PCI_CONFIG_DATA..=Self::PCI_CONFIG_DATA_MAX,
            ),
            DeviceRegion::PortIo(Self::PCI_CONFIG_TYPE..=Self::PCI_CONFIG_TYPE),
        ]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(port, mut val) => {
                match port {
                    Self::PCI_CONFIG_ADDRESS => {
                        // For now, always set the enable bit
                        let addr = 0x80000000 | self.current_address;
                        val.copy_from_u32(addr);
                    }
                    Self::PCI_CONFIG_DATA..=Self::PCI_CONFIG_DATA_MAX => {
                        let bdf =
                            ((self.current_address & 0xffff00) >> 8) as u16;
                        let register = (self.current_address & 0xff >> 2) as u8;
                        let offset = (port - Self::PCI_CONFIG_DATA) as u8;

                        match self.devices.get(&bdf) {
                            Some(device) => {
                                let res =
                                    device.config_space.read_register(register)
                                        >> (offset * 8);
                                val.copy_from_u32(res);
                                debug!(
                                    "pci: port=0x{:x}, register=0x{:x}, offset=0x{:x}, val={}",
                                    port, register, offset, val
                                );
                            }
                            None => {
                                // If no device is present, just return all 0xFFs
                                let res = 0xffffffffu32;
                                val.copy_from_u32(res);
                            }
                        }
                    }
                    _ => {
                        return Err(Error::InvalidValue(format!(
                            "Invalid PCI port read 0x{:x}",
                            port
                        )))
                    }
                }
            }
            DeviceEvent::PortWrite(port, val) => match port {
                Self::PCI_CONFIG_ADDRESS => {
                    let addr: u32 = val.try_into()?;
                    self.current_address = addr & 0x7fffffffu32;
                }
                _ => {
                    debug!(
                            "pci: Attempt to write to port=0x{:x} (addr=0x{:x}). Ignoring.",
                            port, self.current_address
                        );
                }
            },
            _ => (),
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::memory::{
        GuestAddressSpace, GuestAddressSpaceView, GuestPhysAddr,
    };
    use crate::virtdev::*;
    use alloc::boxed::Box;

    fn define_test_view() -> GuestAddressSpaceView<'static> {
        let space: &'static mut GuestAddressSpace =
            Box::leak(Box::new(GuestAddressSpace::new().unwrap()));
        GuestAddressSpaceView::new(GuestPhysAddr::new(0), space)
    }

    fn complex_ready_for_reg_read(reg: u8) -> Arc<RwLock<PciRootComplex>> {
        let view = define_test_view();
        let complex = PciRootComplex::new();
        let addr = ((reg << 2) as u32).to_be_bytes();
        let request = PortWriteRequest::try_from(&addr[..]).unwrap();
        let mut responses = ResponseEventArray::default();
        let event = Event::new(
            DeviceEvent::PortWrite(PciRootComplex::PCI_CONFIG_ADDRESS, request),
            view,
            &mut responses,
        )
        .unwrap();
        {
            let mut complex = complex.write();
            complex.on_event(event).unwrap();
        }
        complex
    }

    #[test]
    fn test_full_register_read() {
        let view = define_test_view();
        let complex = complex_ready_for_reg_read(0);
        let mut buff = [0u8; 4];
        let val = PortReadRequest::FourBytes(&mut buff);
        let mut responses = ResponseEventArray::default();
        let mut complex = complex.write();
        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();

        assert_eq!(u32::from_be_bytes(buff), 0x29c08086);
    }

    #[test]
    fn test_half_register_read() {
        let view = define_test_view();
        let complex = complex_ready_for_reg_read(0);
        let mut buff = [0u8; 2];
        let val = PortReadRequest::TwoBytes(&mut buff);
        let mut responses = ResponseEventArray::default();
        let mut complex = complex.write();
        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();
        assert_eq!(u16::from_be_bytes(buff), 0x8086);

        let view = define_test_view();
        let val = PortReadRequest::TwoBytes(&mut buff);
        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA + 2, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();
        assert_eq!(u16::from_be_bytes(buff), 0x29c0);
    }

    #[test]
    fn test_register_byte_read() {
        let complex = complex_ready_for_reg_read(0);
        let mut buff = [0u8; 1];
        let mut responses = ResponseEventArray::default();
        let mut complex = complex.write();

        let view = define_test_view();
        let val = PortReadRequest::OneByte(&mut buff);

        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();
        assert_eq!(u8::from_be_bytes(buff), 0x86);

        let view = define_test_view();
        let val = PortReadRequest::OneByte(&mut buff);
        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA + 1, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();
        assert_eq!(u8::from_be_bytes(buff), 0x80);

        let view = define_test_view();
        let val = PortReadRequest::OneByte(&mut buff);
        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA + 2, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();
        assert_eq!(u8::from_be_bytes(buff), 0xc0);

        let view = define_test_view();
        let val = PortReadRequest::OneByte(&mut buff);
        let event = Event::new(
            DeviceEvent::PortRead(PciRootComplex::PCI_CONFIG_DATA + 3, val),
            view,
            &mut responses,
        )
        .unwrap();
        complex.on_event(event).unwrap();
        assert_eq!(u8::from_be_bytes(buff), 0x29);
    }
}
