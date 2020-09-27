#![deny(missing_docs)]

//! # Physical PS2 Support
//!
//! This module provides support for interacting with a physical
//! PS2 keyboard controller.

use crate::error::{Error, Result};
use bitflags::bitflags;
use x86::io::{inb, outb};

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

bitflags! {
    struct Ps2StatusFlags: u8 {
        const OUTPUT_BUFFER_FULL = 1 << 0;
        const INPUT_BUFFER_FULL = 1 << 1;
        const SELF_TEST_PASS = 1 << 2;
        const INPUT_FOR_CONTROLLER = 1 << 3;
        const RESERVED_1 = 1 << 4;
        const RESERVED_2 = 1 << 5;
        const TIMEOUT_ERROR = 1 << 6;
        const PARITY_ERROR = 1 << 7;
    }
}

bitflags! {
    struct Ps2ConfigurationFlags: u8 {
        const FIRST_PORT_INTERRUPT = 1 << 0;
        const SECOND_PORT_INTERRUPT = 1 << 1;
        const SYSTEM_POST = 1 << 2;
        const RESERVED_1 = 1 << 3;
        const FIRST_PORT_CLOCK_DISABLED = 1 << 4;
        const SECOND_PORT_CLOCK_DISABLED = 1 << 5;
        const FIRST_PORT_TRANSLATION = 1 << 6;
        const RESERVED_2 = 1 << 7;
    }
}

#[repr(u8)]
#[allow(dead_code)]
enum Command {
    ReadConfig = 0x20,
    WriteConfig = 0x60,
    DisableSecond = 0xA7,
    EnableSecond = 0xA8,
    TestSecond = 0xA9,
    TestController = 0xAA,
    TestFirst = 0xAB,
    Diagnostic = 0xAC,
    DisableFirst = 0xAD,
    EnableFirst = 0xAE,
    WriteSecond = 0xD4,
}

/// A representation of a physical PS2 keyboard controller
///
/// Note that currently only one instance of this type should be created.
pub struct Ps2Controller;

impl Ps2Controller {
    /// Create a new `Ps2Controller` and prepare the controller
    pub fn init() -> Result<Self> {
        // See https://wiki.osdev.org/%228042%22_PS/2_Controller#Initialising_the_PS.2F2_Controller
        let mut controller = Ps2Controller {};
        controller.flush_read("init start");
        controller.write_command_port(Command::DisableFirst);
        controller.write_command_port(Command::DisableSecond);
        controller.flush_read("disable");

        {
            let mut config = controller.read_configuration();
            config.insert(Ps2ConfigurationFlags::FIRST_PORT_CLOCK_DISABLED);
            config.insert(Ps2ConfigurationFlags::SECOND_PORT_CLOCK_DISABLED);
            config.remove(Ps2ConfigurationFlags::FIRST_PORT_TRANSLATION);
            config.remove(Ps2ConfigurationFlags::FIRST_PORT_INTERRUPT);
            config.remove(Ps2ConfigurationFlags::SECOND_PORT_INTERRUPT);
            controller.write_configuration(config);
        }

        controller.write_command_port(Command::TestController);

        if controller.read_data_port() != 0x55 {
            return Err(Error::DeviceError(
                "Failed to init Ps2Controller".into(),
            ));
        }

        controller.write_command_port(Command::EnableFirst);
        controller.write_command_port(Command::EnableSecond);
        controller.flush_read("enable");

        controller.write_data_port(0xff);

        if controller.read_data_port() != 0xFA {
            return Err(Error::DeviceError(
                "Failed to init Ps2Controller".into(),
            ));
        }
        if controller.read_data_port() != 0xAA {
            return Err(Error::DeviceError(
                "Failed to init Ps2Controller".into(),
            ));
        }

        controller.flush_read("defaults");

        {
            let mut config = controller.read_configuration();
            config.remove(Ps2ConfigurationFlags::FIRST_PORT_CLOCK_DISABLED);
            config.remove(Ps2ConfigurationFlags::SECOND_PORT_CLOCK_DISABLED);
            config.insert(Ps2ConfigurationFlags::FIRST_PORT_TRANSLATION);
            config.insert(Ps2ConfigurationFlags::FIRST_PORT_INTERRUPT);
            config.insert(Ps2ConfigurationFlags::SECOND_PORT_INTERRUPT);
            controller.write_configuration(config);
        }

        controller.flush_read("init finished");
        Ok(controller)
    }

    fn flush_read(&mut self, message: &str) {
        while self
            .read_status_port()
            .contains(Ps2StatusFlags::OUTPUT_BUFFER_FULL)
        {
            info!("ps2: flush {}: {:X}", message, self.read_data_port())
        }
    }

    /// Read a pending byte of data from the controller
    pub fn read_data_port(&mut self) -> u8 {
        self.wait_read();
        unsafe { inb(PS2_DATA_PORT) }
    }

    fn write_data_port(&mut self, data: u8) {
        self.wait_write();
        unsafe {
            outb(PS2_DATA_PORT, data);
        }
    }

    fn read_status_port(&mut self) -> Ps2StatusFlags {
        Ps2StatusFlags::from_bits_truncate(unsafe { inb(PS2_STATUS_PORT) })
    }

    fn read_configuration(&mut self) -> Ps2ConfigurationFlags {
        self.write_command_port(Command::ReadConfig);
        Ps2ConfigurationFlags::from_bits_truncate(self.read_data_port())
    }

    fn write_configuration(&mut self, config: Ps2ConfigurationFlags) {
        self.write_command_port(Command::WriteConfig);
        self.write_data_port(config.bits())
    }

    fn wait_read(&mut self) {
        while !self
            .read_status_port()
            .contains(Ps2StatusFlags::OUTPUT_BUFFER_FULL)
        {}
    }

    fn wait_write(&mut self) {
        while self
            .read_status_port()
            .contains(Ps2StatusFlags::INPUT_BUFFER_FULL)
        {}
    }

    fn write_command_port(&mut self, cmd: Command) {
        unsafe {
            outb(PS2_COMMAND_PORT, cmd as u8);
        }
    }
}
