use crate::error::Result;
use crate::virtdev::{DeviceRegion, EmulatedDevice, Event};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

// In the future, we will just ignore all ports not associated with mapped devices,
// but for now, it is useful to explicitly ignore devices we don't need to emulate
// and fail when an unknown port is used.
#[derive(Default, Debug)]
pub struct IgnoredDevice;

impl IgnoredDevice {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self::default()))
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
            // Unused UART interfaces
            DeviceRegion::PortIo(0x2F8..=0x2F8 + 7),
            DeviceRegion::PortIo(0x3E8..=0x3E8 + 7),
            DeviceRegion::PortIo(0x2E8..=0x2E8 + 7),
            // Floppy disk controller
            DeviceRegion::PortIo(0x3f0..=0x3f7),
        ]
    }

    fn on_event(&mut self, _event: Event) -> Result<()> {
        Ok(())
    }
}
