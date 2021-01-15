#![deny(missing_docs)]

//! Support for the I/O APIC.
//!
//! Module includes structures and implementations for the I/O Apic
//! and I/O Redirection Table Entries.
//!
//! # IoApic Structure
//!
//! The I/O APIC structure from this module exposes no static `new`
//! function for creating an instance. The structure should be created
//! by converting a previously obtained I/O APIC Interrupt Controller
//! Structure entry in the Multiple APIC Descriptor Table.

use crate::acpi::madt::{Ics, MADT};
use crate::error::{Error, Result};
use crate::lock::ro_after_init::RoAfterInit;
use core::convert::TryFrom;
use core::fmt;
use core::ops::Range;
use core::ptr;

use arrayvec::ArrayVec;
use spin::Mutex;

const IOREDTBL_KNOWN_BITS_MASK: u64 = 0xff000000_0001ffff;
const IOREDTBL_RW_MASK: u64 = 0xff000000_0001afff;
const IOAPIC_VERSION: u8 = 0x11;
const IOWIN_OFFSET: isize = 0x10;

/// I/O APIC Registers
mod reg {
    /// IOAPIC ID Register
    pub const IOAPICID: u8 = 0x00;
    /// IOAPIC Version Register
    pub const IOAPICVER: u8 = 0x01;
    /// IOAPIC Arbitration ID
    pub const IOAPICARB: u8 = 0x02;
    /// Redirection Table Offset
    pub const IOREDTBL_OFFSET: u8 = 0x10;
}

const MAX_IOAPIC_COUNT: usize = 16;

static IOAPICS: RoAfterInit<ArrayVec<[IoApic; MAX_IOAPIC_COUNT]>> =
    RoAfterInit::uninitialized();

// Get the IoApic and redirection table entry index corresponding to a given GSI.
// Returns None if there is no such IoApic
// TODO(alschwalm): Support InterruptSourceOverride
fn ioapic_for_gsi(gsi: u32) -> Option<(&'static IoApic, u8)> {
    for ioapic in IOAPICS.iter() {
        if ioapic.get_ivec_range().contains(&gsi) {
            return Some((ioapic, (gsi - ioapic.gsi_base) as u8));
        }
    }
    None
}

/// Map a given GSI to an interrupt vector on the core with the associated apic_id
pub fn map_gsi_vector(gsi: u32, vector: u8, apic_id: u8) -> Result<()> {
    match ioapic_for_gsi(gsi) {
        Some((ioapic, entry)) => {
            debug!(
                "Mapping gsi=0x{:x} to vector 0x{:x} on apic id = 0x{:x}",
                gsi, vector, apic_id
            );
            ioapic.write_ioredtbl(
                entry,
                IoRedTblEntry::new(
                    vector,
                    DeliveryMode::Fixed,
                    DestinationMode::Physical,
                    PinPolarity::ActiveHigh,
                    TriggerMode::Edge,
                    false,
                    apic_id,
                )?,
            )?;
            Ok(())
        }
        None => Err(Error::NotFound),
    }
}

/// Initialize the system I/O APICS
///
/// This function should only be called by the BSP
pub unsafe fn init_ioapics(madt: &MADT) -> Result<()> {
    let mut ioapics = ArrayVec::new();
    for ioapic in madt.structures().filter_map(|ics| match ics {
        Ok(ioapic @ Ics::IoApic { .. }) => match IoApic::try_from(ioapic) {
            Ok(ioapic) => Some(ioapic),
            Err(e) => {
                warn!("Invalid IOAPIC in MADT: {:?}", e);
                None
            }
        },
        _ => None,
    }) {
        debug!("Registering IOAPIC for gsi_base = 0x{:x}", ioapic.gsi_base);
        ioapics.push(ioapic);
    }
    RoAfterInit::init(&IOAPICS, ioapics);
    Ok(())
}

