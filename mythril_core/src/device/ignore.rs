use crate::device::{
    DeviceRegion, EmulatedDevice, Port, PortReadRequest, PortWriteRequest,
};
use crate::error::Result;
use crate::memory::GuestAddressSpace;
use crate::vcpu::VCpu;
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
        ]
    }

    fn on_port_read(
        &mut self,
        _vcpu: &VCpu,
        _port: Port,
        _val: PortReadRequest,
        _space: &mut GuestAddressSpace,
    ) -> Result<()> {
        Ok(())
    }

    fn on_port_write(
        &mut self,
        _vcpu: &VCpu,
        _port: Port,
        _val: PortWriteRequest,
        _space: &mut GuestAddressSpace,
    ) -> Result<()> {
        Ok(())
    }
}
