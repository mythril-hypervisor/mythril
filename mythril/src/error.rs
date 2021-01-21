use crate::vmcs;
use alloc::string::String;
use arrayvec::CapacityError;
use core::convert::TryFrom;
use core::num::TryFromIntError;
use num_enum::{TryFromPrimitive, TryFromPrimitiveError};
use x86::bits64::rflags;
use x86::bits64::rflags::RFlags;

// See Section 30.4
#[derive(Debug, PartialEq, TryFromPrimitive)]
#[repr(u64)]
pub enum VmInstructionError {
    // Use to represent any error that is not in the current spec
    UnknownError = 0,

    VmCallInRoot = 1,
    VmClearInvalidAddress = 2,
    VmClearWithVmxOnPtr = 3,
    VmLaunchNonClear = 4,
    VmResumeNonLaunched = 5,
    VmResumeAfterVmxOff = 6,
    VmEntryWithInvalidCtrlFields = 7,
    VmEntryWithInvalidHostFields = 8,
    VmPtrLdWithInvalidPhysAddr = 9,
    VmPtrLdWithVmxOnPtr = 10,
    VmPtrLdWithWrongVmcsRevision = 11,
    VmReadWriteToUnsupportedField = 12,
    VmWriteToReadOnly = 13,
    // 14 is missing in the spec
    VmxOnInRootMode = 15,
    VmEntryWithInvalidExecVmcsPtr = 16,
    VmEntryWithNonLaunchExecVmcs = 17,
    VmEntryWithExecVmcsPtr = 18,
    VmCallWithNonClearVmcs = 19,
    VmCallWithInvalidVmExitFields = 20,
    // 21 is missing in the spec
    VmCallWithIncorrectMsegRev = 22,
    VmxOffUnderDualMonitor = 23,
    VmCallWithInvalidSmmFeatures = 24,
    VmEntryWithInvalidVmExecFields = 25,
    VmEntryWithEventsBlockedMovSs = 26,
    // 27 is missing in the spec
    InvalidOperandToInveptInvvpid = 28,
}

pub fn check_vm_insruction(rflags: u64, error: String) -> Result<()> {
    let rflags = rflags::RFlags::from_bits_truncate(rflags);

    if rflags.contains(RFlags::FLAGS_CF) {
        Err(Error::VmFailInvalid(error))
    } else if rflags.contains(RFlags::FLAGS_ZF) {
        let errno = unsafe {
            let value: u64;
            asm!(
                "vmread rdx, rax",
                in("rax") vmcs::VmcsField::VmInstructionError as u64,
                out("rdx") value,
                options(nostack)
            );
            value
        };
        let vm_error = VmInstructionError::try_from(errno)
            .unwrap_or(VmInstructionError::UnknownError);

        Err(Error::VmFailValid((vm_error, error)))
    } else {
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Vmcs(String),
    VmFailInvalid(String),
    VmFailValid((VmInstructionError, String)),
    DuplicateMapping(String),
    AllocError(String),
    MissingDevice(String),
    MissingFile(String),
    NullPtr(String),
    NotSupported,
    NotFound,
    Exists,
    Exhausted,
    Uefi(String),
    InvalidValue(String),
    InvalidDevice(String),
    NotImplemented(String),
    DeviceError(String),
}

impl<T: TryFromPrimitive> From<TryFromPrimitiveError<T>> for Error {
    fn from(error: TryFromPrimitiveError<T>) -> Error {
        Error::InvalidValue(format!("{}", error))
    }
}

impl From<TryFromIntError> for Error {
    fn from(error: TryFromIntError) -> Error {
        Error::InvalidValue(format!("{}", error))
    }
}

impl From<core::str::Utf8Error> for Error {
    fn from(error: core::str::Utf8Error) -> Error {
        Error::InvalidValue(format!("{}", error))
    }
}

impl<T> From<CapacityError<T>> for Error {
    fn from(_error: CapacityError<T>) -> Error {
        Error::Exhausted
    }
}

pub type Result<T> = core::result::Result<T, Error>;

#[lang = "eh_personality"]
#[cfg(not(test))]
fn eh_personality() {}

#[panic_handler]
#[cfg(not(test))]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        error!(
            "Panic in {} at ({}, {}):",
            location.file(),
            location.line(),
            location.column()
        );
        if let Some(message) = info.message() {
            error!("{}", message);
        }
    }

    loop {
        unsafe {
            // Try to at least keep CPU from running at 100%
            asm!("hlt", options(nostack, nomem));
        }
    }
}

#[alloc_error_handler]
#[cfg(not(test))]
fn out_of_memory(layout: ::core::alloc::Layout) -> ! {
    panic!(
        "Ran out of free memory while trying to allocate {:#?}",
        layout
    );
}
