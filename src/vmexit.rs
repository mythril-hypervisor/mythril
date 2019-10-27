use crate::error::{self, Error, Result};
use crate::memory::GuestPhysAddr;
use crate::{vm, vmcs};
use bitflags::bitflags;
use derive_try_from_primitive::TryFromPrimitive;

extern "C" {
    pub fn vmexit_handler_wrapper();
}

#[repr(C)]
#[repr(packed)]
#[derive(Clone, Copy, Debug)]
pub struct GuestCpuState {
    pub cr2: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

#[no_mangle]
pub extern "C" fn vmexit_handler(state: *mut GuestCpuState) {
    let state = unsafe { state.as_mut() }.expect("Guest cpu sate is NULL");
    let mut vm = unsafe { vm::VMS.get_mut().as_mut().expect("Failed to get VM") };

    let reason = ExitReason::from_active_vmcs(&mut vm.vmcs).expect("Failed to get vm reason");

    info!("reached vmexit handler: {:?}", reason);
    info!("Guest cpu state: {:?}", state);
    info!("{}", vm.vmcs);

    vm.handle_vmexit(state, reason)
        .expect("Failed to handle vmexit")
}

#[no_mangle]
pub extern "C" fn vmresume_failure_handler(rflags: u64) {
    error::check_vm_insruction(rflags, "Failed to vmresume".into()).expect("vmresume failed");
}

// See Table C-1 in Appendix C
#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u32)]
pub enum BasicExitReason {
    NonMaskableInterrupt = 0,
    ExternalInterrupt = 1,
    TripleFault = 2,
    InitSignal = 3,
    StartUpIpi = 4,
    IoSystemManagementInterrupt = 5,
    OtherSystemManagementInterrupt = 6,
    InterruptWindow = 7,
    NonMaskableInterruptWindow = 8,
    TaskSwitch = 9,
    CpuId = 10,
    GetSec = 11,
    Hlt = 12,
    Invd = 13,
    InvlPg = 14,
    Rdpmc = 15,
    Rdtsc = 16,
    Rsm = 17,
    VmCall = 18,
    VmClear = 19,
    VmLaunch = 20,
    VmPtrLd = 21,
    VmPtrRst = 22,
    VmRead = 23,
    VmResume = 24,
    VmWrite = 25,
    VmxOff = 26,
    VmxOn = 27,
    CrAccess = 28,
    MovDr = 29,
    IoInstruction = 30,
    RdMsr = 31,
    WrMsr = 32,
    VmEntryInvalidGuestState = 33,
    VmEntryMsrLoad = 34,
    Mwait = 36,
    MonitorTrapFlag = 37,
    Monitor = 39,
    Pause = 40,
    VmEntryMachineCheck = 41,
    TprBelowThreshold = 43,
    ApicAccess = 44,
    VirtualEio = 45,
    AccessGdtridtr = 46,
    AccessLdtrTr = 47,
    EptViolation = 48,
    EptMisconfigure = 49,
    InvEpt = 50,
    Rdtscp = 51,
    VmxPreemptionTimerExpired = 52,
    Invvpid = 53,
    Wbinvd = 54,
    Xsetbv = 55,
    ApicWrite = 56,
    RdRand = 57,
    Invpcid = 58,
    VmFunc = 59,
    Encls = 60,
    RdSeed = 61,
    PageModificationLogFull = 62,
    Xsaves = 63,
    Xrstors = 64,

