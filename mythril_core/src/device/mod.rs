use crate::error::{Error, Result};
use crate::memory::GuestPhysAddr;
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Ref, RefCell, RefMut};
use core::cmp::Ordering;
use core::convert::TryInto;
use core::ops::RangeInclusive;

pub mod pci;
pub mod pic;

#[derive(Default)]
pub struct DeviceMap {
    map: BTreeMap<DeviceRegion, Rc<RefCell<Box<dyn EmulatedDevice>>>>,
}

#[derive(Eq, PartialEq)]
pub enum DeviceRegion {
    PortIo(RangeInclusive<u16>),
    MemIo(RangeInclusive<GuestPhysAddr>),
}

impl From<DeviceInteraction> for DeviceRegion {
    fn from(val: DeviceInteraction) -> Self {
        match val {
            DeviceInteraction::PortIo(start) => {
                DeviceRegion::PortIo(RangeInclusive::new(start, start))
            }
            DeviceInteraction::MemIo(start) => {
                DeviceRegion::MemIo(RangeInclusive::new(start, start))
            }
        }
    }
}

impl PartialOrd for DeviceRegion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeviceRegion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self {
            DeviceRegion::PortIo(this_range) => match other {
                DeviceRegion::PortIo(other_range) => {
                    if this_range.end() < other_range.start() {
                        Ordering::Less
                    } else if other_range.end() < this_range.start() {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                }
                DeviceRegion::MemIo(_) => Ordering::Less,
            },
            DeviceRegion::MemIo(this_range) => match other {
                DeviceRegion::PortIo(_) => Ordering::Greater,
                DeviceRegion::MemIo(other_range) => {
                    if this_range.end() < other_range.start() {
                        Ordering::Less
                    } else if other_range.end() < this_range.start() {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                }
            },
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum DeviceInteraction {
    PortIo(u16),
    MemIo(GuestPhysAddr),
}

impl From<u16> for DeviceInteraction {
    fn from(val: u16) -> Self {
        DeviceInteraction::PortIo(val)
    }
}

impl From<GuestPhysAddr> for DeviceInteraction {
    fn from(val: GuestPhysAddr) -> Self {
        DeviceInteraction::MemIo(val)
    }
}

impl DeviceMap {
    pub fn device_for(&self, op: DeviceInteraction) -> Option<Ref<Box<dyn EmulatedDevice>>> {
        self.map.get(&op.into()).map(|v| v.borrow())
    }

    pub fn device_for_mut(
        &mut self,
        op: DeviceInteraction,
    ) -> Option<RefMut<Box<dyn EmulatedDevice>>> {
        self.map.get_mut(&op.into()).map(|v| v.borrow_mut())
    }

    pub fn register_device(&mut self, dev: Box<dyn EmulatedDevice>) {
        let services = dev.services();
        let dev = Rc::new(RefCell::new(dev));
        for region in services.into_iter() {
            self.map.insert(region, Rc::clone(&dev));
        }
    }
}

pub trait EmulatedDevice {
    fn services(&self) -> Vec<DeviceRegion>;

    fn on_mem_read(&mut self, _addr: GuestPhysAddr, _data: &mut [u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "MemoryMapped device does not support reading".into(),
        ))
    }
    fn on_mem_write(&mut self, _addr: GuestPhysAddr, _data: &[u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "MemoryMapped device does not support writing".into(),
        ))
    }
    fn on_port_read(&mut self, _port: u16, _val: &mut [u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "PortIo device does not support reading".into(),
        ))
    }
    fn on_port_write(&mut self, _port: u16, _val: &[u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "PortIo device does not support writing".into(),
        ))
    }
}

pub struct ComDevice {
    port: u16,
}

impl ComDevice {
    pub fn new(port: u16) -> Box<dyn EmulatedDevice> {
        Box::new(Self { port })
    }
}

impl EmulatedDevice for ComDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_port_read(&mut self, _port: u16, val: &mut [u8]) -> Result<()> {
        // This is a magical value (called BOCHS_DEBUG_PORT_MAGIC by edk2)
        // FIXME: this should only be returned for a special 'debug' com device
        val[0] = 0xe9;
        Ok(())
    }

    fn on_port_write(&mut self, _port: u16, val: &[u8]) -> Result<()> {
        // TODO: I'm not sure what the correct behavior is here for a Com device.
        //       For now, just print each byte (except NUL because that crashes)
        let s: String = String::from_utf8_lossy(val)
            .into_owned()
            .chars()
            .filter(|c| *c != (0 as char))
            .collect();
        info!("{}", s);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_memmap_write_to_portio_fails() {
        let mut com = ComDevice::new(0);
        let addr = GuestPhysAddr::new(0);
        assert_eq!(com.on_mem_write(addr, &[0, 0, 0, 0]).is_err(), true);
    }

    #[test]
    fn test_device_map() {
        let mut map = DeviceMap::default();
        let com = ComDevice::new(0);
        map.register_device(com);
        let dev = map.device_for(0.into()).unwrap();

        assert_eq!(map.device_for(1.into()).is_none(), true);
    }
}
