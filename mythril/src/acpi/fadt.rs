use super::rsdt::SDT;
use super::GenericAddressStructure;
use crate::error::Result;
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;

mod offsets {
    use super::*;
    pub const FIRMWARE_CTRL: Range<usize> = 0..4;
    pub const DSDT: Range<usize> = 4..8;
    pub const PREFERRED_POWER_MANAGEMENT_PROFILE: usize = 9;
    pub const SCI_INTERRUPT: Range<usize> = 10..12;
    pub const SMI_COMMAND_PORT: Range<usize> = 12..16;
    pub const ACPI_ENABLE: usize = 16;
    pub const ACPI_DISABLE: usize = 17;
    pub const S4_BIOS_REQ: usize = 18;
    pub const PSTATE_CONTROL: usize = 19;
    pub const PM1A_EVENT_BLOCK: Range<usize> = 20..24;
    pub const PM1B_EVENT_BLOCK: Range<usize> = 24..28;
    pub const PM1A_CONTROL_BLOCK: Range<usize> = 28..32;
    pub const PM1B_CONTROL_BLOCK: Range<usize> = 32..36;
    pub const PM2_CONTROL_BLOCK: Range<usize> = 36..40;
    pub const PM_TIMER_BLOCK: Range<usize> = 40..44;
    pub const GPE0_BLOCK: Range<usize> = 44..48;
    pub const GPE1_BLOCK: Range<usize> = 48..52;
    pub const PM1_EVENT_LENGTH: usize = 52;
    pub const PM1_CONTROL_LENGTH: usize = 53;
    pub const PM2_CONTROL_LENGTH: usize = 54;
    pub const PM_TIMER_LENGTH: usize = 55;
    pub const GPE0_LENGTH: usize = 56;
    pub const GPE1_LENGTH: usize = 57;
    pub const GPE1_BASE: usize = 58;
    pub const CSTATE_CONTROL: usize = 59;
    pub const WORST_C2_LATENCY: Range<usize> = 60..62;
    pub const WORST_C3_LATENCY: Range<usize> = 62..64;
    pub const FLUSH_SIZE: Range<usize> = 64..66;
    pub const FLUSH_STRIDE: Range<usize> = 66..68;
    pub const DUTY_OFFSET: usize = 68;
    pub const DUTY_WIDTH: usize = 69;
    pub const DAY_ALARM: usize = 70;
    pub const MONTH_ALARM: usize = 71;
    pub const CENTURY: usize = 72;
    pub const BOOT_ARCHITECTURE_FLAGS: Range<usize> = 73..75;
    pub const FLAGS: Range<usize> = 76..80;
    pub const RESET_REG: Range<usize> = 80..92;
    pub const RESET_VALUE: usize = 92;
    pub const ARM_BOOT_ARCHITECTURE_FLAGS: Range<usize> = 93..95;
    pub const FADT_MINOR_VERSION: usize = 95;
    pub const X_FIRMWARE_CONTROL: Range<usize> = 96..104;
    pub const X_DSDT: Range<usize> = 104..112;
    pub const X_PM1A_EVENT_BLOCK: Range<usize> = 112..124;
    pub const X_PM1B_EVENT_BLOCK: Range<usize> = 124..136;
    pub const X_PM1A_CONTROL_BLOCK: Range<usize> = 136..148;
    pub const X_PM1B_CONTROL_BLOCK: Range<usize> = 148..160;
    pub const X_PM2_CONTROL_BLOCK: Range<usize> = 160..172;
    pub const X_PM_TIMERBLOCK: Range<usize> = 172..184;
    pub const X_GPE0_BLOCK: Range<usize> = 184..196;
    pub const X_GPE1_BLOCK: Range<usize> = 196..208;
}

