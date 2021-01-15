use crate::error::{Error, Result};
use crate::memory::{GuestAddressSpaceView, GuestPhysAddr};
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use arrayvec::ArrayVec;
use core::cmp::Ordering;
use core::convert::{TryFrom, TryInto};
use core::fmt;
use core::ops::RangeInclusive;
use spin::RwLock;

pub mod acpi;
pub mod com;
pub mod debug;
pub mod dma;
pub mod ignore;
pub mod ioapic;
pub mod keyboard;
pub mod lapic;
pub mod pci;
pub mod pic;
pub mod pit;
pub mod pos;
pub mod qemu_fw_cfg;
pub mod rtc;
pub mod vga;

const MAX_EVENT_RESPONSES: usize = 8;
pub type ResponseEventArray =
    ArrayVec<[DeviceEventResponse; MAX_EVENT_RESPONSES]>;
pub type Port = u16;

#[derive(Debug)]
pub enum DeviceEvent<'a> {
    HostUartReceived(u8),
    MemRead(GuestPhysAddr, MemReadRequest<'a>),
    MemWrite(GuestPhysAddr, MemWriteRequest<'a>),
    PortRead(Port, PortReadRequest<'a>),
    PortWrite(Port, PortWriteRequest<'a>),
}

#[derive(Debug)]
pub enum DeviceEventResponse {
    GuestUartTransmitted(u8),
    NextConsole,
    GSI(u32),
}

pub struct Event<'a> {
    pub kind: DeviceEvent<'a>,
    pub space: GuestAddressSpaceView<'a>,
    pub responses: &'a mut ResponseEventArray,
}

impl<'a> Event<'a> {
    pub fn new(
        kind: DeviceEvent<'a>,
        space: GuestAddressSpaceView<'a>,
        responses: &'a mut ResponseEventArray,
    ) -> Result<Self> {
        Ok(Event {
            kind,
            responses,
            space,
        })
    }
}

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
    fn find_device(
        self,
        map: &DeviceMap,
    ) -> Option<&Arc<RwLock<dyn EmulatedDevice>>>;
    fn find_device_mut(
        self,
        map: &mut DeviceMap,
    ) -> Option<&mut Arc<RwLock<dyn EmulatedDevice>>>;
}

impl DeviceInteraction for u16 {
    fn find_device(
        self,
        map: &DeviceMap,
    ) -> Option<&Arc<RwLock<dyn EmulatedDevice>>> {
        let range = PortIoRegion(RangeInclusive::new(self, self));
        map.portio_map.get(&range)
    }
    fn find_device_mut(
        self,
        map: &mut DeviceMap,
    ) -> Option<&mut Arc<RwLock<dyn EmulatedDevice>>> {
        let range = PortIoRegion(RangeInclusive::new(self, self));
        map.portio_map.get_mut(&range)
    }
}

impl DeviceInteraction for GuestPhysAddr {
    fn find_device(
        self,
        map: &DeviceMap,
    ) -> Option<&Arc<RwLock<dyn EmulatedDevice>>> {
        let range = MemIoRegion(RangeInclusive::new(self, self));
        map.memio_map.get(&range)
    }
    fn find_device_mut(
        self,
        map: &mut DeviceMap,
    ) -> Option<&mut Arc<RwLock<dyn EmulatedDevice>>> {
        let range = MemIoRegion(RangeInclusive::new(self, self));
        map.memio_map.get_mut(&range)
    }
}

/// A structure for looking up `EmulatedDevice`s by port or address
#[derive(Default)]
pub struct DeviceMap {
    portio_map: BTreeMap<PortIoRegion, Arc<RwLock<dyn EmulatedDevice>>>,
    memio_map: BTreeMap<MemIoRegion, Arc<RwLock<dyn EmulatedDevice>>>,
}

impl DeviceMap {
    /// Find the device that is responsible for handling an interaction
    pub fn find_device(
        &self,
        op: impl DeviceInteraction,
    ) -> Option<&Arc<RwLock<dyn EmulatedDevice>>> {
        op.find_device(self)
    }

    pub fn register_device(
        &mut self,
        dev: Arc<RwLock<dyn EmulatedDevice>>,
    ) -> Result<()> {
        let services = dev.read().services();
        for region in services.into_iter() {
            match region {
                DeviceRegion::PortIo(val) => {
                    let key = PortIoRegion(val);
                    if self.portio_map.contains_key(&key) {
                        let conflict = self
                            .portio_map
                            .get_key_value(&key)
                            .expect("Could not get conflicting device")
                            .0;

                        return Err(Error::InvalidDevice(format!(
                            "I/O Port already registered: 0x{:x}-0x{:x} conflicts with existing map of 0x{:x}-0x{:x}",
                            key.0.start(), key.0.end(), conflict.0.start(), conflict.0.end()
                        )));
                    }
                    self.portio_map.insert(key, dev.clone());
                }
                DeviceRegion::MemIo(val) => {
                    let key = MemIoRegion(val);
                    if self.memio_map.contains_key(&key) {
                        let conflict = self
                            .memio_map
                            .get_key_value(&key)
                            .expect("Could not get conflicting device")
                            .0;
                        return Err(Error::InvalidDevice(format!(
                            "Memory region already registered: 0x{:x}-0x{:x} conflicts with existing map of 0x{:x}-0x{:x}",
                            key.0.start().as_u64(), key.0.end().as_u64(), conflict.0.start().as_u64(), conflict.0.end().as_u64()
                        )));
                    }
                    self.memio_map.insert(key, dev.clone());
                }
            }
        }
        Ok(())
    }
}

pub trait EmulatedDevice: Send + Sync {
    fn services(&self) -> Vec<DeviceRegion>;

    fn on_event(&mut self, _event: Event) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum PortReadRequest<'a> {
    OneByte(&'a mut [u8; 1]),
    TwoBytes(&'a mut [u8; 2]),
    FourBytes(&'a mut [u8; 4]),
}

#[derive(Debug)]
pub enum PortWriteRequest<'a> {
    OneByte(&'a [u8; 1]),
    TwoBytes(&'a [u8; 2]),
    FourBytes(&'a [u8; 4]),
}

impl<'a> PortReadRequest<'a> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            &Self::OneByte(ref val) => *val,
            &Self::TwoBytes(ref val) => *val,
            &Self::FourBytes(ref val) => *val,
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            &mut Self::OneByte(ref mut val) => *val,
            &mut Self::TwoBytes(ref mut val) => *val,
            &mut Self::FourBytes(ref mut val) => *val,
        }
    }

    pub fn copy_from_u32(&mut self, val: u32) {
        let arr = val.to_be_bytes();
        let len = self.len();
        self.as_mut_slice().copy_from_slice(&arr[4 - len..]);
    }
}

impl<'a> TryFrom<&'a mut [u8]> for PortReadRequest<'a> {
    type Error = Error;

    fn try_from(buff: &'a mut [u8]) -> Result<Self> {
        let res = match buff.len() {
            1 => Self::OneByte(unsafe {
                &mut *(buff.as_mut_ptr() as *mut [u8; 1])
            }),
            2 => Self::TwoBytes(unsafe {
                &mut *(buff.as_mut_ptr() as *mut [u8; 2])
            }),
            4 => Self::FourBytes(unsafe {
                &mut *(buff.as_mut_ptr() as *mut [u8; 4])
            }),
            len => {
                return Err(Error::InvalidValue(format!(
                    "Invalid slice length: {}",
                    len
                )))
            }
        };
        Ok(res)
    }
}

impl<'a> fmt::Display for PortReadRequest<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneByte(arr) => {
                write!(f, "PortReadRequest([0x{:x}])", arr[0])
            }
            Self::TwoBytes(arr) => {
                write!(f, "PortReadRequest([0x{:x}, 0x{:x}])", arr[0], arr[1])
            }
            Self::FourBytes(arr) => write!(
                f,
                "PortReadRequest([0x{:x}, 0x{:x}, 0x{:x}, 0x{:x}])",
                arr[0], arr[1], arr[2], arr[3]
            ),
        }
    }
}

