use crate::device::{
    DeviceRegion, EmulatedDevice, Port, PortReadRequest, PortWriteRequest,
};
use crate::error::Result;
use crate::memory::GuestAddressSpaceViewMut;
use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Default, Debug)]
pub struct Keyboard8042;

impl Keyboard8042 {
    const PS2_DATA: Port = 0x0060;
    const PS2_STATUS: Port = 0x0064;

    pub fn new() -> Box<Self> {
        Box::new(Self::default())
    }
}

impl EmulatedDevice for Keyboard8042 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::PS2_DATA..=Self::PS2_DATA),
            DeviceRegion::PortIo(Self::PS2_STATUS..=Self::PS2_STATUS),
        ]
    }

    fn on_port_read(
        &mut self,
        _port: Port,
        mut val: PortReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
        //FIXME: For now just return 0xff for everything
        val.copy_from_u32(0xff);
        Ok(())
    }

    fn on_port_write(
        &mut self,
        _port: Port,
        _val: PortWriteRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
        Ok(())
    }
}