    // Not in the spec, added for our purposes
    UnknownExitReason = 65,
}

#[derive(Clone, Debug)]
pub struct IoInstructionQualification {
    pub size: u8,
    pub input: bool,
    pub string: bool,
    pub rep: bool,
    pub immediate: bool,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum InterruptType {
    ExternalInterrupt = 0,
    NonMaskableInterrupt = 1,
    HardwareException = 3,
    SoftwareException = 6,
}

#[derive(Clone, Debug)]
pub struct VectoredEventInformation {
    pub vector: u8,
    pub interrupt_type: InterruptType,
    pub error_code: Option<u32>,
    pub nmi_unblocking_iret: bool,
    pub valid: bool,
}

#[derive(Clone, Debug)]
pub struct EptInformation {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    pub read_allowed: bool,
    pub write_allowed: bool,
    pub priv_exec_allowed: bool,
    pub user_exec_allowed: bool,
    pub guest_linear_addr: Option<GuestPhysAddr>,
    pub after_page_translation: bool,
    pub user_mode_address: bool,
    pub read_write_page: bool,
    pub nx_page: bool,
    pub nmi_unblocking_iret: bool,
    pub guest_phys_addr: GuestPhysAddr,
}

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum CrAccessType {
    MovToCr = 0,
    MovFromCr = 1,
    Clts = 2,
    Lmsw = 3,
}

#[derive(Clone, Copy, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MovCrRegister {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl MovCrRegister {
    pub fn read(&self, vmcs: &vmcs::ActiveVmcs, guest_cpu: &GuestCpuState) -> Result<u64> {
        Ok(match self {
            MovCrRegister::Rax => guest_cpu.rax,
            MovCrRegister::Rcx => guest_cpu.rcx,
            MovCrRegister::Rdx => guest_cpu.rdx,
            MovCrRegister::Rbx => guest_cpu.rbx,
            MovCrRegister::Rbp => guest_cpu.rbp,
            MovCrRegister::Rsi => guest_cpu.rsi,
            MovCrRegister::Rdi => guest_cpu.rdi,
            MovCrRegister::R8 => guest_cpu.r8,
            MovCrRegister::R9 => guest_cpu.r9,
            MovCrRegister::R10 => guest_cpu.r10,
            MovCrRegister::R11 => guest_cpu.r11,
            MovCrRegister::R12 => guest_cpu.r12,
            MovCrRegister::R13 => guest_cpu.r13,
            MovCrRegister::R14 => guest_cpu.r14,
            MovCrRegister::R15 => guest_cpu.r15,
            MovCrRegister::Rsp => vmcs.read_field(vmcs::VmcsField::GuestRsp)?,
        })
    }