/// Fixed ACPI Description Table
///
/// See `ACPI § 6.3 Table 5-33`.
pub struct FADT<'a> {
    sdt: &'a SDT<'a>,
    /// Physical memory address of the FACS.
    pub firmware_ctrl: Option<u32>,
    /// Physical memory address of the DSDT.
    pub dsdt: Option<u32>,
    /// An OEM setting used to convey the preferred power management profile.
    /// See ACPI spec for field values.
    pub preferred_power_management_profile: Option<u8>,
    /// System vector the SCI interrupt is wired to.
    pub sci_interrupt: Option<u16>,
    /// System port address of the SMI command port.
    pub smi_command_port: Option<u32>,
    /// The value to write to smi_command_port to disable SMI ownership of the
    /// ACPI hardware registers.
    pub acpi_enable: Option<u8>,
    /// The value to write to smi_command_port to re-enable SMI ownership of
    /// the ACPI hardware registers.
    pub acpi_disable: Option<u8>,
    /// The value to write to smi_command_port to enter S4BIOS state.
    pub s4_bios_req: Option<u8>,
    /// If non-zero, this field contains the value to be written to
    /// smi_command_port to assume processor performance state control
    /// responsibility.
    pub pstate_control: Option<u8>,
    /// System port address of the PM1a event register block.
    pub pm1a_event_block: Option<u32>,
    /// System port address of the PM1b event register block.
    pub pm1b_event_block: Option<u32>,
    /// System port address of the PM1a control register block.
    pub pm1a_control_block: Option<u32>,
    /// System port address of the PM1b control register block.
    pub pm1b_control_block: Option<u32>,
    /// System port address of the PM2 control register block.
    pub pm2_control_block: Option<u32>,
    /// System port address of the power management timer control register
    /// block.
    pub pm_timer_block: Option<u32>,
    /// System port address of the general-purpose event 0 register block.
    pub gpe0_block: Option<u32>,
    /// System port address of the general-purpose event 1 register block.
    pub gpe1_block: Option<u32>,
    /// Number of bytes decoded by pm1a_event_block and pm1b_event_block.
    pub pm1_event_length: Option<u8>,
    /// Number of bytes decoded by pm1a_control_block and pm1b_control_block.
    pub pm1_control_length: Option<u8>,
    /// Number of bytes decoded by pm2_control_block.
    pub pm2_control_length: Option<u8>,
    /// Number of bytes decoded by pm_timer_block.
    pub pm_timer_length: Option<u8>,
    /// Number of bytes decoded by gpe0_block.
    pub gpe0_length: Option<u8>,
    /// Number of bytes decoded by gpe1_block.
    pub gpe1_length: Option<u8>,
    /// Offset within the ACPI general-purpose event model where GPE1 based
    /// events start.
    pub gpe1_base: Option<u8>,
    /// If non-zero, this is the value to write to smi_command_port to indicate
    /// OS support for the _CST object.
    pub cstate_control: Option<u8>,
    /// The worst-case hardware latency, in microseconds, to enter and exit a
    /// C2 state. A value > 100 indicates the system does not support a C2
    /// state.
    pub worst_c2_latency: Option<u16>,
    /// The worst-case hardware latency, in microseconds, to enter and exit a
    /// C3 state. A value > 1000 indicates the system does not support a C3
    /// state.
    pub worst_c3_latency: Option<u16>,
    /// The number of flush strides that need to be read to completely flush
    /// dirty lines from any processor's memory caches.
    pub flush_size: Option<u16>,
    /// The cache line width in bytes of the processor's memory caches.
    pub flush_stride: Option<u16>,
    /// The zero-based index of where the processor's duty cycle setting is
    /// within the processor's P_CNT register.
    pub duty_offset: Option<u8>,
    /// The bit width of the processor's duty cycle setting value in the P_CNT
    /// register.
    pub duty_width: Option<u8>,
    /// The RTC CMOS RAM index to the day-of-month alarm value.
    pub day_alarm: Option<u8>,
    /// The RTC CMOS RAM index to the month of year alarm value.
    pub month_alarm: Option<u8>,
    /// The RTC CMOS RAM index to the century of data value.
    pub century: Option<u8>,
    /// The IA-PC boot architecture flags. (See ACPI § 6.3 Table 5-35)
    pub boot_architecture_flags: Option<u16>,
    /// Fixed feature flags. (See ACPI § 6.3 Table 5-34)
    pub flags: Option<u32>,
    /// The address of the reset register.
    pub reset_reg: Option<GenericAddressStructure>,
    /// The value to write to reset_reg port to reset the system.
    pub reset_value: Option<u8>,
    /// ARM boot architecture flags. (See ACPI § 6.3 Table 5-36)
    pub arm_boot_architecture_flags: Option<u16>,
    /// FADT minor version in "Major.Minor" form.
    pub fadt_minor_version: Option<u8>,
    /// Extended physical address of the FACS.
    pub x_firmware_control: Option<u64>,
    /// Extended physical address of the DSDT.
    pub x_dsdt: Option<u64>,
    /// Extended physical address of the PM1a event register block.
    pub x_pm1a_event_block: Option<GenericAddressStructure>,
    /// Extended physical address of the PM1b event register block.
    pub x_pm1b_event_block: Option<GenericAddressStructure>,
    /// Extended physical address of the PM1a control register block.
    pub x_pm1a_control_block: Option<GenericAddressStructure>,
    /// Extended physical address of the PM1b control register block.
    pub x_pm1b_control_block: Option<GenericAddressStructure>,
    /// Extended physical address of the PM2 control register block.
    pub x_pm2_control_block: Option<GenericAddressStructure>,
    /// Extended physical address of the power management timer control
    /// register block.
    pub x_pm_timerblock: Option<GenericAddressStructure>,
    /// Extended physical address of the general-purpose event 0 register block.
    pub x_gpe0_block: Option<GenericAddressStructure>,
    /// Extended physical address of the general-purpose event 1 register block.
    pub x_gpe1_block: Option<GenericAddressStructure>,
}

