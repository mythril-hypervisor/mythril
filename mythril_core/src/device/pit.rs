use crate::device::{
    DeviceRegion, EmulatedDevice, Port, PortReadRequest, PortWriteRequest,
};
use crate::error::{Error, Result};
use crate::memory::GuestAddressSpaceViewMut;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::convert::TryFrom;
use derive_try_from_primitive::TryFromPrimitive;

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum Channel {
    Channel0 = 0b00,
    Channel1 = 0b01,
    Channel2 = 0b10,
    ReadBack = 0b11,
}

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AccessMode {
    LatchCount = 0b00,
    LoByte = 0b01,
    HiByte = 0b10,
    Word = 0b11,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum OperatingMode {
    Mode0 = 0b000, // interrupt on terminal count
    Mode1 = 0b001, // hardware re-triggerable one-shot
    Mode2 = 0b010, // rate generator
    Mode3 = 0b011, // square wave generator
    Mode4 = 0b100, // software triggered strobe
    Mode5 = 0b101, // hardware triggered strobe
}

impl TryFrom<u8> for OperatingMode {
    type Error = Error;
    fn try_from(val: u8) -> Result<OperatingMode> {
        match val {
            0b000 => Ok(OperatingMode::Mode0),
            0b001 => Ok(OperatingMode::Mode1),
            0b010 => Ok(OperatingMode::Mode2),
            0b011 => Ok(OperatingMode::Mode3),
            0b100 => Ok(OperatingMode::Mode4),
            0b101 => Ok(OperatingMode::Mode5),
            0b110 => Ok(OperatingMode::Mode2),
            0b111 => Ok(OperatingMode::Mode3),
            _ => Err(Error::InvalidValue(format!(
                "Invalid PIT operating mode: {}",
                val
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum BinaryMode {
    Binary = 0b0,
    Bcd = 0b1,
}

#[derive(Default, Debug)]
pub struct Pit8254;

impl Pit8254 {
    pub const PIT_COUNTER_0: Port = 0x0040;
    pub const PIT_COUNTER_1: Port = 0x0041;
    pub const PIT_COUNTER_2: Port = 0x0042;
    pub const PIT_MODE_CONTROL: Port = 0x0043;

    pub const PIT_PS2_CTRL_B: Port = 0x0061;

    pub fn new() -> Box<Self> {
        Box::new(Pit8254::default())
    }
}

impl EmulatedDevice for Pit8254 {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(Self::PIT_COUNTER_0..=Self::PIT_MODE_CONTROL),
            DeviceRegion::PortIo(Self::PIT_PS2_CTRL_B..=Self::PIT_PS2_CTRL_B),
        ]
    }

    fn on_port_read(
        &mut self,
        _port: Port,
        _val: PortReadRequest,
        _space: GuestAddressSpaceViewMut,
    ) -> Result<()> {
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
