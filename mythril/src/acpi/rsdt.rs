use super::verify_checksum;
use crate::error::{Error, Result};
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;
use core::slice;
use core::str;

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
