use super::rsdt::SDT;
use crate::error::{Error, Result};
use bitflags::bitflags;
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;
use derive_try_from_primitive::TryFromPrimitive;

/// See Table 5-43 in the ACPI spcification.
///
/// Note that these offsets are relative to the end of the
/// SDT (the end of the Creator Revision at offset 36).
mod offsets {
    use super::*;
    /// 32-bit physical address at which each processor can access its
    /// local APIC.
    pub const LOCAL_INT_CTRL_ADDR: Range<usize> = 0..4;
    /// Multiple APIC Flags.
    pub const FLAGS: Range<usize> = 4..8;
    /// Interrupt Controller Structures
    pub const INT_CTRL_STRUCTS: usize = 8;
}

/// Interrupt Controller Structure Type Values
///
/// See Table 5-45 in the APIC specification.
#[repr(u8)]
#[derive(Clone, Copy, Debug, TryFromPrimitive)]
pub enum IcsType {
    /// Processor Local APIC Structure Tag.
    ProcessorLocalApic = 0x00,
    /// I/O APIC Structure Tag.
    IoApic = 0x01,
    /// Interrupt Source Override Structure Tag.
    InterruptSourceOverride = 0x02,
    /// Non-Maskable Interrupt Source Structure Tag.
    NmiSource = 0x03,
    /// Local APIC NMI Structure Tag.
    LocalApicNmi = 0x04,
    /// Local APIC Address Override Structure Tag.
    LocalApicAddressOverride = 0x05,
    /// Platform Interrupt Source Structure Tag.
    PlatformInterruptSource = 0x08,
    /// Processor Local x2APIC Structure Tag.
    ProcessorLocalX2Apic = 0x09,
    /// Local x2APIC NMI Structure Tag.
    LocalX2ApicNmi = 0x0a,
}

impl IcsType {
    /// Expected length of buffer for ICS type.
    ///
    /// See the Structure definition tables found in
    /// `ACPI § 5.2.12` for details.
    pub fn expected_len(&self) -> usize {
        match *self {
            IcsType::ProcessorLocalApic => 8,
            IcsType::IoApic => 12,
            IcsType::InterruptSourceOverride => 10,
            IcsType::NmiSource => 8,
            IcsType::LocalApicNmi => 6,
            IcsType::LocalApicAddressOverride => 12,
            IcsType::PlatformInterruptSource => 16,
            IcsType::ProcessorLocalX2Apic => 16,
            IcsType::LocalX2ApicNmi => 12,
        }
    }

    /// Check the length of bytes available for the given
    /// ICS type.
    pub fn check_len(&self, length: usize) -> Result<()> {
        // The length includes the type and length bytes.
        if length == self.expected_len() - 2 {
            Ok(())
        } else {
            Err(Error::InvalidValue(format!(
                "Invalid length={} for type=0x{:x}",
                *self as u8, length
            )))
        }
    }
}

bitflags! {
    /// Multiple APIC Flags.
    ///
    /// See ACPI Table 5-44.
    pub struct MultipleApicFlags: u32 {
        /// Indicates that the system has a PC-AT compatible
        /// dual-8259 setup.
        const PCAT_COMPAT = 1;
    }
}

bitflags! {
    /// Local APIC Flags.
    ///
    /// See ACPI Table 5-47.
    pub struct LocalApicFlags: u32 {
        /// The processor is ready for use.
        const ENABLED = 1;
        /// If `ENABLED` bit is 0, the processor supports enabling this
        /// processor during OS runtime. If `ENABLED` is 1, this bit is
        /// reserved.
        const ONLINE_CAPABLE = 1 << 1;
    }
}

bitflags! {
    /// MPS INTI Flags.
    ///
    /// See ACPI Table 5-50
    pub struct MpsIntiFlags: u16 {
        /// Active High Polarity.
        const ACTIVE_HIGH = 0x0001;
        /// Active Low Polarity.
        const ACTIVE_LOW = 0x0003;
        /// Edge-Triggered Trigger Mode.
        const EDGE_TRIGGERED = 0x0004;
        /// Level-Triggered Trigger Mode.
        const LEVEL_TRIGGERED = 0x0000c;
    }
}

