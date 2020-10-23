use crate::error::Result;
use crate::memory::GuestPhysAddr;
use crate::virtdev::{DeviceRegion, EmulatedDevice, Event};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

#[derive(Default)]
pub struct LocalApic;

impl LocalApic {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(LocalApic::default()))
    }
}

impl EmulatedDevice for LocalApic {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::MemIo(
                GuestPhysAddr::new(0xfee00000)..=GuestPhysAddr::new(0xfee010f0),
            ),
            //FIXME: this is actually the 1st HPET
            DeviceRegion::MemIo(
                GuestPhysAddr::new(0xfed00000)..=GuestPhysAddr::new(0xfed010f0),
            ),
            //FIXME: this is actually the io apic
            DeviceRegion::MemIo(
                GuestPhysAddr::new(0xfec00000)..=GuestPhysAddr::new(0xfec010f0),
            ),
        ]
    }

    fn on_event(&mut self, _event: Event) -> Result<()> {
        Ok(())
    }
}
