use crate::error::Result;
use crate::memory::GuestPhysAddr;
use crate::virtdev::{DeviceRegion, EmulatedDevice, Event};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

#[derive(Default)]
pub struct Hpet;

impl Hpet {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Hpet {}))
    }
}

impl EmulatedDevice for Hpet {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::MemIo(
                GuestPhysAddr::new(0xfed00000)..=GuestPhysAddr::new(0xfed010f0),
            ),
        ]
    }

    fn on_event(&mut self, _event: Event) -> Result<()> {
        Ok(())
    }
}
