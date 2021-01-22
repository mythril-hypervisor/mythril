use super::rsdp::offsets as rsdp_offsets;
use super::seabios::{AllocZone, TableLoaderBuilder, TableLoaderCommand};
use super::{calc_checksum, verify_checksum};
use crate::error::{Error, Result};
use crate::virtdev::qemu_fw_cfg::QemuFwCfgBuilder;
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;
use core::slice;
use core::str;

use arrayvec::{Array, ArrayVec};
use managed::ManagedMap;

/// Offsets from `ACPI § 5.2.6`
mod offsets {
    use super::*;
    /// Well known bytes, "RST PTR ".
    pub const SIGNATURE: Range<usize> = 0..4;
    /// Length of the table.
    pub const LENGTH: Range<usize> = 4..8;
    /// The revision of the structure corresponding to the signature.
    pub const REVISION: usize = 8;
    /// The checksum of the entire table.
    pub const CHECKSUM: usize = 9;
    /// Revision of utility that created the structure
    pub const CREATOR_REVISION: Range<usize> = 32..36;
}

/// A System Descriptor Table.
///
/// See Table 5-28 of `ACPI § 5.2.6` for a full description of the fields
/// found in a System Descriptor Table (SDT) header.
///
/// The structure includes a pointer to the table which immediately follows
/// the header. The contents of the table iare determined by the value of
/// the signature.  See Table 5-29 of `ACPI § 5.2.6` for the full list of
/// possible values for the `signature`.
///
/// Note that the SDT header should be the same even for the 32-bit RSDT
/// and the 64-bit XSDT. It is up to the caller to handle the table based
/// on the value of the `signature`.
pub struct SDT<'a> {
    /// Signature identifying the data contained in the table. See
    /// Table 5-29 in the ACPI spec for the available signatures.
    pub signature: [u8; 4],
    /// Length of the table and header.
    length: u32,
    /// Version of the contained table.
    pub revision: u8,
    /// The raw pointer to the structure.
    pub table: &'a [u8],
}

impl<'a> fmt::Debug for SDT<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}: rev={} length={} table={:p}",
            str::from_utf8(&self.signature).unwrap(),
            self.revision,
            self.length,
            self.table.as_ptr()
        )
    }
}

impl<'a> SDT<'a> {
    /// Create a new 32-bit SDT.
    /// Create a SDT header from an address.
    ///
    /// The function makes no assumption about the address size the table
    /// may point to. The caller is expected to know if this SDT originated
    /// from an XSDT (64-bit) or RSDT (32-bit).
    pub unsafe fn new(sdt_addr: *const u8) -> Result<SDT<'a>> {
        let mut signature = [0u8; 4];

        let header = slice::from_raw_parts(
            sdt_addr as *const u8,
            offsets::CREATOR_REVISION.end,
        );

        signature.copy_from_slice(&header[offsets::SIGNATURE]);

        let length = NativeEndian::read_u32(&header[offsets::LENGTH]);

        let bytes = slice::from_raw_parts(header.as_ptr(), length as usize);

        verify_checksum(bytes, offsets::CHECKSUM)?;

        Ok(SDT {
            signature,
            length,
            revision: header[offsets::REVISION],
            table: &bytes[offsets::CREATOR_REVISION.end..],
        })
    }

    /// The length of the data following the table.
    pub fn len(&self) -> usize {
        self.length as usize - offsets::CREATOR_REVISION.end
    }

    /// The data following the table as a slice of bytes.
    pub fn data(&self) -> &[u8] {
        self.table
    }
}

/// The Root System Description Table.
/// Two variants to support 32 bit RSDT and 64 bit XSDT.
pub enum RSDT<'a> {
    /// Root System Description Table.
    RSDT(SDT<'a>),
    /// Extended System Description Table.
    XSDT(SDT<'a>),
}

impl<'a> RSDT<'a> {
    /// Create a new RSDT.
    pub fn new_rsdt(rsdt_addr: usize) -> Result<RSDT<'a>> {
        let sdt = unsafe { SDT::new(rsdt_addr as *const u8)? };
        Ok(RSDT::RSDT(sdt))
    }

    /// Create a new XSDT.
    pub fn new_xsdt(xsdt_addr: usize) -> Result<RSDT<'a>> {
        let sdt = unsafe { SDT::new(xsdt_addr as *const u8)? };
        Ok(RSDT::XSDT(sdt))
    }

    /// Return the number of entries in the table.
    ///
    /// Note: This is not the same as the length value of the header.
    /// the the length value _includes_ the header.
    pub fn num_entries(&self) -> usize {
        match self {
            RSDT::XSDT(sdt) => sdt.len() / 8,
            RSDT::RSDT(sdt) => sdt.len() / 4,
        }
    }

    /// Return an iterator for the SDT entries.
    pub fn entries(&self) -> RSDTIterator<'a> {
        match self {
            RSDT::RSDT(sdt) => RSDTIterator::new_rsdt(sdt.table),
            RSDT::XSDT(sdt) => RSDTIterator::new_xsdt(sdt.table),
        }
    }

    /// Returns the first matching SDT for a given signature.
    pub fn find_entry(&self, signature: &[u8]) -> Result<SDT<'a>> {
        self.entries()
            .find(|entry| match entry {
                Ok(entry) if &entry.signature == signature => true,
                _ => false,
            })
            .unwrap_or(Err(Error::NotFound))
    }
}

