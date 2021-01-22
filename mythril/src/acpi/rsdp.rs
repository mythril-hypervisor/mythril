use super::rsdt::RSDT;
use super::rsdt::{RSDTBuilder, SDTBuilder};
use super::seabios::{AllocZone, TableLoaderBuilder, TableLoaderCommand};
use super::{calc_checksum, verify_checksum};
use crate::error::{Error, Result};
use crate::virtdev::qemu_fw_cfg::QemuFwCfgBuilder;
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;
use core::slice;

use arrayvec::{Array, ArrayVec};
use managed::ManagedMap;

/// Extented BIOS Data Area Start Address
const EXTENDED_BIOS_DATA_START: usize = 0x000040e;
/// Extented BIOS Data Area End Address
const EXTENDED_BIOS_DATA_SIZE: usize = 0x800;
/// Main BIOS Data Area Start Address
const MAIN_BIOS_DATA_START: usize = 0x000e0000;
/// Main BIOS Data Area End Address
const MAIN_BIOS_DATA_SIZE: usize = 0x20000;
/// Well Known RSDP Signature
pub const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";

/// Offsets from `ACPI § 5.2.5.3`
pub mod offsets {
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
    /// 32-bit length of the entire table.
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

/// The revision values for the RSDP.
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum RSDPRevision {
    /// ACPI 1.0 Root System Descriptor Pointer Revision
    V1 = 0x00,
    /// ACPI 2.0 Root System Descriptor Pointer Revision
    V2 = 0x02,
}

/// Root System Descriptor Pointer (RSDP).
///
/// See `ACPI § 5.2.7`
#[derive(Clone)]
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
        /// 64-bit physical address of the XSDT.
        xsdt_addr: u64,
        //  TODO(ntegan) multiboot2 doesn't give us a length for rsdpv2tag,
        //      so we can't calculate the rsdp checksum
    },
}

impl fmt::Debug for RSDP {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &RSDP::V1 { rsdt_addr, .. } => {
                write!(f, "RSDP: revision=1.0 rsdt=0x{:x}", rsdt_addr)
            }
            &RSDP::V2 { xsdt_addr, .. } => {
                write!(f, "RSDP: revision=2.0 xsdt_addr=0x{:x}", xsdt_addr)
            }
        }
    }
}

impl RSDP {
    /// Make an RSDP from a given collection of bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
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
                xsdt_addr: NativeEndian::read_u64(&bytes[offsets::XSDT_ADDR]),
            },
            _ => {
                error!("Invalid RSDP revision: {}",
                       bytes[offsets::REVISION]);
                return Err(Error::InvalidValue)
                    .into();
            }
        };

        // Now that we know this is a version we support, validate the checksum.
        Self::verify_rsdp_checksum(bytes)?;
        Ok(rsdp)
    }

    /// Find the RSDP in the Extended BIOS Data Area or the
    /// Main BIOS area.
    ///
    /// This is described in `ACPI § 5.2.5.1`
    pub fn find() -> Result<Self> {
        let bytes = unsafe {
            // Try to find the RSDP in the Extended BIOS Data Area (EBDA).
            let range = slice::from_raw_parts(
                MAIN_BIOS_DATA_START as *const u8,
                MAIN_BIOS_DATA_SIZE,
            );
            Self::search_range(range).or_else(|_| {
                let range = slice::from_raw_parts(
                    EXTENDED_BIOS_DATA_START as *const u8,
                    EXTENDED_BIOS_DATA_SIZE,
                );
                // If we didn't find the RSDP in the EBDA, try to find it in
                // the Main BIOS Data.
                Self::search_range(range)
            })?
        };

        Self::from_bytes(bytes)
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
                    _ => {
                        error!("Invalid RSDP revision: {} at {:p}",
                               candidate[offsets::REVISION],
                               candidate.as_ptr());
                        Err(Error::InvalidValue)
                    }
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
            _ => {
                error!("Invalid RSDP revision: {}",
                       bytes[offsets::REVISION]);
                Err(Error::InvalidValue)
            }
        }
    }

    /// Return the RSDT pointed to by this structure.
    pub fn rsdt(&self) -> Result<RSDT> {
        match self {
            &RSDP::V1 { rsdt_addr, .. } => RSDT::new_rsdt(rsdt_addr as usize),
            &RSDP::V2 { xsdt_addr, .. } => RSDT::new_xsdt(xsdt_addr as usize),
        }
    }
}

/// Builder structure for the RSDP
pub struct RSDPBuilder<'a, T: Array<Item=u8>> {
    builder: RSDTBuilder<'a, T>,
}

impl<'a, T: Array<Item=u8>> RSDPBuilder<'a, T> {
    /// Create a new RSDP Builder.
    pub fn new(
        map: ManagedMap<'a, [u8; 4], (ArrayVec<T>, usize)>,
    ) -> RSDPBuilder<T> {
        RSDPBuilder {
            builder: RSDTBuilder::new(map),
        }
    }

    /// Add the given System Descriptor Table to the RSDT.
    pub fn add_sdt(&mut self, builder: impl SDTBuilder) -> Result<()> {
        self.builder.add_sdt(builder)
    }

    /// Create the encoded ACPI table.
    pub fn build(&self, fw_cfg_builder: &mut QemuFwCfgBuilder) -> Result<()> {
        let mut buffer = [0x00; offsets::RESERVED.end];

        // Populate the signature and revision.
        buffer[offsets::SIGNATURE].copy_from_slice(RSDP_SIGNATURE);
        buffer[offsets::REVISION] = RSDPRevision::V2 as u8;

        NativeEndian::write_u32(
            &mut buffer[offsets::LENGTH],
            offsets::RESERVED.end as u32,
        );

        // TODO(dlrobertson): Should we support the ACPI 1.0 RSDT in addition
        // to the XSDT?
        NativeEndian::write_u64(
            &mut buffer[offsets::XSDT_ADDR],
            0x00000000_00000000,
        );

        // Using the RSDT address from an XSDT is invalid according to the spec
        NativeEndian::write_u32(&mut buffer[offsets::RSDT_ADDR], 0x00000000);

        // We create a valid 36 byte RSDP regardless of the revision
        buffer[offsets::CHECKSUM] =
            calc_checksum(&buffer[..offsets::RSDT_ADDR.end]);
        buffer[offsets::EXT_CHECKSUM] =
            calc_checksum(&buffer[..offsets::RESERVED.end]);

        fw_cfg_builder.add_file("etc/mythril/rsdp", &buffer)?;

        // This should be enough for 16 table loader commands.
        let mut table_loader = TableLoaderBuilder::<[_; 2048]>::new()?;

        table_loader.add_command(TableLoaderCommand::Allocate {
            file: "etc/mythril/rsdp",
            align: 0x10,
            zone: AllocZone::Fseg,
        })?;

        self.builder.build(fw_cfg_builder, &mut table_loader)?;

        // No nedd to update the ACPI 1.0 checksum, but we do need to update
        // the ACPI 2.0 checksup after populating the XSDT address.
        table_loader.add_command(TableLoaderCommand::AddChecksum {
            file: "etc/mythril/rsdp",
            offset: offsets::EXT_CHECKSUM as u32,
            start: offsets::XSDT_ADDR.start as u32,
            length: 8,
        })?;

        table_loader.load(fw_cfg_builder)
    }
}
