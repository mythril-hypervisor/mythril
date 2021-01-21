use crate::error::{self, Error, Result};
use crate::memory::GuestPhysAddr;
use crate::{vcpu, vmcs};
use alloc::fmt::Debug;
use bitflags::bitflags;
use core::convert::TryFrom;
use num_enum::TryFromPrimitive;

extern "C" {
    pub fn vmexit_handler_wrapper();
}

#[repr(C)]
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

    // Pushed during VCpu construction
    pub vcpu: *mut vcpu::VCpu,
}

#[no_mangle]
pub extern "C" fn vmexit_handler(state: *mut GuestCpuState) {
    let state = unsafe { state.as_mut() }.expect("Guest cpu sate is NULL");
    let vcpu = unsafe { state.vcpu.as_mut() }.expect("VCpu state is NULL");

    let reason = ExitReason::from_active_vmcs(&mut vcpu.vmcs)
        .expect("Failed to get vm reason");

    if let Err(e) = vcpu.handle_vmexit(state, reason) {
        // Build the reason again, because we don't want to clone
        // in the typical (non-error) case.
        let reason = ExitReason::from_active_vmcs(&mut vcpu.vmcs)
            .expect("Failed to get vm reason");
        info!("exit reason = {:?}", reason);
        panic!("Failed to handle vmexit: {:?}", e);
    }
}

#[no_mangle]
pub extern "C" fn vmresume_failure_handler(rflags: u64) {
    error::check_vm_insruction(rflags, "Failed to vmresume".into())
        .expect("vmresume failed");
}

pub trait ExtendedExitInformation
where
    Self: core::marker::Sized,
{
    fn from_active_vmcs(vmcs: &vmcs::ActiveVmcs) -> Result<Self>;
}

#[derive(Clone, Debug)]
pub struct ExitReason {
    pub flags: ExitReasonFlags,
    pub info: ExitInformation,
}

// See Table C-1 in Appendix C
#[derive(Clone, Debug)]
pub enum ExitInformation {
    NonMaskableInterrupt(VectoredEventInformation),
    ExternalInterrupt(VectoredEventInformation),
    TripleFault,
    InitSignal,
    StartUpIpi,
    IoSystemManagementInterrupt,
    OtherSystemManagementInterrupt,
    InterruptWindow,
    NonMaskableInterruptWindow,
    TaskSwitch,
    CpuId,
    GetSec,
    Hlt,
    Invd,
    InvlPg,
    Rdpmc,
    Rdtsc,
    Rsm,
    VmCall,
    VmClear,
    VmLaunch,
    VmPtrLd,
    VmPtrRst,
    VmRead,
    VmResume,
    VmWrite,
    VmxOff,
    VmxOn,
    CrAccess(CrInformation),
    MovDr,
    IoInstruction(IoInstructionInformation),
    RdMsr,
    WrMsr,
    VmEntryInvalidGuestState,
    VmEntryMsrLoad,
    Mwait,
    MonitorTrapFlag,
    Monitor,
    Pause,
    VmEntryMachineCheck,
    TprBelowThreshold,
    ApicAccess(ApicAccessInformation),
    VirtualEio,
    AccessGdtridtr,
    AccessLdtrTr,
    EptViolation(EptInformation),
    EptMisconfigure,
    InvEpt,
    Rdtscp,
    VmxPreemptionTimerExpired,
    Invvpid,
    Wbinvd,
    Xsetbv,
    ApicWrite,
    RdRand,
    Invpcid,
    VmFunc,
    Encls,
    RdSeed,
    PageModificationLogFull,
    Xsaves,
    Xrstors,
}

