use crate::error::Result;
use crate::virtdev::{
    DeviceEvent, DeviceEventResponse, DeviceRegion, EmulatedDevice, Event, Port,
};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryInto;
use spin::RwLock;

pub struct DebugPort {
    port: Port,
}

impl DebugPort {
    pub fn new(port: Port) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self { port }))
    }
}

impl EmulatedDevice for DebugPort {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(_port, mut val) => {
                val.copy_from_u32(0xe9);
            }
            DeviceEvent::PortWrite(_port, val) => {
                event.responses.push(
                    DeviceEventResponse::GuestUartTransmitted(val.try_into()?),
                );
            }
            _ => (),
        }

        Ok(())
    }
}