impl<'a> PortWriteRequest<'a> {
    pub fn as_slice(&self) -> &'a [u8] {
        match *self {
            Self::OneByte(val) => val,
            Self::TwoBytes(val) => val,
            Self::FourBytes(val) => val,
        }
    }

    pub fn as_u32(&self) -> u32 {
        let arr = match self {
            Self::OneByte(val) => [0, 0, 0, val[0]],
            Self::TwoBytes(val) => [0, 0, val[0], val[1]],
            Self::FourBytes(val) => *val.clone(),
        };
        u32::from_be_bytes(arr)
    }
}

impl<'a> TryFrom<&'a [u8]> for PortWriteRequest<'a> {
    type Error = Error;

    fn try_from(buff: &'a [u8]) -> Result<Self> {
        let res = match buff.len() {
            1 => Self::OneByte(unsafe { &*(buff.as_ptr() as *const [u8; 1]) }),
            2 => Self::TwoBytes(unsafe { &*(buff.as_ptr() as *const [u8; 2]) }),
            4 => {
                Self::FourBytes(unsafe { &*(buff.as_ptr() as *const [u8; 4]) })
            }
            len => {
                return Err(Error::InvalidValue(format!(
                    "Invalid slice length: {}",
                    len
                )))
            }
        };
        Ok(res)
    }
}

