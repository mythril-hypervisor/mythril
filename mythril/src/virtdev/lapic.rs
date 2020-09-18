use crate::error::Result;
use crate::memory::{GuestAddressSpaceViewMut, GuestPhysAddr};
use crate::virtdev::{
    DeviceRegion, EmulatedDevice, InterruptArray, MemReadRequest,
    MemWriteRequest,
};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Default)]
pub struct LocalApic;

impl LocalApic {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(LocalApic::default()))
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

    fn on_mem_read(
        &mut self,
        addr: GuestPhysAddr,
        data: MemReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<InterruptArray> {
        info!(
            "local apic read of addr = {:?} (len=0x{:x})",
            addr,
            data.as_slice().len()
        );
        Ok(InterruptArray::default())
    }

    fn on_mem_write(
        &mut self,
        addr: GuestPhysAddr,
        data: MemWriteRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<InterruptArray> {
        info!("local apic write of addr = {:?} (data={:?})", addr, data);
        Ok(InterruptArray::default())
    }
}
