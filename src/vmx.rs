use crate::error::{self, Error, Result};
use crate::vm;
use crate::vmcs;
use bitflags::bitflags;
use core::convert::TryFrom;
use derive_try_from_primitive::TryFromPrimitive;
use raw_cpuid::CpuId;
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

extern "C" {
    pub fn vmexit_handler_wrapper();
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
pub enum ExitInformation {
    IoInstruction(IoInstructionQualification),
    VectoredEvent(VectoredEventInformation),
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
                        input: qualifier & (1 << 3) == 1,
                        string: qualifier & (1 << 4) == 1,
                        rep: qualifier & (1 << 5) == 1,
                        immediate: qualifier & (1 << 6) == 1,
                        port: ((qualifier & 0xffff0000) >> 16) as u16,
                    },
                )))
            }
            BasicExitReason::NonMaskableInterrupt => {
                let error_code = if inter_info & (1 << 11) == 1 {
                    Some(inter_error as u32)
                } else {
                    None
                };
                Ok(Some(ExitInformation::VectoredEvent(
                    VectoredEventInformation {
                        vector: (inter_info & 0xff) as u8,
                        interrupt_type: InterruptType::try_from(((inter_info & 0x700) >> 8) as u8)
                            .ok_or(Error::NotSupported)?,
                        error_code: error_code,
                        nmi_unblocking_iret: inter_info & (1 << 12) == 1,
                        valid: inter_info & (1 << 31) == 1,
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

    info!("Guest cpu state: {:?}", state);
    info!("reached vmexit handler: {:?}", reason);
    info!(
        "Guest RIP: {}",
        vm.vmcs
            .read_field(vmcs::VmcsField::GuestRip)
            .expect("Failed to read guest rip")
    );

    vm.handle_vmexit(state, reason);
}

#[no_mangle]
pub extern "C" fn vmresume_failure_handler(rflags: u64) {
    error::check_vm_insruction(rflags, "Failed to vmresume".into()).expect("vmresume failed");
}

pub struct Vmx {
    vmxon_region: PhysFrame<Size4KiB>,
}

impl Vmx {
    pub fn enable(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<Self> {
        const VMX_ENABLE_FLAG: u32 = 1 << 13;

        let cpuid = CpuId::new();
        match cpuid.get_feature_info() {
            Some(finfo) if finfo.has_vmx() => Ok(()),
            _ => Err(Error::NotSupported),
        }?;

        unsafe {
            // Enable NE in CR0, This is fixed bit in VMX CR0
            asm!("movq %cr0, %rax; orq %rdx, %rax; movq %rax, %cr0;"
                 :
                 :"{rdx}"(0x20)
                 :"rax");

            // Enable vmx in CR4
            asm!("movq %cr4, %rax; orq %rdx, %rax; movq %rax, %cr4;"
                 :
                 :"{rdx}"(VMX_ENABLE_FLAG)
                 :"rax");
        }

        let revision_id = Self::revision();

        let vmxon_region = alloc
            .allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate vmxon frame"))?;
        let vmxon_region_addr = vmxon_region.start_address().as_u64();

        // Set the revision in the vmx page
        let region_revision = vmxon_region_addr as *mut u32;
        unsafe {
            *region_revision = revision_id;
        }

        let rflags = unsafe {
            let rflags: u64;
            asm!("vmxon $1; pushfq; popq $0"
                 : "=r"(rflags)
                 : "m"(vmxon_region_addr)
                 : "rflags");
            rflags
        };

        // FIXME: this leaks the page on error
        error::check_vm_insruction(rflags, "Failed to enable vmx".into())?;
        Ok(Vmx {
            vmxon_region: vmxon_region,
        })
    }

    pub fn disable(self, alloc: &mut impl FrameDeallocator<Size4KiB>) -> Result<()> {
        //TODO: this should panic when done from a different core than it
        //      was originally activated from
        let rflags = unsafe {
            let rflags: u64;
            asm!("vmxoff; pushfq; popq $0"
                 : "=r"(rflags)
                 :
                 : "rflags");
            rflags
        };

        error::check_vm_insruction(rflags, "Failed to disable vmx".into())?;
        alloc.deallocate_frame(self.vmxon_region);
        Ok(())
    }

    pub fn revision() -> u32 {
        //FIXME: this is currently returning very strange values
        // see https://software.intel.com/en-us/forums/virtualization-software-development/topic/293175
        use crate::registers::MSR_IA32_VMX_BASIC;
        use x86_64::registers::model_specific::Msr;
        let vmx_basic = Msr::new(MSR_IA32_VMX_BASIC);
        unsafe { vmx_basic.read() as u32 }
    }
}