/// The raw interface for the I/O APIC.
pub struct IoApic {
    /// 32-bit physical address to access this I/O APIC.
    pub addr_lock: Mutex<*mut u8>,
    /// Global System Interrupt number where this I/O APIC's interrupt
    /// input starts.
    pub gsi_base: u32,
}

// IoApics are actually Send/Sync. This will not be correctly derived
// because raw pointers are not send (even when protected by a mutex).
unsafe impl Send for IoApic {}
unsafe impl Sync for IoApic {}

impl IoApic {
    /// Create a new raw IoApic structure from the given
    /// base address and global system interrupt base.
    fn new(addr: *mut u8, gsi_base: u32) -> Result<IoApic> {
        let ioapic = IoApic {
            addr_lock: Mutex::new(addr),
            gsi_base,
        };

        // From section 3.2.2 the version number of the I/O APIC
        // should be 0x11
        if ioapic.version() != IOAPIC_VERSION {
            Err(Error::NotSupported)
        } else {
            Ok(ioapic)
        }
    }

    /// Unsafe utility function for reading a 32-bit value from an
    /// I/O APIC register.
    ///
    /// See section 3.0 of the I/O APIC specification.
    unsafe fn read_raw(&self, reg: u8) -> u32 {
        let addr = self.addr_lock.lock();
        ptr::write_volatile(*addr, reg);
        ptr::read_volatile(addr.offset(IOWIN_OFFSET) as *const u32)
    }

    /// Unsafe utility function for writing a 32-bit value to an
    /// I/O APIC register.
    ///
    /// See section 3.0 of the I/O APIC specification.
    unsafe fn write_raw(&self, reg: u8, val: u32) {
        let addr = self.addr_lock.lock();
        ptr::write_volatile(*addr, reg);
        ptr::write_volatile(addr.offset(IOWIN_OFFSET) as *mut u32, val)
    }

    /// The ID of this I/O APIC.
    ///
    /// See section 3.2.1 of the I/O APIC specification.
    pub fn id(&self) -> u8 {
        let raw_id = unsafe { self.read_raw(reg::IOAPICID) };
        ((raw_id >> 24) & 0x0f) as u8
    }

    /// Set the ID of this I/O APIC.
    ///
    /// See section 3.2.1 of the I/O APIC specification.
    pub fn set_id(&self, id: u32) -> Result<()> {
        if id > 0x0f {
            Err(Error::InvalidValue(format!(
                "I/O APIC ID `0x{:x}` too large",
                id
            )))
        } else {
            unsafe {
                self.write_raw(reg::IOAPICID, id << 24);
            }
            Ok(())
        }
    }

    /// The version of this I/O APIC.
    ///
    /// See section 3.2.2 of the I/O APIC specification.
    pub fn version(&self) -> u8 {
        let raw_version = unsafe { self.read_raw(reg::IOAPICVER) };
        (raw_version & 0xff) as u8
    }

    /// The maximum redirection entry for this I/O APIC.
    ///
    /// See section 3.2.2 of the I/O APIC specification.
    pub fn max_redirection_entry(&self) -> u8 {
        let raw_version = unsafe { self.read_raw(reg::IOAPICVER) };
        ((raw_version >> 16) & 0xff) as u8
    }

    /// The arbitration ID for this I/O APIC.
    ///
    /// See section 3.2.3 of the I/O APIC specification.
    pub fn arbitration_id(&self) -> u8 {
        let raw_arb_id = unsafe { self.read_raw(reg::IOAPICARB) };
        ((raw_arb_id >> 24) & 0x0f) as u8
    }

    /// Set the arbitration ID for this I/O APIC.
    ///
    /// See section 3.2.3 of the I/O APIC specification.
    pub fn set_arbitration_id(&self, id: u8) -> Result<()> {
        if id > 15 {
            Err(Error::InvalidValue(format!(
                "I/O APIC Arbitration ID `0x{:x}` too large",
                id
            )))
        } else {
            unsafe {
                self.write_raw(reg::IOAPICARB, (id as u32) << 24);
            }
            Ok(())
        }
    }

