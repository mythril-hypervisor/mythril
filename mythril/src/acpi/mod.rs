#![deny(missing_docs)]

//! # ACPI Support for the Mythril Hypervisor
//!
//! This module contains implementations for structures and functions
//! described in the ACPI specification. For all ACPI specification
//! references found in the code and documentation reffer to [ACPI 6.3].
//!
//! [ACPI 6.3]: https://uefi.org/sites/default/files/resources/ACPI_6_3_May16.pdf

use crate::error::{Error, Result};
use byteorder::{ByteOrder, NativeEndian};
use core::convert::TryFrom;
use num_enum::TryFromPrimitive;
use raw_cpuid::CpuId;

/// Support for the Fixed ACPI Descriptor Table (FADT).
pub mod fadt;
/// Support for the High Precision Event Timer (HPET)
pub mod hpet;
/// Support for the Multiple APIC Descriptor Table (MADT).
pub mod madt;
/// Support for the Root System Descriptor Pointer (RSDP).
pub mod rsdp;
/// Support for the Root System Descriptor Table (RSDT).
pub mod rsdt;

mod offsets {
    use core::ops::Range;

    pub const GAS_ADDRESS_SPACE: usize = 0;
    pub const GAS_BIT_WIDTH: usize = 1;
    pub const GAS_BIT_OFFSET: usize = 2;
    pub const GAS_ACCESS_SIZE: usize = 3;
    pub const GAS_ADDRESS: Range<usize> = 4..12;
}

/// Verify a one byte checksum for a given slice and length.
pub(self) fn verify_checksum(bytes: &[u8], cksum_idx: usize) -> Result<()> {
    // Sum up the bytes in the buffer.
    let result = bytes.iter().fold(0usize, |acc, val| acc + *val as usize);

    // The result of the sum should be zero. See the ACPI § 5.2.5.3
    // in Table 5-27.
    if (result & 0xff) == 0x00 {
        Ok(())
    } else {
        Err(Error::InvalidValue(format!(
            "Checksum mismatch checksum={:x} {:x} != 0x00",
            bytes[cksum_idx],
            result & 0xff,
        )))
    }
}

/// The size of a Generic Address Structure in bytes.
pub const GAS_SIZE: usize = 12;

/// Generic Address Structure (GAS) used by ACPI for position of registers.
///
/// See Table 5-25 in ACPI specification.
#[derive(Debug, PartialEq)]
pub struct GenericAddressStructure {
    /// The address space where the associated address exists.
    pub address_space: AddressSpaceID,
    /// The size in bits of the given register.
    pub bit_width: u8,
    /// The bit offset of the given register at the given address.
    pub bit_offset: u8,
    /// The size of the memory access for the given address.
    pub access_size: AccessSize,
    /// The 64-bit address of the register or data structure.
    pub address: u64,
}

/// Where a given address pointed to by a GAS resides.
///
/// See Table 5-25 of the ACPI specification.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, TryFromPrimitive)]
pub enum AddressSpaceID {
    /// The associated address exists in the System Memory space.
    SystemMemory = 0x00,
    /// The associated address exists in the System I/O space.
    SystemIO = 0x01,
    /// The associated address exists in the PCI Configuration space.
    PCIConfiguration = 0x02,
    /// The associated address exists in an Embedded Controller.
    EmbeddedController = 0x03,
    /// The associated address exists in the SMBus.
    SMBus = 0x04,
    /// The associated address exists in the SystemCMOS.
    SystemCMOS = 0x05,
    /// The associated address exists in a PCI Bar Target.
    PciBarTarget = 0x06,
    /// The associated address exists in an IPMI.
    IPMI = 0x07,
    /// The associated address exists in General Purpose I/O.
    GPIO = 0x08,
    /// The associated address exists in a Generic Serial Bus.
    GenericSerialBus = 0x09,
    /// The associated address exists in the Platform Communications Channel (PCC).
    PlatformCommunicationsChannel = 0x0A,
    /// The associated address exists in Functional Fixed Hardware.
    FunctionalFixedHardware = 0x7F,
}

/// Specifies access size of an address in a GAS.
///
/// See Table 5-25 in the ACPI specification.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive)]
pub enum AccessSize {
    /// Undefined (legacy reasons).
    Undefined = 0x00,
    /// Byte access.
    Byte = 0x01,
    /// Word access.
    Word = 0x02,
    /// DWord access.
    DWord = 0x03,
    /// QWord access.
    QWord = 0x04,
}

impl GenericAddressStructure {
    /// Create a new GAS from a slice of bytes.
    pub fn new(bytes: &[u8]) -> Result<GenericAddressStructure> {
        if bytes.len() != GAS_SIZE {
            return Err(Error::InvalidValue(format!(
                "Invalid number of bytes for GAS: {} != {}",
                bytes.len(),
                GAS_SIZE
            )));
        }

        let address_space =
            AddressSpaceID::try_from(bytes[offsets::GAS_ADDRESS_SPACE])?;

        let bit_width = bytes[offsets::GAS_BIT_WIDTH];
        let bit_offset = bytes[offsets::GAS_BIT_OFFSET];

        let access_size =
            AccessSize::try_from(bytes[offsets::GAS_ACCESS_SIZE])?;

        let address = NativeEndian::read_u64(&bytes[offsets::GAS_ADDRESS]);

        if address_space == AddressSpaceID::SystemMemory
            || address_space == AddressSpaceID::SystemIO
        {
            // call CPUID to determine if we need to verify the address. If the
            // call to CPUID fails, the check is not performed.
            let cpuid = CpuId::new();
            let is_64bit = cpuid
                .get_extended_function_info()
                .and_then(|x| Some(x.has_64bit_mode()))
                .ok_or_else(|| Error::NotSupported)?;

            // verify that the address is only 32 bits for 32-bit platforms.
            if !is_64bit && ((address >> 32) & 0xFFFFFFFF) != 0 {
                return Err(Error::InvalidValue(format!(
                    "Invalid address for a 32-bit system: {:x}",
                    address
                )));
            }
        }

        Ok(Self {
            address_space,
            bit_width,
            bit_offset,
            access_size,
            address,
        })
    }
}
