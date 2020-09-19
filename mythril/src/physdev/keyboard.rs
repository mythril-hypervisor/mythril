use crate::error::Result;
use bitflags::bitflags;
use x86::io::{inb, outb};

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

bitflags! {
    pub struct Ps2StatusFlags: u8 {
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
    pub struct Ps2ConfigurationFlags: u8 {
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

pub struct Ps2Controller;
impl Ps2Controller {
    pub fn init() -> Result<()> {
        // See https://wiki.osdev.org/%228042%22_PS/2_Controller#Initialising_the_PS.2F2_Controller
        Self::flush_read("init start");
        Self::write_command_port(Command::DisableFirst);
        Self::write_command_port(Command::DisableSecond);
        Self::flush_read("disable");

        {
            let mut config = Self::read_configuration();
            config.insert(Ps2ConfigurationFlags::FIRST_PORT_CLOCK_DISABLED);
            config.insert(Ps2ConfigurationFlags::SECOND_PORT_CLOCK_DISABLED);
            config.remove(Ps2ConfigurationFlags::FIRST_PORT_TRANSLATION);
            config.remove(Ps2ConfigurationFlags::FIRST_PORT_INTERRUPT);
            config.remove(Ps2ConfigurationFlags::SECOND_PORT_INTERRUPT);
            Self::write_configuration(config);
        }

        Self::write_command_port(Command::TestController);
        //TODO: these should return error
        assert_eq!(Self::read_data_port(), 0x55);

        Self::write_command_port(Command::EnableFirst);
        Self::write_command_port(Command::EnableSecond);
        Self::flush_read("enable");

        Self::write_data_port(0xff);
        //TODO: these should return error
        assert_eq!(Self::read_data_port(), 0xFA);
        assert_eq!(Self::read_data_port(), 0xAA);

        Self::flush_read("defaults");

        {
            let mut config = Self::read_configuration();
            config.remove(Ps2ConfigurationFlags::FIRST_PORT_CLOCK_DISABLED);
            config.remove(Ps2ConfigurationFlags::SECOND_PORT_CLOCK_DISABLED);
            config.insert(Ps2ConfigurationFlags::FIRST_PORT_TRANSLATION);
            config.insert(Ps2ConfigurationFlags::FIRST_PORT_INTERRUPT);
            config.insert(Ps2ConfigurationFlags::SECOND_PORT_INTERRUPT);
            Self::write_configuration(config);
        }

        Self::flush_read("init finished");
        Ok(())
    }

    fn flush_read(message: &str) {
        while Self::read_status_port()
            .contains(Ps2StatusFlags::OUTPUT_BUFFER_FULL)
        {
            info!("ps2: flush {}: {:X}", message, Self::read_data_port())
        }
    }

    fn read_data_port() -> u8 {
        Self::wait_read();
        unsafe { inb(PS2_DATA_PORT) }
    }

    fn write_data_port(data: u8) {
        Self::wait_write();
        unsafe {
            outb(PS2_DATA_PORT, data);
        }
    }

    fn read_status_port() -> Ps2StatusFlags {
        Ps2StatusFlags::from_bits_truncate(unsafe { inb(PS2_STATUS_PORT) })
    }

    fn read_configuration() -> Ps2ConfigurationFlags {
        Self::write_command_port(Command::ReadConfig);
        Ps2ConfigurationFlags::from_bits_truncate(Self::read_data_port())
    }

    fn write_configuration(config: Ps2ConfigurationFlags) {
        Self::write_command_port(Command::WriteConfig);
        Self::write_data_port(config.bits())
    }

    fn wait_read() {
        while !Self::read_status_port()
            .contains(Ps2StatusFlags::OUTPUT_BUFFER_FULL)
        {}
    }

    fn wait_write() {
        while Self::read_status_port()
            .contains(Ps2StatusFlags::INPUT_BUFFER_FULL)
        {}
    }

    fn write_command_port(cmd: Command) {
        unsafe {
            outb(PS2_COMMAND_PORT, cmd as u8);
        }
    }
}
