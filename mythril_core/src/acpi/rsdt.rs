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

/// The Root System Descriptor Table.
pub struct RSDT<'a>(SDT<'a>);

impl<'a> RSDT<'a> {
    /// Create a new SDT.
    pub fn new(rsdt_addr: u32) -> Result<RSDT<'a>> {
        let sdt = unsafe { SDT::new(rsdt_addr as *const u8)? };
        Ok(RSDT(sdt))
    }

    /// Return the number of entries in the table.
    ///
    /// Note: This is not the same as the length value of the header.
    /// the the length value _includes_ the header.
    pub fn num_entries(&self) -> usize {
        self.0.len() / 4
    }

    /// Return an iterator for the SDT entries.
    pub fn entries(&self) -> RSDTIterator<'a> {
        RSDTIterator::new(self.0.table)
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
        write!(f, "{:?}", self.0)
    }
}

/// Iterator for the SDT entries found in the RSDT.
pub struct RSDTIterator<'a> {
    table: &'a [u8],
    offset: usize,
}

impl<'a> RSDTIterator<'a> {
    /// Create an iterator for the SDT entries in an RSDT.
    pub fn new<'b: 'a>(table: &'b [u8]) -> RSDTIterator<'a> {
        RSDTIterator { table, offset: 0 }
    }
}

impl<'a> Iterator for RSDTIterator<'a> {
    type Item = Result<SDT<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.offset + 4;
        if next <= self.table.len() {
            let item = unsafe {
                // TODO(dlrobertson): We currently only support the 32-bit RSDT.
                // When we support the 64-bit XSDT, the table may point to an
                // array of 64-bit pointers.
                let ptr =
                    NativeEndian::read_u32(&self.table[self.offset..next]);
                self.offset = next;
                SDT::new(ptr as *const u8)
            };
            Some(item)
        } else {
            None
        }
    }
}
