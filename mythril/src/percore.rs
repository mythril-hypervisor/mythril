#![deny(missing_docs)]

//! # Per-core variable support
//!
//! This module defines macros for declaring per-core variables. A new variable
//! can be declared with `declare_per_core!` and can be accessed with `get_per_core!`
//! and `get_per_core_mut!`. These access methods must not be used prior to the
//! invocation of `init_sections` by the BSP.

use crate::error::Result;
use crate::lock::ro_after_init::RoAfterInit;
use alloc::vec::Vec;
use core::fmt;
use x86::msr;

static AP_PER_CORE_SECTIONS: RoAfterInit<Vec<u8>> =
    RoAfterInit::uninitialized();

crate::declare_per_core! {
    static mut CORE_ID: RoAfterInit<CoreId> =
        RoAfterInit::uninitialized();
}

extern "C" {
    static PER_CORE_START: u8;
    static PER_CORE_END: u8;
}

unsafe fn per_core_section_len() -> usize {
    let section_start = &PER_CORE_START as *const u8;
    let section_end = &PER_CORE_END as *const u8;
    section_end as usize - section_start as usize
}

unsafe fn per_core_address(symbol_addr: *const u8) -> *const u8 {
    let offset = symbol_addr as u64 - (&PER_CORE_START as *const _ as u64);

    let section_start = msr::rdmsr(msr::IA32_FS_BASE);
    (section_start + offset) as *const u8
}

/// Initialize the per-core sections
///
/// This must be called after the global allocator has been
/// initialized and may only be called by the BSP.
pub unsafe fn init_sections(ncores: usize) -> Result<()> {
    let section_start = &PER_CORE_START as *const u8;
    let section_len = per_core_section_len();
    let per_core_section =
        core::slice::from_raw_parts(section_start, section_len);

    let mut ap_sections = Vec::with_capacity(section_len * (ncores - 1));
    for _ in 0..ncores - 1 {
        ap_sections.extend_from_slice(per_core_section);
    }

    RoAfterInit::init(&AP_PER_CORE_SECTIONS, ap_sections);

    // Also initialize things for this core (which must be the BSP)
    init_segment_for_core(0);
    Ok(())
}

/// Initialize this core's per-core data
///
/// This must be called by each AP (and the BSP) before
/// the usage of any per-core variable
pub unsafe fn init_segment_for_core(core_idx: u64) {
    let fs = if core_idx == 0 {
        &PER_CORE_START as *const u8 as u64
    } else {
        let section_len = per_core_section_len();
        (&AP_PER_CORE_SECTIONS[section_len * (core_idx - 1) as usize])
            as *const u8 as u64
    };

    msr::wrmsr(msr::IA32_FS_BASE, fs);

    RoAfterInit::init(
        crate::get_per_core_mut!(CORE_ID),
        CoreId {
            raw: core_idx as u32,
        },
    );
}

/// The sequential index of a core
#[derive(Copy, Clone, Debug, Ord, PartialEq, PartialOrd, Eq)]
pub struct CoreId {
    /// The raw ID as an integer
    pub raw: u32,
}

impl From<u32> for CoreId {
    fn from(value: u32) -> Self {
        CoreId { raw: value }
    }
}

impl fmt::Display for CoreId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self.raw)
    }
}

/// Get this current core's sequential index
///
/// This _must not_ be called before calling `init_segment_for_core`
pub fn read_core_id() -> CoreId {
    unsafe { **crate::get_per_core!(CORE_ID) }
}

#[doc(hidden)]
pub unsafe fn get_per_core_impl<T>(t: &T) -> &T {
    core::mem::transmute(per_core_address(t as *const T as *const u8))
}

#[doc(hidden)]
pub unsafe fn get_per_core_mut_impl<T>(t: &mut T) -> &mut T {
    core::mem::transmute(per_core_address(t as *const T as *const u8))
}

#[macro_export]
macro_rules! get_per_core {
    ($name:ident) => {
        #[allow(unused_unsafe)]
        unsafe {
            $crate::percore::get_per_core_impl(&$name)
        }
    };
}

#[macro_export]
macro_rules! get_per_core_mut {
    ($name:ident) => {
        #[allow(unused_unsafe)]
        unsafe {
            $crate::percore::get_per_core_mut_impl(&mut $name)
        }
    };
}

// The following macros are derived from lazy-static
#[macro_export(local_inner_macros)]
#[doc(hidden)]
macro_rules! __declare_per_core_internal {
    ($(#[$attr:meta])* ($($vis:tt)*) static mut $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        #[link_section = ".per_core"]
        $($vis)* static mut $N: $T = $e;

        declare_per_core!($($t)*);
    };
    ($(#[$attr:meta])* ($($vis:tt)*) static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        #[link_section = ".per_core"]
        $($vis)* static $N: $T = $e;

        declare_per_core!($($t)*);
    };
    () => ()
}

#[macro_export(local_inner_macros)]
macro_rules! declare_per_core {
    ($(#[$attr:meta])* static mut $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        // use `()` to explicitly forward the information about private items
        __declare_per_core_internal!($(#[$attr])* () static mut $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub static mut $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __declare_per_core_internal!($(#[$attr])* (pub) static mut $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub ($($vis:tt)+) static mut $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __declare_per_core_internal!($(#[$attr])* (pub ($($vis)+)) static mut $N : $T = $e; $($t)*);
    };
    // Rules for immutable variables
    ($(#[$attr:meta])* static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __declare_per_core_internal!($(#[$attr])* () static $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __declare_per_core_internal!($(#[$attr])* (pub) static $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub ($($vis:tt)+) static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __declare_per_core_internal!($(#[$attr])* (pub ($($vis)+)) static $N : $T = $e; $($t)*);
    };
    () => ()
}