impl<'a> fmt::Debug for RSDT<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let my_sdt = match self {
            RSDT::RSDT(the_sdt) => the_sdt,
            RSDT::XSDT(the_sdt) => the_sdt,
        };
        write!(f, "{:?}", my_sdt)
    }
}

/// Iterator for the SDT entries found in the RSDT/XSDT.
pub enum RSDTIterator<'a> {
    /// Iterates through 32 bit RSDT.
    RSDTIterator {
        /// Table of 32 bit SDT pointers.
        table: &'a [u8],
        /// Current offset to keep track of iteration.
        offset: usize,
    },
    /// Iterates through 64 bit XSDT.
    XSDTIterator {
        /// Table of 64 bit SDT pointers.
        table: &'a [u8],
        /// Current offset to keep track of iteration.
        offset: usize,
    },
}

impl<'a> RSDTIterator<'a> {
    /// Create an iterator for the SDT entries in an RSDT.
    pub fn new_rsdt<'b: 'a>(table: &'b [u8]) -> RSDTIterator<'a> {
        RSDTIterator::RSDTIterator { table, offset: 0 }
    }

    /// Create an iterator for the SDT entries in an XSDT.
    pub fn new_xsdt<'b: 'a>(table: &'b [u8]) -> RSDTIterator<'a> {
        RSDTIterator::XSDTIterator { table, offset: 0 }
    }
}

impl<'a> Iterator for RSDTIterator<'a> {
    type Item = Result<SDT<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RSDTIterator::RSDTIterator { table, offset } => {
                let next = *offset + 4usize;
                if next <= table.len() {
                    let item = unsafe {
                        let ptr = NativeEndian::read_u32(&table[*offset..next])
                            as usize;
                        *offset = next;
                        SDT::new(ptr as *const u8)
                    };
                    Some(item)
                } else {
                    None
                }
            }
            RSDTIterator::XSDTIterator { table, offset } => {
                let next = *offset + 8usize;
                if next <= table.len() {
                    let item = unsafe {
                        let ptr = NativeEndian::read_u64(&table[*offset..next])
                            as usize;
                        *offset = next;
                        SDT::new(ptr as *const u8)
                    };
                    Some(item)
                } else {
                    None
                }
            }
        }
    }
}

fn write_sdt_header(
    signature: &[u8],
    revision: u8,
    creator_revision: u32,
    sdt_len: usize,
    buffer: &mut [u8],
) -> Result<()> {
    // The SDT length value is the value of the entire SDT including
    // the header.
    if buffer.len() < sdt_len {
        error!("Buffer length should be at least `{}` but was `{}`",
               sdt_len,
               buffer.len());
        return Err(Error::InvalidValue);
    }
    // Fill in the SDT header with the implementations values
    buffer[offsets::SIGNATURE].copy_from_slice(signature);
    NativeEndian::write_u32(&mut buffer[offsets::LENGTH], sdt_len as u32);
    buffer[offsets::REVISION] = revision;
    NativeEndian::write_u32(
        &mut buffer[offsets::CREATOR_REVISION],
        creator_revision,
    );

    // After the SDT header and body have been populated calculate the
    // table checksum.
    buffer[offsets::CHECKSUM] = calc_checksum(&buffer);
    Ok(())
}

/// Builder trait for System Descriptor Tables
pub trait SDTBuilder {
    /// The identifying signature for this table type.
    const SIGNATURE: [u8; 4];

    /// The revision of the table
    fn revision(&self) -> u8 {
        0
    }

    /// The revision of the table
    fn creator_revision(&self) -> u32 {
        0
    }

    // TODO(dlrobertson): Can we rely on encoding the SDT table
    // without access to the guest address space? Do we need to
    // know the address of anything we place in the guest?