/// Interrupt Controller Structures.
#[derive(Debug)]
pub enum Ics {
    /// Processor Local APIC Structure.
    ///
    /// See `ACPI § 5.2.12.2`.
    LocalApic {
        /// Processor Object ID.
        apic_uid: u8,
        /// The processors local APIC ID.
        apic_id: u8,
        /// Local APIC Flags.
        flags: LocalApicFlags,
    },
    /// I/O APIC Structure.
    ///
    /// See `ACPI § 5.2.12.3`.
    IoApic {
        /// I/O APIC ID.
        ioapic_id: u8,
        /// 32-bit physical address to access this I/O APIC.
        ioapic_addr: *mut u8,
        /// Global System Interrupt number where this I/O APIC's interrupt
        /// input starts.
        gsi_base: u32,
    },
    /// Interrupt Source Override Structure.
    ///
    /// See `ACPI § 5.2.12.5`.
    InterruptSourceOverride {
        /// Bus-relative interrupt source.
        source: u8,
        /// Global System Interrupt that this bus-relative interrupt will
        /// signal.
        gsi: u32,
        /// MPS INI Flags.
        flags: MpsIntiFlags,
    },
    /// Non-Maskable Interrupt Source Structure.
    ///
    /// See `ACPI § 5.2.12.6`.
    NmiSource {
        /// MPS INI Flags.
        flags: MpsIntiFlags,
        /// Global System Interrupt that this NMI will signal.
        gsi: u32,
    },
    /// Local APIC NMI Structure.
    ///
    /// See `ACPI § 5.2.12.7`.
    LocalApicNmi {
        /// Processor Object ID.
        acpi_proc_uid: u8,
        /// MPS INI Flags.
        flags: MpsIntiFlags,
        /// Local APIC interrupt input LINTn to which NMI is connected.
        local_apic_lint: u8,
    },
    /// Processor Local x2APIC Structure.
    ///
    /// See `ACPI § 5.2.12.12`.
    LocalX2Apic {
        /// Processor local x2APIC ID.
        x2apic_id: u32,
        /// Local APIC Flags.
        flags: LocalApicFlags,
        /// Processor Object ID.
        apic_proc_uid: u32,
    },
}

impl Ics {
    /// Parse the given
    fn parse<'a>(ty: IcsType, bytes: &'a [u8]) -> Result<Ics> {
        ty.check_len(bytes.len())?;
        match ty {
            IcsType::ProcessorLocalApic => Ok(Ics::LocalApic {
                apic_uid: bytes[0],
                apic_id: bytes[1],
                flags: LocalApicFlags::from_bits_truncate(
                    NativeEndian::read_u32(&bytes[2..6]),
                ),
            }),
            IcsType::IoApic => {
                let ioapic_addr = NativeEndian::read_u32(&bytes[2..6]);
                Ok(Ics::IoApic {
                    ioapic_id: bytes[0],
                    ioapic_addr: ioapic_addr as *mut u8,
                    gsi_base: NativeEndian::read_u32(&bytes[6..10]),
                })
            }
            IcsType::InterruptSourceOverride => {
                Ok(Ics::InterruptSourceOverride {
                    source: bytes[1],
                    gsi: NativeEndian::read_u32(&bytes[2..6]),
                    flags: MpsIntiFlags::from_bits_truncate(
                        NativeEndian::read_u16(&bytes[6..8]),
                    ),
                })
            }
            IcsType::NmiSource => Ok(Ics::NmiSource {
                flags: MpsIntiFlags::from_bits_truncate(
                    NativeEndian::read_u16(&bytes[0..2]),
                ),
                gsi: NativeEndian::read_u32(&bytes[2..6]),
            }),
            IcsType::LocalApicNmi => Ok(Ics::LocalApicNmi {
                acpi_proc_uid: bytes[0],
                flags: MpsIntiFlags::from_bits_truncate(
                    NativeEndian::read_u16(&bytes[1..3]),
                ),
                local_apic_lint: bytes[3],
            }),
            IcsType::ProcessorLocalX2Apic => Ok(Ics::LocalX2Apic {
                x2apic_id: NativeEndian::read_u32(&bytes[2..6]),
                flags: LocalApicFlags::from_bits_truncate(
                    NativeEndian::read_u32(&bytes[6..10]),
                ),
                apic_proc_uid: NativeEndian::read_u32(&bytes[10..14]),
            }),
            _ => Err(Error::NotImplemented(format!(
                "type=0x{:x} length={} not implemented",
                ty as u8,
                bytes.len()
            ))),
        }
    }

    /// The controll structure type for the value.
    pub fn ics_type(&self) -> IcsType {
        match self {
            &Ics::LocalApic { .. } => IcsType::ProcessorLocalApic,
            &Ics::IoApic { .. } => IcsType::IoApic,
            &Ics::InterruptSourceOverride { .. } => {
                IcsType::InterruptSourceOverride
            }
            &Ics::NmiSource { .. } => IcsType::NmiSource,
            &Ics::LocalApicNmi { .. } => IcsType::LocalApicNmi,
            &Ics::LocalX2Apic { .. } => IcsType::ProcessorLocalX2Apic,
        }
    }
}

