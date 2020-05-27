use super::rsdt::SDT;
use super::GenericAddressStructure;
use crate::error::{Error, Result};
use byteorder::{ByteOrder, NativeEndian};
use core::fmt;
use core::ops::Range;
use derive_try_from_primitive::TryFromPrimitive;

mod offsets {
    use super::*;
    pub const EVENT_TIMER_BLOCK_ID: Range<usize> = 0..4;
    pub const BASE_ADDRESS: Range<usize> = 4..16;
    pub const HPET_NUMBER: usize = 16;
    pub const MIN_CLOCK_TICK: Range<usize> = 17..19;
    pub const PAGE_PROTECTION: usize = 19;
}

/// Page Protection for HPET register access.
///
/// See Table 3 in the IA-PC HPET specification.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, TryFromPrimitive)]
pub enum PageProtection {
    /// No page protection
    NoProtection = 0x00,
    /// Register access is protected by a 4KB page
    Protected4KB = 0x01,
    /// Register access is protected by a 64KB page
    Protected64KB = 0x02,
}

/// High Precision Event Timer ACPI entry.
///
/// See `IA-PC HPET § 1.0a`.
pub struct HPET<'a> {
    /// System Descriptor Table Header for this structure.
    sdt: &'a SDT<'a>,
    /// The hardware revision ID.
    pub hardware_rev_id: u8,
    /// The number of comparators in the first timer block.
    pub comparator_count: u8,
    /// The size cap of the counter. If false, then only 32 bit mode is allowed.
    /// If true, then both 32 bit and 64 bit modes are supported.
    pub counter_cap: bool,
    /// If true, then the HPET is LegacyReplacement IRQ routing capable.
    pub legacy_replacement: bool,
    /// The PCI vendor ID of the first timer block.
    pub pci_vendor_id: u16,
    /// The address of the HPET register stored as an ACPI GAS.
    pub address: GenericAddressStructure,
    /// The HPET sequence number.
    pub hpet_number: u8,
    /// The minimum number of ticks that must be used by any counter programmed
    /// in periodic mode to avoid lost interrupts.
    pub minimum_tick: u16,
    /// The type of page protection for HPET register access.
    pub page_protection: PageProtection,
}

impl<'a> HPET<'a> {
    /// Create a new HPET given a SDT.
    pub fn new(sdt: &'a SDT<'a>) -> Result<HPET<'a>> {
        let event_timer_block_id =
            NativeEndian::read_u32(&sdt.table[offsets::EVENT_TIMER_BLOCK_ID]);
        let address =
            GenericAddressStructure::new(&sdt.table[offsets::BASE_ADDRESS])?;
        let hpet_number = sdt.table[offsets::HPET_NUMBER];
        let minimum_tick =
            NativeEndian::read_u16(&sdt.table[offsets::MIN_CLOCK_TICK]);

        let page_protection =
            PageProtection::try_from(sdt.table[offsets::PAGE_PROTECTION] & 0xF)
                .ok_or_else(|| {
                    Error::InvalidValue(format!(
                        "Invalid HPET Page Protection type: {}",
                        sdt.table[offsets::PAGE_PROTECTION] & 0xF
                    ))
                })?;

        let hardware_rev_id = (event_timer_block_id & 0xFF) as u8;
        let comparator_count = ((event_timer_block_id >> 8) & 0x1F) as u8;
        let counter_cap = ((event_timer_block_id >> 13) & 0x1) as u8;
        let legacy_replacement = ((event_timer_block_id >> 15) & 0x1) as u8;
        let pci_vendor_id = ((event_timer_block_id >> 16) & 0xFFFF) as u16;

        let counter_cap = counter_cap != 0;
        let legacy_replacement = legacy_replacement != 0;

        Ok(Self {
            sdt,
            hardware_rev_id,
            comparator_count,
            counter_cap,
            legacy_replacement,
            pci_vendor_id,
            address,
            hpet_number,
            minimum_tick,
            page_protection,
        })
    }
}

impl<'a> fmt::Debug for HPET<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.sdt)?;
        write!(f, " HPET address=0x{:x}", self.address.address)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::acpi::{AccessSize, AddressSpaceID};

    #[test]
    fn test_hpet_parse() {
        // sample HPET ACPI entry
        let buf = [
            0x48, 0x50, 0x45, 0x54, 0x38, 0x00, 0x00, 0x00, 0x01, 0xb6, 0x41,
            0x4c, 0x41, 0x53, 0x4b, 0x41, 0x41, 0x20, 0x4d, 0x20, 0x49, 0x00,
            0x00, 0x00, 0x09, 0x20, 0x07, 0x01, 0x41, 0x4d, 0x49, 0x2e, 0x05,
            0x00, 0x00, 0x00, 0x01, 0xa7, 0x86, 0x80, 0x00, 0x40, 0x00, 0x00,
            0x00, 0x00, 0xd0, 0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xee, 0x37,
            0x00,
        ];

        let hpet_sdt = unsafe { SDT::new(buf.as_ptr()).unwrap() };
        let hpet = HPET::new(&hpet_sdt).unwrap();

        assert_eq!(hpet.hardware_rev_id, 1);
        assert_eq!(hpet.comparator_count, 7);
        assert_eq!(hpet.counter_cap, true);
        assert_eq!(hpet.legacy_replacement, true);
        assert_eq!(hpet.pci_vendor_id, 0x8086);
        assert_eq!(hpet.address.address_space, AddressSpaceID::SystemMemory);
        assert_eq!(hpet.address.bit_width, 64);
        assert_eq!(hpet.address.bit_offset, 0);
        assert_eq!(hpet.address.access_size, AccessSize::Undefined);
        assert_eq!(hpet.address.address, 0xfed00000);
        assert_eq!(hpet.hpet_number, 0);
        assert_eq!(hpet.minimum_tick, 0x37ee);
        assert_eq!(hpet.page_protection, PageProtection::NoProtection);
    }
}