impl<'a> TryFrom<PortWriteRequest<'a>> for u8 {
    type Error = Error;

    fn try_from(value: PortWriteRequest<'a>) -> Result<Self> {
        match value {
            PortWriteRequest::OneByte(val) => Ok(val[0]),
            val => Err(Error::InvalidValue(format!(
                "Value {} cannot be converted to u8",
                val
            ))),
        }
    }
}

impl<'a> TryFrom<PortWriteRequest<'a>> for u16 {
    type Error = Error;

    fn try_from(value: PortWriteRequest<'a>) -> Result<Self> {
        match value {
            PortWriteRequest::TwoBytes(val) => Ok(u16::from_be_bytes(*val)),
            val => Err(Error::InvalidValue(format!(
                "Value {} cannot be converted to u16",
                val
            ))),
        }
    }
}

impl<'a> TryFrom<PortWriteRequest<'a>> for u32 {
    type Error = Error;

    fn try_from(value: PortWriteRequest<'a>) -> Result<Self> {
        match value {
            PortWriteRequest::FourBytes(val) => Ok(u32::from_be_bytes(*val)),
            val => Err(Error::InvalidValue(format!(
                "Value {} cannot be converted to u32",
                val
            ))),
        }
    }
}

impl<'a> fmt::Display for PortWriteRequest<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneByte(arr) => {
                write!(f, "PortWriteRequest([0x{:x}])", arr[0])
            }
            Self::TwoBytes(arr) => {
                write!(f, "PortWriteRequest([0x{:x}, 0x{:x}])", arr[0], arr[1])
            }
            Self::FourBytes(arr) => write!(
                f,
                "PortWriteRequest([0x{:x}, 0x{:x}, 0x{:x}, 0x{:x}])",
                arr[0], arr[1], arr[2], arr[3]
            ),
        }
    }
}

pub struct MemWriteRequest<'a> {
    data: &'a [u8],
}

impl fmt::Debug for MemWriteRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemWriteRequest")
            .field("data", &format_args!("{:02x?}", self.data))
            .finish()
    }
}

impl<'a> MemWriteRequest<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn as_slice(&self) -> &'a [u8] {
        self.data
    }
}

impl<'a> fmt::Display for MemWriteRequest<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemWriteRequest({:?})", self.data)
    }
}

impl<'a> TryInto<u8> for MemWriteRequest<'a> {
    type Error = Error;

    fn try_into(self) -> Result<u8> {
        if self.data.len() == 1 {
            Ok(self.data[0])
        } else {
            Err(Error::InvalidValue(format!(
                "Value {} cannot be converted to u8",
                self
            )))
        }
    }
}

#[derive(Debug)]
pub struct MemReadRequest<'a> {
    data: &'a mut [u8],
}