    /// Read the raw 64-bit value from the IO Redirect Table Register
    /// for the given ID.
    ///
    /// See section 3.2.4 of the I/O APIC specification.
    unsafe fn read_ioredtbl_raw(&self, id: u8) -> u64 {
        let base_reg = reg::IOREDTBL_OFFSET + (id * 2);
        let low_order = self.read_raw(base_reg) as u64;
        let high_order = self.read_raw(base_reg + 1) as u64;

        low_order | high_order << 32
    }

    /// Read the IO Redirect Table Register for a given id.
    pub fn read_ioredtbl(&self, id: u8) -> Result<IoRedTblEntry> {
        if id > 23 {
            Err(Error::InvalidValue(format!(
                "I/O APIC IO Redirect Table register`0x{:x}` too large",
                id
            )))
        } else {
            let bits = unsafe { self.read_ioredtbl_raw(id) };

            IoRedTblEntry::try_from(bits)
        }
    }

    /// Write the raw 64-bit value to the IO Redirect Table Register
    /// for the given ID.
    ///
    /// While reading from the IO Redirect Table is reasonably simple,
    /// when writing to the table, there are several read-only
    /// bits we should not attempt to write to.
    ///
    /// See section 3.2.4 of the I/O APIC specification.
    unsafe fn write_ioredtbl_raw(&self, id: u8, val: u64) {
        let base_reg = reg::IOREDTBL_OFFSET + (id * 2);
        let low_order = (val & 0xffffffff) as u32;
        let high_order = (val >> 32) as u32;
        self.write_raw(base_reg, low_order);
        self.write_raw(base_reg + 1, high_order);
    }

    /// Write to the IO Redirect Table Register for a given id.
    pub fn write_ioredtbl(&self, id: u8, entry: IoRedTblEntry) -> Result<()> {
        let val: u64 = entry.into();
        if id > 23 {
            Err(Error::InvalidValue(format!(
                "I/O APIC IO Redirect Table register`0x{:x}` too large",
                id
            )))
        } else if (val & !IOREDTBL_RW_MASK) != 0 {
            Err(Error::InvalidValue(format!(
                "Read-only IO Redirect Table Entry bits set: 0x{:x}",
                val & !IOREDTBL_RW_MASK
            )))
        } else {
            unsafe {
                self.write_ioredtbl_raw(id, val);
            }
            Ok(())
        }
    }

    /// convenience function to get a Range of the interrupt vectors
    /// that should be associated with this IoApic.
    pub fn get_ivec_range(&self) -> Range<u32> {
        return Range {
            start: self.gsi_base,
            end: self.gsi_base + (self.max_redirection_entry() as u32),
        };
    }
}

impl TryFrom<Ics> for IoApic {
    type Error = Error;

    fn try_from(value: Ics) -> Result<IoApic> {
        match value {
            Ics::IoApic {
                ioapic_addr,
                gsi_base,
                ..
            } => IoApic::new(ioapic_addr, gsi_base),
            _ => Err(Error::InvalidValue(format!(
                "Attempting to create an IoApic from: {:?}",
                value.ics_type()
            ))),
        }
    }
}

impl fmt::Debug for IoApic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "IOAPIC: id: {} version: 0x{:x} address: {:p} GSI: {}",
            self.id(),
            self.version(),
            *self.addr_lock.lock(),
            self.gsi_base
        )
    }
}

/// Type of the I/O APIC Destination Field.
///
///  - Physical: bits `59:56` contain an APIC ID
///  - Logical: bits `63:56` specify the logical address, which
///    may be a set of processors.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DestinationMode {
    /// Indicates that the destination is an APIC ID.
    Physical = 0x00,
    /// Indicates that the destination is a logical address.
    Logical = 0x01,
}

impl TryFrom<u8> for DestinationMode {
    type Error = Error;

