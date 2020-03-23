use super::rsdt::RSDT;
use super::verify_checksum;
use crate::error::{Error, Result};
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;
use core::slice;

/// Extented BIOS Data Area Start Address
const EXTENDED_BIOS_DATA_START: usize = 0x000040e;
/// Extented BIOS Data Area End Address
const EXTENDED_BIOS_DATA_SIZE: usize = 0x800;
/// Main BIOS Data Area Start Address
const MAIN_BIOS_DATA_START: usize = 0x000e0000;
/// Main BIOS Data Area End Address
const MAIN_BIOS_DATA_SIZE: usize = 0x20000;
/// Well Known RSDP Signature
const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";

/// Offsets from `ACPI § 5.2.5.3`
mod offsets {
    use super::*;
    /// Well known bytes, "RST PTR ".
    pub const SIGNATURE: Range<usize> = 0..8;
    /// Checksum of the fields defined by ACPI 1.0.
    pub const CHECKSUM: usize = 8;
    /// OEM-supplied ID string.
    pub const OEMID: Range<usize> = 9..15;
    /// Revision of the structure. Zero for ACPI 1.0 and two for ACPI 2.0.
    pub const REVISION: usize = 15;
    /// 32-bit physical address of the RSDT.
    pub const RSDT_ADDR: Range<usize> = 16..20;
    /// Length of the XSDT table in bytes (ACPI 2.0 only).
    pub const LENGTH: Range<usize> = 20..24;
    /// 64-bit physical address of the XSDT (ACPI 2.0 only).
    pub const XSDT_ADDR: Range<usize> = 24..32;
    /// Checksum of entire structure (ACPI 2.0 only).
    pub const EXT_CHECKSUM: usize = 32;
    /// Size of the RSDP, including the reservied field.
    pub const RESERVED: Range<usize> = 33..36;
}

/// Structure size of the RSDP for revision one.
const RSDP_V1_SIZE: usize = offsets::RSDT_ADDR.end;
/// Structure size of the RSDP for revision two.
const RSDP_V2_SIZE: usize = offsets::RESERVED.end;

/// Root System Descriptor Pointer (RSDP).
///
/// See `ACPI § 5.2.7`
pub enum RSDP {
    /// RSDP structure variant for version 1 (`revision == 0`).
    V1 {
        /// OEM Supplied string.
        oemid: [u8; 6],
        /// 32-bit physical address of the RSDT.
        rsdt_addr: u32,
    },
    /// RSDP structure variant for version 2 (`revision == 2`).
    V2 {
        /// OEM Supplied string.
        oemid: [u8; 6],
        /// 32-bit physical address of the RSDT.
        rsdt_addr: u32,
        /// Length of the XSDT table in bytes.
        length: u32,
        /// 64-bit physical address of the XSDT.
        xsdt_addr: u64,
    },
}

impl fmt::Debug for RSDP {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &RSDP::V1 { rsdt_addr, .. } => {
                write!(f, "RSDP: revision=1.0 rsdt=0x{:x}", rsdt_addr)
            }
            &RSDP::V2 {
                rsdt_addr,
                xsdt_addr,
                ..
            } => write!(
                f,
                "RSDP: revision=2.0 rsdt=0x{:x} xsdt_addr=0x{:x}",
                rsdt_addr, xsdt_addr
            ),
        }
    }
}