impl<'a> MemReadRequest<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self { data }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.data
    }
}

impl<'a> fmt::Display for MemReadRequest<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemReadRequest({:?})", self.data)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::virtdev::com::*;
    use core::convert::TryInto;

    // This is just a dummy device so we can have arbitrary port ranges
    // for testing.
    struct DummyDevice {
        services: Vec<RangeInclusive<Port>>,
    }

    impl DummyDevice {
        fn new(
            services: Vec<RangeInclusive<Port>>,
        ) -> Arc<RwLock<dyn EmulatedDevice>> {
            Arc::new(RwLock::new(Self { services }))
        }
    }

    impl EmulatedDevice for DummyDevice {
        fn services(&self) -> Vec<DeviceRegion> {
            self.services
                .iter()
                .map(|x| DeviceRegion::PortIo(x.clone()))
                .collect()
        }
    }

    #[test]
    fn test_device_map() {
        let mut map = DeviceMap::default();
        let com = Uart8250::new(0);
        map.register_device(com).unwrap();
        let _dev = map.find_device(0u16).unwrap();

        assert_eq!(map.find_device(10u16).is_none(), true);
    }

    #[test]
    fn test_write_request_try_from() {
        let val: Result<PortWriteRequest> =
            [0x12, 0x34, 0x56, 0x78][..].try_into();
        assert_eq!(val.is_ok(), true);

        let val: Result<PortWriteRequest> = [0x12, 0x34, 0x56][..].try_into();
        assert_eq!(val.is_err(), true);

        let val: PortWriteRequest =
            [0x12, 0x34, 0x56, 0x78][..].try_into().unwrap();
        assert_eq!(val.as_u32(), 0x12345678);

        let val: PortWriteRequest = [0x12, 0x34][..].try_into().unwrap();
        assert_eq!(val.as_u32(), 0x1234);
    }

    #[test]
    fn test_portio_value_read() {
        let mut arr = [0x00, 0x00];
        let mut val = PortReadRequest::TwoBytes(&mut arr);
        val.copy_from_u32(0x1234u32);
        assert_eq!([0x12, 0x34], val.as_slice());
        assert_eq!(0x1234, u16::from_be_bytes(arr));
    }

    #[test]
    fn test_conflicting_portio_device() {
        let mut map = DeviceMap::default();
        let com = Uart8250::new(0);
        map.register_device(com).unwrap();
        let com = Uart8250::new(0);

        assert!(map.register_device(com).is_err());
    }

    #[test]
    fn test_fully_overlapping_portio_device() {
        // region 2 fully inside region 1
        let services = vec![0..=10, 2..=8];
        let dummy = DummyDevice::new(services);
        let mut map = DeviceMap::default();

        assert!(map.register_device(dummy).is_err());
    }

    #[test]
    fn test_fully_encompassing_portio_device() {
        // region 1 fully inside region 2
        let services = vec![2..=8, 0..=10];
        let dummy = DummyDevice::new(services);
        let mut map = DeviceMap::default();

        assert!(map.register_device(dummy).is_err());
    }

    #[test]
    fn test_partially_overlapping_tail_portio_device() {
        // region 1 and region 2 partially overlap at the tail of region 1 and
        // the start of region 2
        let services = vec![0..=4, 3..=8];
        let dummy = DummyDevice::new(services);
        let mut map = DeviceMap::default();

        assert!(map.register_device(dummy).is_err());
    }

    #[test]
    fn test_partially_overlapping_head_portio_device() {
        // region 1 and region 2 partially overlap at the start of region 1 and
        // the tail of region 2
        let services = vec![3..=8, 0..=4];
        let dummy = DummyDevice::new(services);
        let mut map = DeviceMap::default();

        assert!(map.register_device(dummy).is_err());
    }

    #[test]
    fn test_non_overlapping_portio_device() {
        // region 1 and region 2 don't overlap
        let services = vec![0..=3, 4..=8];
        let dummy = DummyDevice::new(services);
        let mut map = DeviceMap::default();

        assert!(map.register_device(dummy).is_ok());
    }
}