    pub fn write(
        &self,
        value: u64,
        vmcs: &mut vmcs::ActiveVmcs,
        guest_cpu: &mut GuestCpuState,
    ) -> Result<()> {
        match self {
            MovCrRegister::Rax => guest_cpu.rax = value,
            MovCrRegister::Rcx => guest_cpu.rcx = value,
            MovCrRegister::Rdx => guest_cpu.rdx = value,
            MovCrRegister::Rbx => guest_cpu.rbx = value,
            MovCrRegister::Rbp => guest_cpu.rbp = value,
            MovCrRegister::Rsi => guest_cpu.rsi = value,
            MovCrRegister::Rdi => guest_cpu.rdi = value,
            MovCrRegister::R8 => guest_cpu.r8 = value,
            MovCrRegister::R9 => guest_cpu.r9 = value,
            MovCrRegister::R10 => guest_cpu.r10 = value,
            MovCrRegister::R11 => guest_cpu.r11 = value,
            MovCrRegister::R12 => guest_cpu.r12 = value,
            MovCrRegister::R13 => guest_cpu.r13 = value,
            MovCrRegister::R14 => guest_cpu.r14 = value,
            MovCrRegister::R15 => guest_cpu.r15 = value,
            MovCrRegister::Rsp => vmcs.write_field(vmcs::VmcsField::GuestRsp, value)?,
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CrInformation {
    pub cr_num: u8,
    pub access_type: CrAccessType,
    pub lmsw_memory_operand: bool,
    pub register: Option<MovCrRegister>,
    pub lmsw_data: Option<u16>,
}

#[derive(Clone, Debug)]
pub enum ExitInformation {
    IoInstruction(IoInstructionQualification),
    VectoredEvent(VectoredEventInformation),
    EptInformation(EptInformation),
    CrAccess(CrInformation),
}

impl ExitInformation {
    fn from_active_vmcs(
        basic: BasicExitReason,
        vmcs: &mut vmcs::ActiveVmcs,
    ) -> Result<Option<Self>> {
        let qualifier = vmcs.read_field(vmcs::VmcsField::ExitQualification)?;
        let inter_info = vmcs.read_field(vmcs::VmcsField::VmExitIntrInfo)?;
        let inter_error = vmcs.read_field(vmcs::VmcsField::VmExitIntrErrorCode)?;
        match basic {
            BasicExitReason::CrAccess => {
                let access_type = CrAccessType::try_from(((qualifier & 0b11000) >> 3) as u8)
                    .ok_or(Error::InvalidValue("Invalid CR access type".into()))?;
                let reg = ((qualifier & 0xf00) >> 8) as u8;
                let (reg, source) = match access_type {
                    CrAccessType::MovToCr | CrAccessType::MovFromCr => (
                        Some(
                            MovCrRegister::try_from(reg)
                                .ok_or(Error::InvalidValue("Invalid general register".into()))?,
                        ),
                        None,
                    ),
                    _ => (None, Some(((qualifier & 0xffff0000) >> 16) as u16)),
                };
                Ok(Some(ExitInformation::CrAccess(CrInformation {
                    cr_num: (qualifier & 0b111) as u8,
                    access_type: access_type,
                    lmsw_memory_operand: qualifier & (1 << 6) != 0,
                    register: reg,
                    lmsw_data: source,
                })))
            }

            BasicExitReason::EptViolation => {
                let guest_linear_addr = if qualifier & (1 << 7) != 0 {
                    Some(GuestPhysAddr::new(
                        vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?,
                    ))
                } else {
                    None
                };
                let guest_phys_addr =
                    GuestPhysAddr::new(vmcs.read_field(vmcs::VmcsField::GuestPhysicalAddress)?);

                Ok(Some(ExitInformation::EptInformation(EptInformation {
                    read: qualifier & (1 << 0) != 0,
                    write: qualifier & (1 << 1) != 0,
                    exec: qualifier & (1 << 2) != 0,
                    read_allowed: qualifier & (1 << 3) != 0,
                    write_allowed: qualifier & (1 << 4) != 0,
                    priv_exec_allowed: qualifier & (1 << 5) != 0,
                    user_exec_allowed: qualifier & (1 << 6) != 0,
                    guest_linear_addr: guest_linear_addr,
                    after_page_translation: qualifier & (1 << 8) != 0,
                    user_mode_address: qualifier & (1 << 9) != 0,
                    read_write_page: qualifier & (1 << 10) != 0,
                    nx_page: qualifier & (1 << 11) != 0,
                    nmi_unblocking_iret: qualifier & (1 << 12) != 0,
                    guest_phys_addr: guest_phys_addr,
                })))
            }
            BasicExitReason::IoInstruction => {
                let size: u8 = match qualifier & 0x7 {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    3 => 4,
                    _ => unreachable!(),
                };

                Ok(Some(ExitInformation::IoInstruction(
                    IoInstructionQualification {
                        size: size,
                        input: qualifier & (1 << 3) != 0,
                        string: qualifier & (1 << 4) != 0,
                        rep: qualifier & (1 << 5) != 0,
                        immediate: qualifier & (1 << 6) != 0,
                        port: ((qualifier & 0xffff0000) >> 16) as u16,
                    },
                )))
            }
            BasicExitReason::NonMaskableInterrupt => {
                let error_code = if inter_info & (1 << 11) != 0 {
                    Some(inter_error as u32)
                } else {
                    None
                };
                Ok(Some(ExitInformation::VectoredEvent(
                    VectoredEventInformation {
                        vector: (inter_info & 0xff) as u8,
                        interrupt_type: InterruptType::try_from(((inter_info & 0x700) >> 8) as u8)
                            .ok_or(Error::InvalidValue("Invalid interrupt type".into()))?,
                        error_code: error_code,
                        nmi_unblocking_iret: inter_info & (1 << 12) != 0,
                        valid: inter_info & (1 << 31) != 0,
                    },
                )))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExitReason {
    pub flags: ExitReasonFlags,
    pub reason: BasicExitReason,
    pub information: Option<ExitInformation>,
}

bitflags! {
    pub struct ExitReasonFlags: u64 {
        const ENCLAVE_MODE =        1 << 27;
        const PENDING_MTF_EXIT =    1 << 28;
        const EXIT_FROM_ROOT =      1 << 29;
        const VM_ENTRY_FAIL =       1 << 31;
    }
}

impl ExitReason {
    fn from_active_vmcs(vmcs: &mut vmcs::ActiveVmcs) -> Result<Self> {
        let reason = vmcs.read_field(vmcs::VmcsField::VmExitReason)?;
        let basic_reason = BasicExitReason::try_from((reason & 0x7fff) as u32)
            .unwrap_or(BasicExitReason::UnknownExitReason);
        Ok(ExitReason {
            flags: ExitReasonFlags::from_bits_truncate(reason),
            reason: basic_reason,
            information: ExitInformation::from_active_vmcs(basic_reason, vmcs)?,
        })
    }
}