impl ExitReason {
    fn from_active_vmcs(vmcs: &mut vmcs::ActiveVmcs) -> Result<Self> {
        let reason = vmcs.read_field(vmcs::VmcsField::VmExitReason)?;
        let basic_reason = (reason & 0x7fff) as u32;
        let flags = ExitReasonFlags::from_bits_truncate(reason);
        let info = match basic_reason {
            0 => ExitInformation::NonMaskableInterrupt(
                VectoredEventInformation::from_active_vmcs(vmcs)?,
            ),
            1 => ExitInformation::ExternalInterrupt(
                VectoredEventInformation::from_active_vmcs(vmcs)?,
            ),
            2 => ExitInformation::TripleFault,
            3 => ExitInformation::InitSignal,
            4 => ExitInformation::StartUpIpi,
            5 => ExitInformation::IoSystemManagementInterrupt,
            6 => ExitInformation::OtherSystemManagementInterrupt,
            7 => ExitInformation::InterruptWindow,
            8 => ExitInformation::NonMaskableInterruptWindow,
            9 => ExitInformation::TaskSwitch,
            10 => ExitInformation::CpuId,
            11 => ExitInformation::GetSec,
            12 => ExitInformation::Hlt,
            13 => ExitInformation::Invd,
            14 => ExitInformation::InvlPg,
            15 => ExitInformation::Rdpmc,
            16 => ExitInformation::Rdtsc,
            17 => ExitInformation::Rsm,
            18 => ExitInformation::VmCall,
            19 => ExitInformation::VmClear,
            20 => ExitInformation::VmLaunch,
            21 => ExitInformation::VmPtrLd,
            22 => ExitInformation::VmPtrRst,
            23 => ExitInformation::VmRead,
            24 => ExitInformation::VmResume,
            25 => ExitInformation::VmWrite,
            26 => ExitInformation::VmxOff,
            27 => ExitInformation::VmxOn,
            28 => ExitInformation::CrAccess(CrInformation::from_active_vmcs(
                vmcs,
            )?),
            29 => ExitInformation::MovDr,
            30 => ExitInformation::IoInstruction(
                IoInstructionInformation::from_active_vmcs(vmcs)?,
            ),
            31 => ExitInformation::RdMsr,
            32 => ExitInformation::WrMsr,
            33 => ExitInformation::VmEntryInvalidGuestState,
            34 => ExitInformation::VmEntryMsrLoad,
            // 35 is unused
            36 => ExitInformation::Mwait,
            37 => ExitInformation::MonitorTrapFlag,
            // 38 is unused
            39 => ExitInformation::Monitor,
            40 => ExitInformation::Pause,
            41 => ExitInformation::VmEntryMachineCheck,
            43 => ExitInformation::TprBelowThreshold,
            44 => ExitInformation::ApicAccess(
                ApicAccessInformation::from_active_vmcs(vmcs)?,
            ),
            45 => ExitInformation::VirtualEio,
            46 => ExitInformation::AccessGdtridtr,
            47 => ExitInformation::AccessLdtrTr,
            48 => ExitInformation::EptViolation(
                EptInformation::from_active_vmcs(vmcs)?,
            ),
            49 => ExitInformation::EptMisconfigure,
            50 => ExitInformation::InvEpt,
            51 => ExitInformation::Rdtscp,
            52 => ExitInformation::VmxPreemptionTimerExpired,
            53 => ExitInformation::Invvpid,
            54 => ExitInformation::Wbinvd,
            55 => ExitInformation::Xsetbv,
            56 => ExitInformation::ApicWrite,
            57 => ExitInformation::RdRand,
            58 => ExitInformation::Invpcid,
            59 => ExitInformation::VmFunc,
            60 => ExitInformation::Encls,
            61 => ExitInformation::RdSeed,
            62 => ExitInformation::PageModificationLogFull,
            63 => ExitInformation::Xsaves,
            64 => ExitInformation::Xrstors,
            reason => {
                return Err(Error::InvalidValue(format!(
                    "Unexpected basic vmexit reason: {}",
                    reason
                )))
            }
        };
        Ok(ExitReason {
            flags: flags,
            info: info,
        })
    }
}

#[derive(Clone, Debug, TryFromPrimitive, PartialEq)]
#[repr(u8)]
pub enum ApicAccessKind {
    LinearRead = 0,
    LinearWrite = 1,
    LinearFetch = 2,
    LinearEventDelivery = 3,
    PhysicalAccessDuringEvent = 10,
    PhysicalAccessDuringFetch = 15,
}

#[derive(Clone, Debug)]
pub struct ApicAccessInformation {
    pub offset: Option<u16>,
    pub kind: ApicAccessKind,
    pub async_instr_exec: bool,
}

impl ExtendedExitInformation for ApicAccessInformation {
    fn from_active_vmcs(vmcs: &vmcs::ActiveVmcs) -> Result<Self> {
        let qualifier = vmcs.read_field(vmcs::VmcsField::ExitQualification)?;

        let kind = ((qualifier >> 12) & 0b1111) as u8;
        let kind = ApicAccessKind::try_from(kind)?;

        let offset = if kind == ApicAccessKind::LinearRead
            || kind == ApicAccessKind::LinearWrite
            || kind == ApicAccessKind::LinearFetch
            || kind == ApicAccessKind::LinearEventDelivery
        {
            Some((qualifier & 0xfff) as u16)
        } else {
            None
        };

        Ok(ApicAccessInformation {
            kind: kind,
            async_instr_exec: qualifier & (1 << 16) != 0,
            offset: offset,
        })
    }
}

