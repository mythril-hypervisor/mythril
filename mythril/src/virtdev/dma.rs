use crate::error::Result;
use crate::memory::GuestAddressSpaceViewMut;
use crate::virtdev::{
    DeviceRegion, EmulatedDevice, InterruptArray, Port, PortReadRequest,
    PortWriteRequest,
};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Default, Debug)]
pub struct Dma8237;

#[allow(dead_code)]
impl Dma8237 {
    const DMA1_CHAN2_ADDR: Port = 0x0004;
    const DMA1_CHAN2_COUNT: Port = 0x0005;
    const DMA1_MASK: Port = 0x000a;
    const DMA1_MODE: Port = 0x000b;
    const DMA1_CLEAR_FF: Port = 0x000c;
    const DMA1_MASTER_CLEAR: Port = 0x000d;

    // This order is correct. 2-3-1
    const DMA_CHAN2_PAGE_CHECK: Port = 0x0081;
    const DMA_CHAN3_PAGE_CHECK: Port = 0x0082;
    const DMA_CHAN1_PAGE_CHECK: Port = 0x0083;

    const DMA2_MASK: Port = 0x00d4;
    const DMA2_MODE: Port = 0x00d6;
    const DMA2_MASTER_CLEAR: Port = 0x00da;

    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Dma8237::default()))
    }
}

impl EmulatedDevice for Dma8237 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(
                Self::DMA1_CHAN2_ADDR..=Self::DMA1_MASTER_CLEAR,
            ),
            DeviceRegion::PortIo(
                Self::DMA_CHAN2_PAGE_CHECK..=Self::DMA_CHAN1_PAGE_CHECK,
            ),
            DeviceRegion::PortIo(Self::DMA2_MASK..=Self::DMA2_MASTER_CLEAR),
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
