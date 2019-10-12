use crate::error::{self, Error, Result};
use crate::vm;
use crate::vmcs;
use bitflags::bitflags;
use core::convert::TryFrom;
use num_enum::TryFromPrimitive;
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

struct ExitReason(u32);
bitflags! {
    pub struct ExitReasonFields: u64 {
        const ENCLAVE_MODE =        1 << 27;
        const PENDING_MTF_EXIT =    1 << 28;
        const EXIT_FROM_ROOT =      1 << 29;
        const VM_ENTRY_FAIL =       1 << 31;
    }
}

impl ExitReason {
    fn from_active_vmcs(vmcs: &mut vmcs::ActiveVmcs) -> Result<Self> {
        let reason = vmcs.read_field(vmcs::VmcsField::VmExitReason)?;
        info!("Reason: 0x{:x}", reason);
        Ok(ExitReason(reason as u32))
    }

    fn reason(&self) -> BasicExitReason {
        BasicExitReason::try_from(self.0 & 0x7fff).unwrap_or(BasicExitReason::UnknownExitReason)
    }
}

#[no_mangle]
pub extern "C" fn vmexit_handler() {
    let vm = unsafe { vm::VMS.get_mut().as_mut().expect("Failed to get VM") };

    let reason = ExitReason::from_active_vmcs(&mut vm.vmcs).expect("Failed to get vm reason");

    info!("reached vmexit handler: {:?}", reason.reason());

    let rip = vm
        .vmcs
        .read_field(vmcs::VmcsField::GuestRip)
        .expect("Failed to read guest rip");

    let es_ar = vm
        .vmcs
        .read_field(vmcs::VmcsField::GuestEsArBytes)
        .expect("Failed to read guest es ar");
    info!("Resume at 0x{:x}, es_ar 0x{:x}", rip, es_ar);
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
