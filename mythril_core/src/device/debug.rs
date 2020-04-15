use crate::device::{
    DeviceRegion, EmulatedDevice, Port, PortReadRequest, PortWriteRequest,
};
use crate::error::Result;
use crate::logger;
use crate::memory::GuestAddressSpaceViewMut;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub struct DebugPort {
    id: u64,
    port: Port,
    buff: Vec<u8>,
}

impl DebugPort {
    pub fn new(vmid: u64, port: Port) -> Box<dyn EmulatedDevice> {
        Box::new(Self {
            port,
            buff: vec![],
            id: vmid,
        })
    }
}

impl EmulatedDevice for DebugPort {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(self.port..=self.port)]
    }

    fn on_port_read(
        &mut self,
        _port: Port,
        mut val: PortReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
        // This is a magical value (called BOCHS_DEBUG_PORT_MAGIC by edk2)
        val.copy_from_u32(0xe9);
        Ok(())
    }

    fn on_port_write(
        &mut self,
        _port: Port,
        val: PortWriteRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
        self.buff.extend_from_slice(val.as_slice());

        // Flush on newlines
        if val.as_slice().iter().filter(|b| **b == 10).next().is_some() {
            let s = String::from_utf8_lossy(&self.buff);

            logger::write_console(&format!("GUEST{}: {}", self.id, s));
            self.buff.clear();
        }
        Ok(())
    }
}
