use crate::device::{DeviceRegion, EmulatedDevice, Port};
use crate::error::{Error, Result};
use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Default, Debug)]
pub struct ProgrammableOptionSelect;

impl ProgrammableOptionSelect {
    const POS_ARBITRATION_CLOCK: Port = 0x90;
    const POS_CARD_SELECT_FEEDBACK: Port = 0x91;
    const POS_CONTROL_AND_STATUS: Port = 0x92;
    const POS_RESERVED_1: Port = 0x93;
    const POS_BOARD_ENABLE_SETUP: Port = 0x94;
    const POS_RESERVED_2: Port = 0x95;
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

    fn on_port_read(&mut self, _port: Port, val: &mut [u8]) -> Result<()> {
        let data = 0u32.to_be_bytes();
        val.copy_from_slice(&data[..val.len()]);
        Ok(())
    }

    fn on_port_write(&mut self, _port: Port, _val: &[u8]) -> Result<()> {
        Ok(())
    }
}