impl RSDP {
    /// Find the RSDP in the Extended BIOS Data Area or the
    /// Main BIOS area.
    ///
    /// This is described in `ACPI § 5.2.5.1`
    pub fn find() -> Result<RSDP> {
        let bytes = unsafe {
            // Try to find the RSDP in the Extended BIOS Data Area (EBDA).
            let range = slice::from_raw_parts(
                EXTENDED_BIOS_DATA_START as *const u8,
                EXTENDED_BIOS_DATA_SIZE,
            );
            Self::search_range(range).or_else(|_| {
                let range = slice::from_raw_parts(
                    MAIN_BIOS_DATA_START as *const u8,
                    MAIN_BIOS_DATA_SIZE,
                );
                // If we didn't find the RSDP in the EBDA, try to find it in
                // the Main BIOS Data.
                Self::search_range(range)
            })?
        };
        info!("RSDP: {:p}", bytes.as_ptr());

        // Extract the OEMID, revision, and RSDT address from the address
        // found regardless of the ACPI version.
        let mut oemid = [0u8; offsets::OEMID.end - offsets::OEMID.start];
        oemid.copy_from_slice(&bytes[offsets::OEMID]);

        let rsdt_addr = NativeEndian::read_u32(&bytes[offsets::RSDT_ADDR]);

        let rsdp = match bytes[offsets::REVISION] {
            // If this is ACPI 1.0, there is no XSDT.
            0 => RSDP::V1 { oemid, rsdt_addr },
            // This is ACPI 2.0. Extract the addrss and length of the XSDT.
            2 => RSDP::V2 {
                oemid,
                rsdt_addr,
                length: NativeEndian::read_u32(&bytes[offsets::LENGTH]),
                xsdt_addr: NativeEndian::read_u64(&bytes[offsets::XSDT_ADDR]),
            },
            _ => {
                return Err(Error::InvalidValue(format!(
                    "Invalid RSDP revision: {}",
                    bytes[offsets::REVISION]
                )))
                .into();
            }
        };

        // Now that we know this is a version we support, validate the checksum.
        Self::verify_rsdp_checksum(bytes)?;
        Ok(rsdp)
    }

    /// Search a given address range for the RSDP.
    fn search_range(range: &[u8]) -> Result<&[u8]> {
        let end = range.len() - RSDP_V1_SIZE;

        // The RSDP is a two byte real mode segment pointer
        //
        // TODO(dlrobertson): are we sure it is two byte
        // aligned?
        for i in (0..end).step_by(2) {
            let rsdp_v1_end = i + RSDP_V1_SIZE;
            let rsdp_v2_end = i + RSDP_V2_SIZE;

            // Initially set the candidate slice size to that of the
            // smaller revision one structure.
            let candidate = &range[i..rsdp_v1_end];

            // If the RSDP structure is a revision 2 structure, add the
            // extended info to the slice.
            if &candidate[offsets::SIGNATURE] == RSDP_SIGNATURE {
                return match candidate[offsets::REVISION] {
                    0 => Ok(candidate),
                    2 if rsdp_v2_end < range.len() => {
                        Ok(&range[i..rsdp_v2_end])
                    }
                    _ => Err(Error::InvalidValue(format!(
                        "Invalid RSDP revision: {} at {:p}",
                        candidate[offsets::REVISION],
                        candidate.as_ptr()
                    ))),
                };
            }
        }

        Err(Error::NotFound)
    }

    /// Checksum validation for the RSDP.
    fn verify_rsdp_checksum(bytes: &[u8]) -> Result<()> {
        // Verify the RSDT checksum regardless of the ACPI version.
        verify_checksum(&bytes[..offsets::RSDT_ADDR.end], offsets::CHECKSUM)?;

        // We need to also validate the checksum of the extended data
        // for ACPI 2.0.
        match bytes[offsets::REVISION] {
            0 => Ok(()),
            2 => verify_checksum(
                &bytes[..offsets::RESERVED.end],
                offsets::EXT_CHECKSUM,
            ),
            _ => Err(Error::InvalidValue(format!(
                "Invalid RSDP revision: {}",
                bytes[offsets::REVISION]
            ))),
        }
    }

    /// Return the RSDT pointed to by this structure.
    pub fn rsdt(&self) -> Result<RSDT> {
        match self {
            &RSDP::V1 { rsdt_addr, .. } => RSDT::new(rsdt_addr),
            &RSDP::V2 { .. } => Err(Error::NotSupported),
        }
    }
}