    fn try_from(value: u8) -> Result<DestinationMode> {
        match value {
            0x00 => Ok(DestinationMode::Physical),
            0x01 => Ok(DestinationMode::Logical),
            _ => Err(Error::InvalidValue(format!(
                "Invalid destination mode: 0x{:x}",
                value
            ))),
        }
    }
}

/// Type of signal on the interrupt pin that triggers an interrupt.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerMode {
    /// Edge sensitive trigger.
    Edge = 0x00,
    /// Level sensitive trigger.
    Level = 0x01,
}

impl TryFrom<u8> for TriggerMode {
    type Error = Error;

    fn try_from(value: u8) -> Result<TriggerMode> {
        match value {
            0x00 => Ok(TriggerMode::Edge),
            0x01 => Ok(TriggerMode::Level),
            _ => Err(Error::InvalidValue(format!(
                "Invalid trigger mode: 0x{:x}",
                value
            ))),
        }
    }
}

/// Polarity of the input signal.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinPolarity {
    /// Active High
    ActiveHigh = 0x00,
    /// Active Low
    ActiveLow = 0x01,
}

impl TryFrom<u8> for PinPolarity {
    type Error = Error;

    fn try_from(value: u8) -> Result<PinPolarity> {
        match value {
            0x00 => Ok(PinPolarity::ActiveHigh),
            0x01 => Ok(PinPolarity::ActiveLow),
            _ => Err(Error::InvalidValue(format!(
                "Invalid pin polarity: 0x{:x}",
                value
            ))),
        }
    }
}

/// Delivery Mode
///
/// A 3-bit field that specifying how the destination APICs should act
/// upon receiving the signal.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeliveryMode {
    /// Deliver the signal on the INTR signal of all processor cores
    /// listed in the destination.
    Fixed = 0b000,
    /// Deliver the signal on the INTR signal of all processor core
    /// that is executing the lowest priority among listed processors.
    LowestPriority = 0b001,
    /// System Management Interrupt. Must be edge triggered and vector
    /// must be all zeros.
    SMI = 0b010,
    /// Deliver the signal on the NMI signal of all processor cores
    /// listed. Must be edge triggered.
    NMI = 0b100,
    /// All addressed local APICs will assume their INIT state. Must
    /// be edge triggered.
    INIT = 0b101,
    /// Deliver the signal to the INTR signal of all processor cores
    /// listed in destination. Must be edge triggered.
    ExtINT = 0b111,
}

impl DeliveryMode {
    fn valid_for_level_trigger(&self) -> bool {
        match *self {
            DeliveryMode::Fixed | DeliveryMode::LowestPriority => true,
            _ => false,
        }
    }
}

impl TryFrom<u8> for DeliveryMode {
    type Error = Error;

    fn try_from(value: u8) -> Result<DeliveryMode> {
        match value {
            0b000 => Ok(DeliveryMode::Fixed),
            0b001 => Ok(DeliveryMode::LowestPriority),
            0b010 => Ok(DeliveryMode::SMI),
            0b100 => Ok(DeliveryMode::NMI),
            0b101 => Ok(DeliveryMode::INIT),
            0b111 => Ok(DeliveryMode::ExtINT),
            _ => Err(Error::InvalidValue(format!(
                "Invalid pin polarity: 0x{:x}",
                value
            ))),
        }
    }
}

/// The status of the delivery of this interrupt.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeliveryStatus {
    /// No activity on this interrupt.
    Idle = 0x00,
    /// Interrupt has been injected, but is currently pending.
    SendPending = 0x01,
}

impl TryFrom<u8> for DeliveryStatus {
    type Error = Error;

    fn try_from(value: u8) -> Result<DeliveryStatus> {
        match value {
            0x00 => Ok(DeliveryStatus::Idle),
            0x01 => Ok(DeliveryStatus::SendPending),
            _ => Err(Error::InvalidValue(format!(
                "Invalid delivery status: 0x{:x}",
                value
            ))),
        }
    }
}

