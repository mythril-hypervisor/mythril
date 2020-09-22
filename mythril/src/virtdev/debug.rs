use crate::error::Result;
use crate::logger;
use crate::virtdev::{DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

pub struct DebugPort {
    id: u64,
    port: Port,
    buff: Vec<u8>,
}

impl DebugPort {
    pub fn new(vmid: u64, port: Port) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            port,
            buff: vec![],
            id: vmid,
        }))
    }
}

impl EmulatedDevice for DebugPort {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead((port, mut val)) => {
                val.copy_from_u32(0xe9);
            }
            DeviceEvent::PortWrite((port, val)) => {
                self.buff.extend_from_slice(val.as_slice());

                // Flush on newlines
                if val.as_slice().iter().filter(|b| **b == 10).next().is_some()
                {
                    let s = String::from_utf8_lossy(&self.buff);

                    logger::write_console(&format!("GUEST{}: {}", self.id, s));
                    self.buff.clear();
                }
            }
            _ => (),
        }

        Ok(())
    }
}