/// Multiple APIC Descriptor Table (MADT).
///
/// See `ACPI § 5.2.12`.
pub struct MADT<'a> {
    /// System Descriptor Table Header for this structure.
    sdt: &'a SDT<'a>,
    /// 32-bit physical Local Interrupt Controller Address.
    pub ica: *const u8,
    /// Multiple APIC Flags. See `ACPI § 5.2.12` Table 5-44
    /// for the flag values and their meaning.
    pub flags: MultipleApicFlags,
    /// A TLV buffer of MADT specific structures.
    ///
    /// From `ACPI § 5.2.12`:
    ///
    /// > The first byte of each structure declares the type of that
    /// > structure and the second byte declares the length of that
    /// > structure.
    ///
    /// The Interrupt Controller Structure buffer is the data immediately
    /// following the System Descriptor Table Header (`SDT`), and as a
    /// result should have the same lifetime as the `SDT`.
    ics: &'a [u8],
}

impl<'a> MADT<'a> {
    /// Create a new MADT given a SDT.
    pub fn new(sdt: &'a SDT<'a>) -> MADT<'a> {
        let ica =
            NativeEndian::read_u32(&sdt.table[offsets::LOCAL_INT_CTRL_ADDR]);
        let flags = MultipleApicFlags::from_bits_truncate(
            NativeEndian::read_u32(&sdt.table[offsets::FLAGS]),
        );
        let ics = &sdt.table[offsets::INT_CTRL_STRUCTS..];
        MADT {
            sdt,
            flags,
            ics,
            ica: ica as *const u8,
        }
    }

    /// Interrupt Controller Structures.
    pub fn structures<'c, 'd: 'c>(&'d self) -> IcsIterator<'c> {
        IcsIterator { bytes: self.ics }
    }
}

/// Iterator for the Interrupt Controller Structures found in the MADT.
pub struct IcsIterator<'a> {
    bytes: &'a [u8],
}

impl<'a> IcsIterator<'a> {
    /// Create a new Iterator for the Interrupt Controller Structures.
    pub fn new(bytes: &'a [u8]) -> IcsIterator<'a> {
        IcsIterator { bytes }
    }
}

impl<'a> Iterator for IcsIterator<'a> {
    type Item = Result<Ics>;

    fn next(&mut self) -> Option<Self::Item> {
        if 2 > self.bytes.len() {
            return None;
        }

        let ty = match IcsType::try_from(self.bytes[0]) {
            Some(ty) => ty,
            None => {
                return Some(Err(Error::InvalidValue(format!(
                    "Invalid ICS type: {}",
                    self.bytes[0]
                ))));
            }
        };
        let len = self.bytes[1] as usize;

        if len > self.bytes.len() {
            return Some(Err(Error::InvalidValue(format!(
                "Payload for type=0x{:x} and len={} to big for buffer len={}",
                ty as u8,
                len,
                self.bytes.len()
            ))));
        }

        let bytes = &self.bytes[2..len];

        self.bytes = &self.bytes[len..];

        Some(Ics::parse(ty, bytes))
    }
}

impl<'a> fmt::Debug for MADT<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.sdt)?;
        write!(f, " ICA={:p} flags=0x{:x}", self.ica, self.flags)
    }
}