    /// Attempt to encode the SDT table
    fn encode_table<T: Array<Item = u8>>(
        &mut self,
        buffer: &mut ArrayVec<T>,
    ) -> Result<()>;

    /// Encode the entire SDT including the header and body based on the
    /// implemented methods.
    fn encode_sdt<T: Array<Item = u8>>(
        &mut self,
        buffer: &mut ArrayVec<T>,
    ) -> Result<usize> {
        let header = [0x00; offsets::CREATOR_REVISION.end];
        buffer.try_extend_from_slice(&header[..])?;
        self.encode_table(buffer)?;
        write_sdt_header(
            &Self::SIGNATURE,
            self.revision(),
            self.creator_revision(),
            buffer.len(),
            buffer,
        )?;
        Ok(buffer.len())
    }
}

/// Builder structure for the RSDT
pub(super) struct RSDTBuilder<'a, T: Array<Item = u8>> {
    map: ManagedMap<'a, [u8; 4], (ArrayVec<T>, usize)>,
}

impl<'a, T: Array<Item = u8>> RSDTBuilder<'a, T> {
    /// Create a new XSDT Builder.
    pub(super) fn new(
        map: ManagedMap<'a, [u8; 4], (ArrayVec<T>, usize)>,
    ) -> RSDTBuilder<T> {
        RSDTBuilder { map }
    }

    /// Add the given System Descriptor Table to the RSDT.
    pub(super) fn add_sdt<U: SDTBuilder>(
        &mut self,
        mut builder: U,
    ) -> Result<()> {
        let mut buffer = ArrayVec::<T>::new();
        let size = builder.encode_sdt(&mut buffer)?;
        if self.map.get(&U::SIGNATURE) == None {
            if self.map.insert(U::SIGNATURE, (buffer, size)).is_err() {
                Err(Error::Exhausted)
            } else {
                Ok(())
            }
        } else {
            error!("The key `{}` already exists",
                   str::from_utf8(&U::SIGNATURE).unwrap());
            Err(Error::InvalidValue)
        }
    }

    /// Create the encoded ACPI table.
    pub fn build<U: Array<Item = u8>>(
        &self,
        fw_cfg_builder: &mut QemuFwCfgBuilder,
        table_loader: &mut TableLoaderBuilder<U>,
    ) -> Result<()> {
        let mut xsdt = [0x00u8; 4096];
        let xsdt_table_length = self.map.iter().count() * 8;
        let xsdt_length = xsdt_table_length + offsets::CREATOR_REVISION.end;
        write_sdt_header(b"XSDT", 0, 0, xsdt_length, &mut xsdt[..])?;

        fw_cfg_builder.add_file("etc/mythril/xsdt", &xsdt[..xsdt_length])?;

        table_loader.add_command(TableLoaderCommand::Allocate {
            file: "etc/mythril/xsdt",
            align: 8,
            zone: AllocZone::Fseg,
        })?;

        table_loader.add_command(TableLoaderCommand::AddPointer {
            src: "etc/mythril/xsdt",
            dst: "etc/mythril/rsdp",
            offset: rsdp_offsets::XSDT_ADDR.start as u32,
            size: 8,
        })?;

        for (i, (name, (sdt, size))) in self.map.iter().enumerate() {
            const LEN_OF_ETC_MYTHRIL :usize = 12;
            const LEN_OF_NAME: usize = 4;
            let mut table_name_bytes = [0u8;LEN_OF_ETC_MYTHRIL + LEN_OF_NAME];
            table_name_bytes[0..LEN_OF_ETC_MYTHRIL].copy_from_slice("etc/mythril/".as_bytes());
            table_name_bytes[LEN_OF_ETC_MYTHRIL..].copy_from_slice(name);
            let table_name = str::from_utf8(&table_name_bytes)?;

            table_loader.add_command(TableLoaderCommand::Allocate {
                file: table_name,
                align: 8,
                zone: AllocZone::Fseg,
            })?;

            table_loader.add_command(TableLoaderCommand::AddPointer {
                src: table_name,
                dst: "etc/mythril/xsdt",
                offset: ((i * 8) + offsets::CREATOR_REVISION.end) as u32,
                size: 8,
            })?;

            fw_cfg_builder.add_file(table_name, &sdt[..*size])?;
        }

        // Update the XSDT checksum after populating the pointer table.
        table_loader.add_command(TableLoaderCommand::AddChecksum {
            file: "etc/mythril/xsdt",
            offset: offsets::CHECKSUM as u32,
            start: offsets::CREATOR_REVISION.end as u32,
            length: xsdt_table_length as u32,
        })?;

        Ok(())
    }
}
