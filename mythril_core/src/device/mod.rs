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
pub mod qemu_fw_cfg;
pub mod rtc;

pub type Port = u16;

#[derive(Eq, PartialEq)]
struct PortIoRegion(RangeInclusive<Port>);

#[derive(Eq, PartialEq)]
struct MemIoRegion(RangeInclusive<GuestPhysAddr>);

impl PartialOrd for PortIoRegion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PortIoRegion {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0.end() < other.0.start() {
            Ordering::Less
        } else if other.0.end() < self.0.start() {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl PartialOrd for MemIoRegion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MemIoRegion {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0.end() < other.0.start() {
            Ordering::Less
        } else if other.0.end() < self.0.start() {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

pub enum DeviceRegion {
    PortIo(RangeInclusive<Port>),
    MemIo(RangeInclusive<GuestPhysAddr>),
}

pub trait DeviceInteraction {
    fn find_device(self, map: &DeviceMap) -> Option<&Box<dyn EmulatedDevice>>;
    fn find_device_mut(self, map: &mut DeviceMap) -> Option<&mut Box<dyn EmulatedDevice>>;
}

impl DeviceInteraction for u16 {
    fn find_device(self, map: &DeviceMap) -> Option<&Box<dyn EmulatedDevice>> {
        let range = PortIoRegion(RangeInclusive::new(self, self));
        map.portio_map.get(&range).map(|v| &**v)
    }
    fn find_device_mut(self, map: &mut DeviceMap) -> Option<&mut Box<dyn EmulatedDevice>> {
        let range = PortIoRegion(RangeInclusive::new(self, self));
        //NOTE: This is safe because all of the clones will exist in the same DeviceMap,
        //      so there cannot be other outstanding references
        map.portio_map
            .get_mut(&range)
            .map(|v| unsafe { Rc::get_mut_unchecked(v) })
    }
}

impl DeviceInteraction for GuestPhysAddr {
    fn find_device(self, map: &DeviceMap) -> Option<&Box<dyn EmulatedDevice>> {
        let range = MemIoRegion(RangeInclusive::new(self, self));
        map.memio_map.get(&range).map(|v| &**v)
    }
    fn find_device_mut(self, map: &mut DeviceMap) -> Option<&mut Box<dyn EmulatedDevice>> {
        let range = MemIoRegion(RangeInclusive::new(self, self));
        map.memio_map
            .get_mut(&range)
            .map(|v| unsafe { Rc::get_mut_unchecked(v) })
    }
}

#[derive(Default)]
pub struct DeviceMap {
    portio_map: BTreeMap<PortIoRegion, Rc<Box<dyn EmulatedDevice>>>,
    memio_map: BTreeMap<MemIoRegion, Rc<Box<dyn EmulatedDevice>>>,
}

impl DeviceMap {
    pub fn device_for(&self, op: impl DeviceInteraction) -> Option<&Box<dyn EmulatedDevice>> {
        op.find_device(self)
    }

    pub fn device_for_mut(
        &mut self,
        op: impl DeviceInteraction,
    ) -> Option<&mut Box<dyn EmulatedDevice>> {
        op.find_device_mut(self)
    }

    //TODO: detect conflicts
    pub fn register_device(&mut self, dev: Box<dyn EmulatedDevice>) -> Result<()> {
        let services = dev.services();
        let dev = Rc::new(dev);
        for region in services.into_iter() {
            match region {
                DeviceRegion::PortIo(val) => {
                    let key = PortIoRegion(val);
                    self.portio_map.insert(key, Rc::clone(&dev));
                }
                DeviceRegion::MemIo(val) => {
                    let key = MemIoRegion(val);
                    self.memio_map.insert(key, Rc::clone(&dev));
                }
            }
        }
        Ok(())
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
    fn on_port_read(&mut self, _port: Port, _val: &mut [u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "PortIo device does not support reading".into(),
        ))
    }
    fn on_port_write(&mut self, _port: Port, _val: &[u8]) -> Result<()> {
        Err(Error::NotImplemented(
            "PortIo device does not support writing".into(),
        ))
    }
}

pub struct ComDevice {
    port: Port,
    buff: Vec<u8>,
}

impl ComDevice {
    pub fn new(port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self { port, buff: vec![] })
    }
}

impl EmulatedDevice for ComDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_port_read(&mut self, _port: Port, val: &mut [u8]) -> Result<()> {
        // This is a magical value (called BOCHS_DEBUG_PORT_MAGIC by edk2)
        // FIXME: this should only be returned for a special 'debug' com device
        val[0] = 0xe9;
        Ok(())
    }

    fn on_port_write(&mut self, _port: Port, val: &[u8]) -> Result<()> {
        self.buff.extend_from_slice(val);

        // Flush on newlines
        if val.iter().filter(|b| **b == 10).next().is_some() {
            // TODO: I'm not sure what the correct behavior is here for a Com device.
            //       For now, just print each byte (except NUL because that crashes)
            let s: String = String::from_utf8_lossy(&self.buff)
                .into_owned()
                .chars()
                .filter(|c| *c != (0 as char))
                .collect();

            // FIXME: for now print guest output with some newlines to make
            //        it a bit more visible
            info!("\n\nGUEST: {}\n\n", s);
            self.buff.clear();
        }

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
        map.register_device(com).unwrap();
        let dev = map.device_for(0u16).unwrap();

        assert_eq!(map.device_for(1u16).is_none(), true);
    }
}