/// A entry in the I/O Redirection Table.
#[derive(Debug, Clone)]
pub struct IoRedTblEntry {
    /// The interrupt vector.
    vector: u8,
    /// The action the APIC should take on signal.
    delivery_mode: DeliveryMode,
    /// The interpretation of the destination field.
    destination_mode: DestinationMode,
    /// The current status of the delivery of this interrupt.
    delivery_status: DeliveryStatus,
    /// Polarity of the input signal.
    pin_polarity: PinPolarity,
    /// This is used for level triggered interrupts, this bit is set
    /// to `true` when local APIC(s) accept the level interrupt sent by the
    /// IOAPIC. The Remote IRR bit is set to 0 when an EOI message with
    /// a matching interrupt vector is received from a local APIC.
    remote_irr: bool,
    /// Type of singal on interrupt pin.
    trigger_mode: TriggerMode,
    /// The interrupt signal is masked.
    interrupt_mask: bool,
    /// The destination field may be a logical set of processors or an
    /// APIC ID.
    /// let
    destination: u8,
}

impl IoRedTblEntry {
    /// Create a IO Redirection Table Entry.
    pub fn new(
        vector: u8,
        delivery_mode: DeliveryMode,
        destination_mode: DestinationMode,
        pin_polarity: PinPolarity,
        trigger_mode: TriggerMode,
        interrupt_mask: bool,
        destination: u8,
    ) -> Result<IoRedTblEntry> {
        let entry = IoRedTblEntry {
            vector,
            delivery_mode,
            destination_mode,
            delivery_status: DeliveryStatus::Idle,
            pin_polarity,
            remote_irr: false,
            trigger_mode,
            interrupt_mask,
            destination,
        };

        entry.validate()?;

        Ok(entry)
    }

    /// Perform basic validity checks found in the table from section 3.2.4
    /// in the I/O APIC specification.
    fn validate(&self) -> Result<()> {
        if self.trigger_mode == TriggerMode::Level
            && !self.delivery_mode.valid_for_level_trigger()
        {
            return Err(Error::InvalidValue(format!(
                "The delivery mode `0b{:b}` is invalid for level trigger mode",
                self.delivery_mode as u8
            )));
        }

        // When the physical destination mode is used the address can be only
        // 4 bits. See the table in section 3.2.4 of the I/O APIC spec for
        // details.
        if self.destination_mode == DestinationMode::Physical
            && self.destination > 15
        {
            return Err(Error::InvalidValue(format!(
                "Invalid Physical APIC ID destination: 0x{:x}",
                self.destination
            )));
        }

        if self.delivery_mode == DeliveryMode::SMI && self.vector != 0 {
            return Err(Error::InvalidValue(format!(
                "SMI delivery mode requires an empty vector: 0x{:x}",
                self.vector
            )));
        }

        Ok(())
    }
}

impl TryFrom<u64> for IoRedTblEntry {
    type Error = Error;

    fn try_from(bits: u64) -> Result<IoRedTblEntry> {
        if (bits & !IOREDTBL_KNOWN_BITS_MASK) != 0 {
            // An unknown bit was found.
            return Err(Error::NotSupported);
        }

        let vector = (bits & 0xff) as u8;
        let delivery_mode = DeliveryMode::try_from(((bits >> 8) & 0x7) as u8)?;
        let destination_mode =
            DestinationMode::try_from(((bits >> 11) & 0x1) as u8)?;
        let delivery_status =
            DeliveryStatus::try_from(((bits >> 12) & 0x1) as u8)?;
        let pin_polarity = PinPolarity::try_from(((bits >> 13) & 0x1) as u8)?;
        let remote_irr = ((bits >> 14) & 0x1) != 0;
        let trigger_mode = TriggerMode::try_from(((bits >> 15) & 0x1) as u8)?;
        let interrupt_mask = ((bits >> 16) & 0x1) != 0;

        let destination = ((bits >> 56) & 0xff) as u8;

        let entry = IoRedTblEntry {
            vector,
            delivery_mode,
            destination_mode,
            delivery_status,
            pin_polarity,
            trigger_mode,
            destination,
            remote_irr,
            interrupt_mask,
        };

        entry.validate()?;

        Ok(entry)
    }
}

