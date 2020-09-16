use crate::device::{
    DeviceRegion, EmulatedDevice, InterruptArray, Port, PortReadRequest,
    PortWriteRequest,
};
use crate::error::Result;
use crate::memory::GuestAddressSpaceViewMut;
use alloc::boxed::Box;
use alloc::vec::Vec;

// In the future, we will just ignore all ports not associated with mapped devices,
// but for now, it is useful to explicitly ignore devices we don't need to emulate
// and fail when an unknown port is used.
#[derive(Default, Debug)]
pub struct IgnoredDevice;

impl IgnoredDevice {
    pub fn new() -> Box<Self> {
        Box::new(Self::default())
    }
}

impl EmulatedDevice for IgnoredDevice {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            // Ignore #IGNNE stuff
            DeviceRegion::PortIo(241..=241),
            DeviceRegion::PortIo(240..=240),
            // IO delay port
            DeviceRegion::PortIo(128..=128),
            //TODO: don't know what this is yet
            DeviceRegion::PortIo(135..=135),
        ]
    }

    fn on_port_read(
        &mut self,
        _port: Port,
        _val: PortReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<InterruptArray> {
        Ok(InterruptArray::default())
    }

    fn on_port_write(
        &mut self,
        _port: Port,
        _val: PortWriteRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<InterruptArray> {
        Ok(InterruptArray::default())
    }
}