#[derive(Clone, Debug)]
pub struct IoInstructionInformation {
    pub size: u8,
    pub input: bool,
    pub string: bool,
    pub rep: bool,
    pub immediate: bool,
    pub port: u16,
}

impl ExtendedExitInformation for IoInstructionInformation {
    fn from_active_vmcs(vmcs: &vmcs::ActiveVmcs) -> Result<Self> {
        let qualifier = vmcs.read_field(vmcs::VmcsField::ExitQualification)?;
        Ok(IoInstructionInformation {
            size: (qualifier & 7) as u8 + 1,
            input: qualifier & (1 << 3) != 0,
            string: qualifier & (1 << 4) != 0,
            rep: qualifier & (1 << 5) != 0,
            immediate: qualifier & (1 << 6) != 0,
            port: ((qualifier & 0xffff0000) >> 16) as u16,
        })
    }
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

impl ExtendedExitInformation for VectoredEventInformation {
    fn from_active_vmcs(vmcs: &vmcs::ActiveVmcs) -> Result<Self> {
        let inter_info = vmcs.read_field(vmcs::VmcsField::VmExitIntrInfo)?;
        let inter_error =
            vmcs.read_field(vmcs::VmcsField::VmExitIntrErrorCode)?;

        let error_code = if inter_info & (1 << 11) != 0 {
            Some(inter_error as u32)
        } else {
            None
        };

        Ok(VectoredEventInformation {
            vector: (inter_info & 0xff) as u8,
            interrupt_type: InterruptType::try_from(
                ((inter_info & 0x700) >> 8) as u8,
            )?,
            error_code: error_code,
            nmi_unblocking_iret: inter_info & (1 << 12) != 0,
            valid: inter_info & (1 << 31) != 0,
        })
    }
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

impl ExtendedExitInformation for EptInformation {
    fn from_active_vmcs(vmcs: &vmcs::ActiveVmcs) -> Result<Self> {
        let qualifier = vmcs.read_field(vmcs::VmcsField::ExitQualification)?;
        let guest_linear_addr = if qualifier & (1 << 7) != 0 {
            Some(GuestPhysAddr::new(
                vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?,
            ))
        } else {
            None
        };
        let guest_phys_addr = GuestPhysAddr::new(
            vmcs.read_field(vmcs::VmcsField::GuestPhysicalAddress)?,
        );

        Ok(EptInformation {
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
        })
    }
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
    pub fn read(
        &self,
        vmcs: &vmcs::ActiveVmcs,
        guest_cpu: &GuestCpuState,
    ) -> Result<u64> {
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
            MovCrRegister::Rsp => {
                vmcs.write_field(vmcs::VmcsField::GuestRsp, value)?
            }
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

impl ExtendedExitInformation for CrInformation {
    fn from_active_vmcs(vmcs: &vmcs::ActiveVmcs) -> Result<Self> {
        let qualifier = vmcs.read_field(vmcs::VmcsField::ExitQualification)?;
        let access_type =
            CrAccessType::try_from(((qualifier & 0b110000) >> 4) as u8)?;
        let reg = ((qualifier & 0xf00) >> 8) as u8;
        let cr_num = (qualifier & 0b1111) as u8;
        let (cr_num, reg, source) = match access_type {
            CrAccessType::MovToCr | CrAccessType::MovFromCr => {
                (cr_num, Some(MovCrRegister::try_from(reg)?), None)
            }
            _ => (0, None, Some(((qualifier & 0xffff0000) >> 16) as u16)),
        };
        Ok(CrInformation {
            cr_num: cr_num,
            access_type: access_type,
            lmsw_memory_operand: qualifier & (1 << 6) != 0,
            register: reg,
            lmsw_data: source,
        })
    }
}

bitflags! {
    pub struct ExitReasonFlags: u64 {
        const ENCLAVE_MODE =        1 << 27;
        const PENDING_MTF_EXIT =    1 << 28;
        const EXIT_FROM_ROOT =      1 << 29;
        const VM_ENTRY_FAIL =       1 << 31;
    }
}