impl From<IoRedTblEntry> for u64 {
    fn from(entry: IoRedTblEntry) -> u64 {
        let mut bits = entry.vector as u64;

        bits |= (entry.delivery_mode as u64) << 8;
        bits |= (entry.destination_mode as u64) << 11;
        bits |= (entry.delivery_status as u64) << 12;
        bits |= (entry.pin_polarity as u64) << 13;

        if entry.remote_irr {
            bits |= 1 << 14;
        }

        bits |= (entry.trigger_mode as u64) << 15;

        if entry.interrupt_mask {
            bits |= 1 << 16;
        }

        bits |= (entry.destination as u64) << 56;
        bits
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::string::ToString;

    fn get_ioapic(buf: *mut u8) -> Result<IoApic> {
        let ics = Ics::IoApic {
            ioapic_id: 0,
            ioapic_addr: buf,
            gsi_base: 0,
        };
        IoApic::try_from(ics)
    }

    #[test]
    fn ioredtblentry_roundtrip() {
        let all_edge = IOREDTBL_KNOWN_BITS_MASK ^ (1 << 15);
        assert_eq!(
            all_edge,
            u64::from(IoRedTblEntry::try_from(all_edge).unwrap())
        );

        let all_level = IOREDTBL_KNOWN_BITS_MASK ^ 0x700;
        assert_eq!(
            all_level,
            u64::from(IoRedTblEntry::try_from(all_level).unwrap())
        );

        let none = 0x00000000_00000000;
        assert_eq!(none, u64::from(IoRedTblEntry::try_from(none).unwrap()));
    }

    #[test]
    fn ioredtblentry_invalid_trigger_mode() {
        // ExtINT is invalid for level trigger mode.
        let invalid_for_level = 0x0f000000_00008700;
        let err = Error::InvalidValue(
            "The delivery mode `0b111` is invalid for level trigger mode"
                .to_string(),
        );
        assert_eq!(
            IoRedTblEntry::try_from(invalid_for_level).unwrap_err(),
            err
        );
    }

    #[test]
    fn ioredtblentry_unknown_bit() {
        let unknown_bit = 0xff000000_0009ffff;
        assert_eq!(
            IoRedTblEntry::try_from(unknown_bit).unwrap_err(),
            Error::NotSupported
        );
    }

    #[test]
    fn ioredtblentry_invalid_dest() {
        // Destination is a full byte but a physical destination mode
        // is used.
        let invalid_dest = 0xff000000_0000_0000;
        let err = Error::InvalidValue(
            "Invalid Physical APIC ID destination: 0xff".to_string(),
        );
        assert_eq!(err, IoRedTblEntry::try_from(invalid_dest).unwrap_err());
    }

    #[test]
    fn ioredtblentry_write_ro_bit() {
        let mut buf: [u8; 24] = [
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            IOAPIC_VERSION,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];
        let ioapic = get_ioapic(buf.as_mut_ptr()).unwrap();
        let bits = 0x0f000000_0000_1000;
        let entry = IoRedTblEntry::try_from(bits).unwrap();

        let err = Error::InvalidValue(
            "Read-only IO Redirect Table Entry bits set: 0x1000".to_string(),
        );
        // The delivery status is set, which should be read-only.
        assert_eq!(err, ioapic.write_ioredtbl(0, entry).unwrap_err());
    }

    #[test]
    fn ioapic_unsupported_version() {
        const BAD_VERSION: u8 = 0xa5;
        let mut buf: [u8; 24] = [
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            BAD_VERSION,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        assert_eq!(
            Error::NotSupported,
            get_ioapic(buf.as_mut_ptr()).unwrap_err()
        );
    }
}
