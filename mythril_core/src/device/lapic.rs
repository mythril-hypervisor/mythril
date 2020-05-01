use crate::device::{
    DeviceRegion, EmulatedDevice, MemReadRequest, MemWriteRequest,
};
use crate::error::Result;
use crate::memory::{GuestAddressSpaceViewMut, GuestPhysAddr};
use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Default)]
pub struct LocalApic;

impl LocalApic {
    pub fn new() -> Box<Self> {
        Box::new(LocalApic::default())
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
                GuestPhysAddr::new(0xfed00000)..=GuestPhysAddr::new(0xfed010f0)
            ),

            //FIXME: this is actually the io apic
            DeviceRegion::MemIo(
                GuestPhysAddr::new(0xfec00000)..=GuestPhysAddr::new(0xfec010f0)
            )
        ]
    }

    fn on_mem_read(
        &mut self,
        addr: GuestPhysAddr,
        data: MemReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
        info!("local apic read of addr = {:?} (len=0x{:x})", addr, data.as_slice().len());
        Ok(())
    }

    fn on_mem_write(
        &mut self,
        addr: GuestPhysAddr,
        data: MemWriteRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
        info!("local apic write of addr = {:?} (data={:?})", addr, data);
        Ok(())
    }
}
