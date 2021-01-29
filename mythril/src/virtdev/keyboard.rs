use crate::virtdev::{DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port};
use crate::{error::Result, vm::VirtualMachineConfig};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

#[derive(Default, Debug)]
pub struct Keyboard8042;

impl Keyboard8042 {
    const PS2_DATA: Port = 0x0060;
    const PS2_STATUS: Port = 0x0064;

    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self::default()))
    }
}

impl EmulatedDevice for Keyboard8042 {
    fn services(&self, _vm_config: &VirtualMachineConfig) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::PS2_DATA..=Self::PS2_DATA),
            DeviceRegion::PortIo(Self::PS2_STATUS..=Self::PS2_STATUS),
        ]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(_port, mut val) => {
                //FIXME: For now just return 0xff for everything
                val.copy_from_u32(0xff);
            }
            _ => (),
        }
        Ok(())
    }
}
