use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::Result;
use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Default, Debug)]
pub struct ProgrammableOptionSelect;

impl ProgrammableOptionSelect {
    const POS_ARBITRATION_CLOCK: Port = 0x90;
    const _POS_CARD_SELECT_FEEDBACK: Port = 0x91;
    const _POS_CONTROL_AND_STATUS: Port = 0x92;
    const _POS_RESERVED_1: Port = 0x93;
    const _POS_BOARD_ENABLE_SETUP: Port = 0x94;
    const _POS_RESERVED_2: Port = 0x95;
    const POS_ADAPTER_ENABLE_SETUP: Port = 0x96;

    pub fn new() -> Box<Self> {
        Box::new(ProgrammableOptionSelect::default())
    }
}

// Currently we don't actually implement any of this, but I don't think we
// need to either (kvm doesn't seem to)
impl EmulatedDevice for ProgrammableOptionSelect {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![DeviceRegion::PortIo(
            Self::POS_ARBITRATION_CLOCK..=Self::POS_ADAPTER_ENABLE_SETUP,
        )]
    }

    fn on_port_read(
        &mut self,
        _port: Port,
        val: &mut PortIoValue,
    ) -> Result<()> {
        val.copy_from_u32(0);
        Ok(())
    }

    fn on_port_write(&mut self, _port: Port, _val: PortIoValue) -> Result<()> {
        Ok(())
    }
}