impl<'a> FADT<'a> {
    /// Create a new FADT given a SDT.
    pub fn new(sdt: &'a SDT<'a>) -> Result<FADT<'a>> {
        let firmware_ctrl = sdt
            .table
            .get(offsets::FIRMWARE_CTRL)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let dsdt = sdt
            .table
            .get(offsets::DSDT)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let preferred_power_management_profile = sdt
            .table
            .get(offsets::PREFERRED_POWER_MANAGEMENT_PROFILE)
            .and_then(|num| Some(*num));
        let sci_interrupt = sdt
            .table
            .get(offsets::SCI_INTERRUPT)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let smi_command_port = sdt
            .table
            .get(offsets::SMI_COMMAND_PORT)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let acpi_enable = sdt
            .table
            .get(offsets::ACPI_ENABLE)
            .and_then(|num| Some(*num));
        let acpi_disable = sdt
            .table
            .get(offsets::ACPI_DISABLE)
            .and_then(|num| Some(*num));
        let s4_bios_req = sdt
            .table
            .get(offsets::S4_BIOS_REQ)
            .and_then(|num| Some(*num));
        let pstate_control = sdt
            .table
            .get(offsets::PSTATE_CONTROL)
            .and_then(|num| Some(*num));
        let pm1a_event_block = sdt
            .table
            .get(offsets::PM1A_EVENT_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let pm1b_event_block = sdt
            .table
            .get(offsets::PM1B_EVENT_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let pm1a_control_block = sdt
            .table
            .get(offsets::PM1A_CONTROL_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let pm1b_control_block = sdt
            .table
            .get(offsets::PM1B_CONTROL_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let pm2_control_block = sdt
            .table
            .get(offsets::PM2_CONTROL_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let pm_timer_block = sdt
            .table
            .get(offsets::PM_TIMER_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let gpe0_block = sdt
            .table
            .get(offsets::GPE0_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let gpe1_block = sdt
            .table
            .get(offsets::GPE1_BLOCK)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));
        let pm1_event_length = sdt
            .table
            .get(offsets::PM1_EVENT_LENGTH)
            .and_then(|num| Some(*num));
        let pm1_control_length = sdt
            .table
            .get(offsets::PM1_CONTROL_LENGTH)
            .and_then(|num| Some(*num));
        let pm2_control_length = sdt
            .table
            .get(offsets::PM2_CONTROL_LENGTH)
            .and_then(|num| Some(*num));
        let pm_timer_length = sdt
            .table
            .get(offsets::PM_TIMER_LENGTH)
            .and_then(|num| Some(*num));
        let gpe0_length = sdt
            .table
            .get(offsets::GPE0_LENGTH)
            .and_then(|num| Some(*num));
        let gpe1_length = sdt
            .table
            .get(offsets::GPE1_LENGTH)
            .and_then(|num| Some(*num));
        let gpe1_base =
            sdt.table.get(offsets::GPE1_BASE).and_then(|num| Some(*num));
        let cstate_control = sdt
            .table
            .get(offsets::CSTATE_CONTROL)
            .and_then(|num| Some(*num));
        let worst_c2_latency = sdt
            .table
            .get(offsets::WORST_C2_LATENCY)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let worst_c3_latency = sdt
            .table
            .get(offsets::WORST_C3_LATENCY)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let flush_size = sdt
            .table
            .get(offsets::FLUSH_SIZE)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let flush_stride = sdt
            .table
            .get(offsets::FLUSH_STRIDE)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let duty_offset = sdt
            .table
            .get(offsets::DUTY_OFFSET)
            .and_then(|num| Some(*num));
        let duty_width = sdt
            .table
            .get(offsets::DUTY_WIDTH)
            .and_then(|num| Some(*num));
        let day_alarm =
            sdt.table.get(offsets::DAY_ALARM).and_then(|num| Some(*num));
        let month_alarm = sdt
            .table
            .get(offsets::MONTH_ALARM)
            .and_then(|num| Some(*num));
        let century =
            sdt.table.get(offsets::CENTURY).and_then(|num| Some(*num));
        let boot_architecture_flags = sdt
            .table
            .get(offsets::BOOT_ARCHITECTURE_FLAGS)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let flags = sdt
            .table
            .get(offsets::FLAGS)
            .and_then(|bytes| Some(NativeEndian::read_u32(bytes)));

        let reset_reg = if let Some(bytes) = sdt.table.get(offsets::RESET_REG) {
            Some(GenericAddressStructure::new(bytes)?)
        } else {
            None
        };

        let reset_value = sdt
            .table
            .get(offsets::RESET_VALUE)
            .and_then(|num| Some(*num));
        let arm_boot_architecture_flags = sdt
            .table
            .get(offsets::ARM_BOOT_ARCHITECTURE_FLAGS)
            .and_then(|bytes| Some(NativeEndian::read_u16(bytes)));
        let fadt_minor_version = sdt
            .table
            .get(offsets::FADT_MINOR_VERSION)
            .and_then(|num| Some(*num));
        let x_firmware_control = sdt
            .table
            .get(offsets::X_FIRMWARE_CONTROL)
            .and_then(|bytes| Some(NativeEndian::read_u64(bytes)));
        let x_dsdt = sdt
            .table
            .get(offsets::X_DSDT)
            .and_then(|bytes| Some(NativeEndian::read_u64(bytes)));

        let x_pm1a_event_block =
            if let Some(bytes) = sdt.table.get(offsets::X_PM1A_EVENT_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_pm1b_event_block =
            if let Some(bytes) = sdt.table.get(offsets::X_PM1B_EVENT_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_pm1a_control_block =
            if let Some(bytes) = sdt.table.get(offsets::X_PM1A_CONTROL_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_pm1b_control_block =
            if let Some(bytes) = sdt.table.get(offsets::X_PM1B_CONTROL_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_pm2_control_block =
            if let Some(bytes) = sdt.table.get(offsets::X_PM2_CONTROL_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_pm_timerblock =
            if let Some(bytes) = sdt.table.get(offsets::X_PM_TIMERBLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_gpe0_block =
            if let Some(bytes) = sdt.table.get(offsets::X_GPE0_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        let x_gpe1_block =
            if let Some(bytes) = sdt.table.get(offsets::X_GPE1_BLOCK) {
                Some(GenericAddressStructure::new(bytes)?)
            } else {
                None
            };

        Ok(Self {
            sdt,
            firmware_ctrl,
            dsdt,
            preferred_power_management_profile,
            sci_interrupt,
            smi_command_port,
            acpi_enable,
            acpi_disable,
            s4_bios_req,
            pstate_control,
            pm1a_event_block,
            pm1b_event_block,
            pm1a_control_block,
            pm1b_control_block,
            pm2_control_block,
            pm_timer_block,
            gpe0_block,
            gpe1_block,
            pm1_event_length,
            pm1_control_length,
            pm2_control_length,
            pm_timer_length,
            gpe0_length,
            gpe1_length,
            gpe1_base,
            cstate_control,
            worst_c2_latency,
            worst_c3_latency,
            flush_size,
            flush_stride,
            duty_offset,
            duty_width,
            day_alarm,
            month_alarm,
            century,
            boot_architecture_flags,
            flags,
            reset_reg,
            reset_value,
            arm_boot_architecture_flags,
            fadt_minor_version,
            x_firmware_control,
            x_dsdt,
            x_pm1a_event_block,
            x_pm1b_event_block,
            x_pm1a_control_block,
            x_pm1b_control_block,
            x_pm2_control_block,
            x_pm_timerblock,
            x_gpe0_block,
            x_gpe1_block,
        })
    }
}

impl<'a> fmt::Debug for FADT<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.sdt)?;
        if let Some(dsdt) = self.dsdt {
            write!(f, " DSDT address=0x{:x}", dsdt)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::acpi::GenericAddressStructure;

    #[test]
    fn test_fadt_parse_v5() {
        // sample FADT entry taken from live machine with ACPI v5.0
        let buf = [
            0x46, 0x41, 0x43, 0x50, 0x0c, 0x01, 0x00, 0x00, 0x05, 0x86, 0x41,
            0x4c, 0x41, 0x53, 0x4b, 0x41, 0x41, 0x20, 0x4d, 0x20, 0x49, 0x00,
            0x00, 0x00, 0x09, 0x20, 0x07, 0x01, 0x41, 0x4d, 0x49, 0x20, 0x13,
            0x00, 0x01, 0x00, 0x80, 0x9f, 0x1e, 0xde, 0xa0, 0x61, 0xca, 0xdd,
            0x01, 0x01, 0x09, 0x00, 0xb2, 0x00, 0x00, 0x00, 0xa0, 0xa1, 0x00,
            0x00, 0x00, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x18,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x50, 0x18, 0x00, 0x00, 0x08,
            0x18, 0x00, 0x00, 0x20, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x04, 0x02, 0x01, 0x04, 0x10, 0x00, 0x00, 0x00, 0x65, 0x00, 0xe9,
            0x03, 0x00, 0x04, 0x10, 0x00, 0x00, 0x00, 0x0d, 0x00, 0x32, 0x10,
            0x00, 0x00, 0xa5, 0x84, 0x03, 0x00, 0x01, 0x08, 0x00, 0x00, 0xf9,
            0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0x61, 0xca,
            0xdd, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x00, 0x02, 0x00, 0x18,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x10, 0x00, 0x02,
            0x04, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08,
            0x00, 0x01, 0x50, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x20, 0x00, 0x03, 0x08, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x80, 0x00, 0x01, 0x20, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        let fadt_sdt = unsafe { SDT::new(buf.as_ptr()).unwrap() };
        let fadt = FADT::new(&fadt_sdt).unwrap();

        assert_eq!(fadt.firmware_ctrl, Some(0xde1e9f80));
        assert_eq!(fadt.dsdt, Some(0xddca61a0));
        assert_eq!(fadt.preferred_power_management_profile, Some(0x1));
        assert_eq!(fadt.sci_interrupt, Some(0x9));
        assert_eq!(fadt.smi_command_port, Some(0xb2));
        assert_eq!(fadt.acpi_enable, Some(0xa0));
        assert_eq!(fadt.acpi_disable, Some(0xa1));
        assert_eq!(fadt.s4_bios_req, Some(0x0));
        assert_eq!(fadt.pstate_control, Some(0x0));
        assert_eq!(fadt.pm1a_event_block, Some(0x1800));
        assert_eq!(fadt.pm1b_event_block, Some(0x0));
        assert_eq!(fadt.pm1a_control_block, Some(0x1804));
        assert_eq!(fadt.pm1b_control_block, Some(0x0));
        assert_eq!(fadt.pm2_control_block, Some(0x1850));
        assert_eq!(fadt.pm_timer_block, Some(0x1808));
        assert_eq!(fadt.gpe0_block, Some(0x1820));
        assert_eq!(fadt.gpe1_block, Some(0x0));
        assert_eq!(fadt.pm1_event_length, Some(0x4));
        assert_eq!(fadt.pm1_control_length, Some(0x2));
        assert_eq!(fadt.pm2_control_length, Some(0x1));
        assert_eq!(fadt.pm_timer_length, Some(0x4));
        assert_eq!(fadt.gpe0_length, Some(0x10));
        assert_eq!(fadt.gpe1_length, Some(0x0));
        assert_eq!(fadt.gpe1_base, Some(0x0));
        assert_eq!(fadt.cstate_control, Some(0x0));
        assert_eq!(fadt.worst_c2_latency, Some(0x65));
        assert_eq!(fadt.worst_c3_latency, Some(0x3e9));
        assert_eq!(fadt.flush_size, Some(0x400));
        assert_eq!(fadt.flush_stride, Some(0x10));
        assert_eq!(fadt.duty_offset, Some(0x0));
        assert_eq!(fadt.duty_width, Some(0x0));
        assert_eq!(fadt.day_alarm, Some(0xd));
        assert_eq!(fadt.month_alarm, Some(0x0));
        assert_eq!(fadt.century, Some(0x32));
        assert_eq!(fadt.boot_architecture_flags, Some(0x10));
        assert_eq!(fadt.flags, Some(0x384a5));
        let gas_bytes = [
            0x01, 0x08, 0x00, 0x00, 0xf9, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.reset_reg, Some(gas));
        assert_eq!(fadt.reset_value, Some(0x6));
        assert_eq!(fadt.arm_boot_architecture_flags, Some(0x0));
        assert_eq!(fadt.fadt_minor_version, Some(0x0));
        assert_eq!(fadt.x_firmware_control, Some(0x0));
        assert_eq!(fadt.x_dsdt, Some(0xddca61a0));
        let gas_bytes = [
            0x01, 0x20, 0x00, 0x02, 0x00, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_pm1a_event_block, Some(gas));
        let gas_bytes = [
            0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_pm1b_event_block, Some(gas));
        let gas_bytes = [
            0x01, 0x10, 0x00, 0x02, 0x04, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_pm1a_control_block, Some(gas));
        let gas_bytes = [
            0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_pm1b_control_block, Some(gas));
        let gas_bytes = [
            0x01, 0x08, 0x00, 0x01, 0x50, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_pm2_control_block, Some(gas));
        let gas_bytes = [
            0x01, 0x20, 0x00, 0x03, 0x08, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_pm_timerblock, Some(gas));
        let gas_bytes = [
            0x01, 0x80, 0x00, 0x01, 0x20, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_gpe0_block, Some(gas));
        let gas_bytes = [
            0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let gas = GenericAddressStructure::new(&gas_bytes).unwrap();
        assert_eq!(fadt.x_gpe1_block, Some(gas));
    }

    #[test]
    fn test_fadt_parse_v1() {
        // FADT taken from the default x86_64 machine in QEMU 5.1.0
        let buf = [
            0x46, 0x41, 0x43, 0x50, 0x74, 0x00, 0x00, 0x00, 0x01, 0xe7, 0x42,
            0x4f, 0x43, 0x48, 0x53, 0x20, 0x42, 0x58, 0x50, 0x43, 0x46, 0x41,
            0x43, 0x50, 0x01, 0x00, 0x00, 0x00, 0x42, 0x58, 0x50, 0x43, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0x3f, 0x40, 0x00, 0xfe, 0x3f,
            0x01, 0x00, 0x09, 0x00, 0xb2, 0x00, 0x00, 0x00, 0xf1, 0xf0, 0x00,
            0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x06,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
            0x06, 0x00, 0x00, 0xe0, 0xaf, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x04, 0x02, 0x00, 0x04, 0x04, 0x00, 0x00, 0x00, 0xff, 0x0f, 0xff,
            0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00,
            0x00, 0x00, 0xa5, 0x80, 0x00, 0x00,
        ];

        let fadt_sdt = unsafe { SDT::new(buf.as_ptr()).unwrap() };
        let fadt = FADT::new(&fadt_sdt).unwrap();

        assert_eq!(fadt.firmware_ctrl, Some(0x3ffe0000));
        assert_eq!(fadt.dsdt, Some(0x3ffe0040));
        assert_eq!(fadt.preferred_power_management_profile, Some(0x0));
        assert_eq!(fadt.sci_interrupt, Some(0x9));
        assert_eq!(fadt.smi_command_port, Some(0xb2));
        assert_eq!(fadt.acpi_enable, Some(0xf1));
        assert_eq!(fadt.acpi_disable, Some(0xf0));
        assert_eq!(fadt.s4_bios_req, Some(0x0));
        assert_eq!(fadt.pstate_control, Some(0x0));
        assert_eq!(fadt.pm1a_event_block, Some(0x600));
        assert_eq!(fadt.pm1b_event_block, Some(0x0));
        assert_eq!(fadt.pm1a_control_block, Some(0x604));
        assert_eq!(fadt.pm1b_control_block, Some(0x0));
        assert_eq!(fadt.pm2_control_block, Some(0x0));
        assert_eq!(fadt.pm_timer_block, Some(0x608));
        assert_eq!(fadt.gpe0_block, Some(0xafe0));
        assert_eq!(fadt.gpe1_block, Some(0x0));
        assert_eq!(fadt.pm1_event_length, Some(0x4));
        assert_eq!(fadt.pm1_control_length, Some(0x2));
        assert_eq!(fadt.pm2_control_length, Some(0x0));
        assert_eq!(fadt.pm_timer_length, Some(0x4));
        assert_eq!(fadt.gpe0_length, Some(0x4));
        assert_eq!(fadt.gpe1_length, Some(0x0));
        assert_eq!(fadt.gpe1_base, Some(0x0));
        assert_eq!(fadt.cstate_control, Some(0x0));
        assert_eq!(fadt.worst_c2_latency, Some(0xfff));
        assert_eq!(fadt.worst_c3_latency, Some(0xfff));
        assert_eq!(fadt.flush_size, Some(0x0));
        assert_eq!(fadt.flush_stride, Some(0x0));
        assert_eq!(fadt.duty_offset, Some(0x0));
        assert_eq!(fadt.duty_width, Some(0x0));
        assert_eq!(fadt.day_alarm, Some(0x0));
        assert_eq!(fadt.month_alarm, Some(0x0));
        assert_eq!(fadt.century, Some(0x32));
        assert_eq!(fadt.boot_architecture_flags, Some(0x0));
        assert_eq!(fadt.flags, Some(0x80a5));
        assert_eq!(fadt.reset_reg, None);
        assert_eq!(fadt.reset_value, None);
        assert_eq!(fadt.arm_boot_architecture_flags, None);
        assert_eq!(fadt.fadt_minor_version, None);
        assert_eq!(fadt.x_firmware_control, None);
        assert_eq!(fadt.x_dsdt, None);
        assert_eq!(fadt.x_pm1a_event_block, None);
        assert_eq!(fadt.x_pm1b_event_block, None);
        assert_eq!(fadt.x_pm1a_control_block, None);
        assert_eq!(fadt.x_pm1b_control_block, None);
        assert_eq!(fadt.x_pm2_control_block, None);
        assert_eq!(fadt.x_pm_timerblock, None);
        assert_eq!(fadt.x_gpe0_block, None);
        assert_eq!(fadt.x_gpe1_block, None);
    }
}
