#![deny(missing_docs)]

//! # ACPI Support for the Mythril Hypervisor
//!
//! This module contains implementations for structures and functions
//! described in the ACPI specification. For all ACPI specification
//! references found in the code and documentation reffer to [ACPI 6.3].
//!
//! [ACPI 6.3]: https://uefi.org/sites/default/files/resources/ACPI_6_3_May16.pdf

use crate::error::{Error, Result};

/// Support for the Root System Descriptor Pointer (RSDP).
pub mod rsdp;
/// Support for the Root System Descriptor Table (RSDT).
pub mod rsdt;

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
