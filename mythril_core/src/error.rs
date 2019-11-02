use crate::vmcs;
use alloc::string::String;
use derive_try_from_primitive::TryFromPrimitive;
use x86::bits64::rflags;
use x86::bits64::rflags::RFlags;

// See Section 30.4
#[derive(Debug, TryFromPrimitive)]
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
            asm!("vmread %rax, %rdx;"
                 :"={rdx}"(value)
                 :"{rax}"(vmcs::VmcsField::VmInstructionError as u64)
                 :"rflags"
                 : "volatile");
            value
        };
        let vm_error =
            VmInstructionError::try_from(errno).unwrap_or(VmInstructionError::UnknownError);

        Err(Error::VmFailValid((vm_error, error)))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub enum Error {
    Vmcs(String),
    VmFailInvalid(String),
    VmFailValid((VmInstructionError, String)),
    AllocError(&'static str),
    MissingDevice(String),
    MissingFile(String),
    NullPtr(String),
    NotSupported,
    Uefi(String),
    InvalidValue(String),
    NotImplemented(String),
}

pub type Result<T> = core::result::Result<T, Error>;
