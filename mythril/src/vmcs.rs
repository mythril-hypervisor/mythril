use crate::error::{self, Error, Result};
use crate::memory::Raw4kPage;
use crate::vmx;
use alloc::boxed::Box;
use bitflags::bitflags;
use core::fmt;
use num_enum::TryFromPrimitive;
use x86::msr::rdmsr;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum VmcsField {
    VirtualProcessorId = 0x00000000,
    PostedIntrNv = 0x00000002,
    GuestEsSelector = 0x00000800,
    GuestCsSelector = 0x00000802,
    GuestSsSelector = 0x00000804,
    GuestDsSelector = 0x00000806,
    GuestFsSelector = 0x00000808,
    GuestGsSelector = 0x0000080a,
    GuestLdtrSelector = 0x0000080c,
    GuestTrSelector = 0x0000080e,
    GuestIntrStatus = 0x00000810,
    GuestPmlIndex = 0x00000812,
    HostEsSelector = 0x00000c00,
    HostCsSelector = 0x00000c02,
    HostSsSelector = 0x00000c04,
    HostDsSelector = 0x00000c06,
    HostFsSelector = 0x00000c08,
    HostGsSelector = 0x00000c0a,
    HostTrSelector = 0x00000c0c,
    IoBitmapA = 0x00002000,
    IoBitmapAHigh = 0x00002001,
    IoBitmapB = 0x00002002,
    IoBitmapBHigh = 0x00002003,
    MsrBitmap = 0x00002004,
    MsrBitmapHigh = 0x00002005,
    VmExitMsrStoreAddr = 0x00002006,
    VmExitMsrStoreAddrHigh = 0x00002007,
    VmExitMsrLoadAddr = 0x00002008,
    VmExitMsrLoadAddrHigh = 0x00002009,
    VmEntryMsrLoadAddr = 0x0000200a,
    VmEntryMsrLoadAddrHigh = 0x0000200b,
    PmlAddress = 0x0000200e,
    PmlAddressHigh = 0x0000200f,
    TscOffset = 0x00002010,
    TscOffsetHigh = 0x00002011,
    VirtualApicPageAddr = 0x00002012,
    VirtualApicPageAddrHigh = 0x00002013,
    ApicAccessAddr = 0x00002014,
    ApicAccessAddrHigh = 0x00002015,
    PostedIntrDescAddr = 0x00002016,
    PostedIntrDescAddrHigh = 0x00002017,
    EptPointer = 0x0000201a,
    EptPointerHigh = 0x0000201b,
    EoiExitBitmap0 = 0x0000201c,
    EoiExitBitmap0High = 0x0000201d,
    EoiExitBitmap1 = 0x0000201e,
    EoiExitBitmap1High = 0x0000201f,
    EoiExitBitmap2 = 0x00002020,
    EoiExitBitmap2High = 0x00002021,
    EoiExitBitmap3 = 0x00002022,
    EoiExitBitmap3High = 0x00002023,
    VmreadBitmap = 0x00002026,
    VmreadBitmapHigh = 0x00002027,
    VmwriteBitmap = 0x00002028,
    VmwriteBitmapHigh = 0x00002029,
    XssExitBitmap = 0x0000202C,
    XssExitBitmapHigh = 0x0000202D,
    TscMultiplier = 0x00002032,
    TscMultiplierHigh = 0x00002033,
    GuestPhysicalAddress = 0x00002400,
    GuestPhysicalAddressHigh = 0x00002401,
    VmcsLinkPointer = 0x00002800,
    VmcsLinkPointerHigh = 0x00002801,
    GuestIa32Debugctl = 0x00002802,
    GuestIa32DebugctlHigh = 0x00002803,
    GuestIa32Pat = 0x00002804,
    GuestIa32PatHigh = 0x00002805,
    GuestIa32Efer = 0x00002806,
    GuestIa32EferHigh = 0x00002807,
    GuestIa32PerfGlobalCtrl = 0x00002808,
    GuestIa32PerfGlobalCtrlHigh = 0x00002809,
    GuestPdptr0 = 0x0000280a,
    GuestPdptr0High = 0x0000280b,
    GuestPdptr1 = 0x0000280c,
    GuestPdptr1High = 0x0000280d,
    GuestPdptr2 = 0x0000280e,
    GuestPdptr2High = 0x0000280f,
    GuestPdptr3 = 0x00002810,
    GuestPdptr3High = 0x00002811,
    GuestBndcfgs = 0x00002812,
    GuestBndcfgsHigh = 0x00002813,
    HostIa32Pat = 0x00002c00,
    HostIa32PatHigh = 0x00002c01,
    HostIa32Efer = 0x00002c02,
    HostIa32EferHigh = 0x00002c03,
    HostIa32PerfGlobalCtrl = 0x00002c04,
    HostIa32PerfGlobalCtrlHigh = 0x00002c05,
    PinBasedVmExecControl = 0x00004000,
    CpuBasedVmExecControl = 0x00004002,
    ExceptionBitmap = 0x00004004,
    PageFaultErrorCodeMask = 0x00004006,
    PageFaultErrorCodeMatch = 0x00004008,
    Cr3TargetCount = 0x0000400a,
    VmExitControls = 0x0000400c,
    VmExitMsrStoreCount = 0x0000400e,
    VmExitMsrLoadCount = 0x00004010,
    VmEntryControls = 0x00004012,
    VmEntryMsrLoadCount = 0x00004014,
    VmEntryIntrInfoField = 0x00004016,
    VmEntryExceptionErrorCode = 0x00004018,
    VmEntryInstructionLen = 0x0000401a,
    TprThreshold = 0x0000401c,
    SecondaryVmExecControl = 0x0000401e,
    PleGap = 0x00004020,
    PleWindow = 0x00004022,
    VmInstructionError = 0x00004400,
    VmExitReason = 0x00004402,
    VmExitIntrInfo = 0x00004404,
    VmExitIntrErrorCode = 0x00004406,
    IdtVectoringInfoField = 0x00004408,
    IdtVectoringErrorCode = 0x0000440a,
    VmExitInstructionLen = 0x0000440c,
    VmxInstructionInfo = 0x0000440e,
    GuestEsLimit = 0x00004800,
    GuestCsLimit = 0x00004802,
    GuestSsLimit = 0x00004804,
    GuestDsLimit = 0x00004806,
    GuestFsLimit = 0x00004808,
    GuestGsLimit = 0x0000480a,
    GuestLdtrLimit = 0x0000480c,
    GuestTrLimit = 0x0000480e,
    GuestGdtrLimit = 0x00004810,
    GuestIdtrLimit = 0x00004812,
    GuestEsArBytes = 0x00004814,
    GuestCsArBytes = 0x00004816,
    GuestSsArBytes = 0x00004818,
    GuestDsArBytes = 0x0000481a,
    GuestFsArBytes = 0x0000481c,
    GuestGsArBytes = 0x0000481e,
    GuestLdtrArBytes = 0x00004820,
    GuestTrArBytes = 0x00004822,
    GuestInterruptibilityInfo = 0x00004824,
    GuestActivityState = 0x00004826,
    GuestSysenterCs = 0x0000482A,
    VmxPreemptionTimerValue = 0x0000482E,
    HostIa32SysenterCs = 0x00004c00,
    Cr0GuestHostMask = 0x00006000,
    Cr4GuestHostMask = 0x00006002,
    Cr0ReadShadow = 0x00006004,
    Cr4ReadShadow = 0x00006006,
    Cr3TargetValue0 = 0x00006008,
    Cr3TargetValue1 = 0x0000600a,
    Cr3TargetValue2 = 0x0000600c,
    Cr3TargetValue3 = 0x0000600e,
    ExitQualification = 0x00006400,
    GuestLinearAddress = 0x0000640a,
    GuestCr0 = 0x00006800,
    GuestCr3 = 0x00006802,
    GuestCr4 = 0x00006804,
    GuestEsBase = 0x00006806,
    GuestCsBase = 0x00006808,
    GuestSsBase = 0x0000680a,
    GuestDsBase = 0x0000680c,
    GuestFsBase = 0x0000680e,
    GuestGsBase = 0x00006810,
    GuestLdtrBase = 0x00006812,
    GuestTrBase = 0x00006814,
    GuestGdtrBase = 0x00006816,
    GuestIdtrBase = 0x00006818,
    GuestDr7 = 0x0000681a,
    GuestRsp = 0x0000681c,
    GuestRip = 0x0000681e,
    GuestRflags = 0x00006820,
    GuestPendingDbgExceptions = 0x00006822,
    GuestSysenterEsp = 0x00006824,
    GuestSysenterEip = 0x00006826,
    HostCr0 = 0x00006c00,
    HostCr3 = 0x00006c02,
    HostCr4 = 0x00006c04,
    HostFsBase = 0x00006c06,
    HostGsBase = 0x00006c08,
    HostTrBase = 0x00006c0a,
    HostGdtrBase = 0x00006c0c,
    HostIdtrBase = 0x00006c0e,
    HostIa32SysenterEsp = 0x00006c10,
    HostIa32SysenterEip = 0x00006c12,
    HostRsp = 0x00006c14,
    HostRip = 0x00006c16,
}

bitflags! {
    pub struct PinBasedCtrlFlags: u64 {
        const EXT_INTR_EXIT =        0x00000001;
        const NMI_EXITING =          0x00000008;
        const VIRTUAL_NMIS =         0x00000020;
        const PREEMPT_TIMER =        0x00000040;
        const POSTED_INTERRUPT =     0x00000080;
    }
}

bitflags! {
    pub struct CpuBasedCtrlFlags: u64 {
        const INTERRUPT_WINDOW_EXITING =    0x00000004;
        const USE_TSC_OFFSETING =           0x00000008;
        const HLT_EXITING =                 0x00000080;
        const INVLPG_EXITING =              0x00000200;
        const MWAIT_EXITING =               0x00000400;
        const RDPMC_EXITING =               0x00000800;
        const RDTSC_EXITING =               0x00001000;
        const CR3_LOAD_EXITING =            0x00008000;
        const CR3_STORE_EXITING =           0x00010000;
        const CR8_LOAD_EXITING =            0x00080000;
        const CR8_STORE_EXITING =           0x00100000;
        const TPR_SHADOW =                  0x00200000;
        const VIRTUAL_NMI_PENDING =         0x00400000;
        const MOV_DR_EXITING =              0x00800000;
        const UNCOND_IO_EXITING =           0x01000000;
        const ACTIVATE_IO_BITMAP =          0x02000000;
        const MONITOR_TRAP_FLAG =           0x08000000;
        const ACTIVATE_MSR_BITMAP =         0x10000000;
        const MONITOR_EXITING =             0x20000000;
        const PAUSE_EXITING =               0x40000000;
        const ACTIVATE_SECONDARY_CONTROLS = 0x80000000;
    }
}

bitflags! {
    pub struct VmExitCtrlFlags: u64 {
        const SAVE_DEBUG_CNTRLS =      0x00000004;
        const IA32E_MODE =             0x00000200;
        const LOAD_PERF_GLOBAL_CTRL =  0x00001000;
        const ACK_INTR_ON_EXIT =       0x00008000;
        const SAVE_GUEST_PAT =         0x00040000;
        const LOAD_HOST_PAT =          0x00080000;
        const SAVE_GUEST_EFER =        0x00100000;
        const LOAD_HOST_EFER =         0x00200000;
        const SAVE_PREEMPT_TIMER =     0x00400000;
        const CLEAR_BNDCFGS =          0x00800000;
    }
}

bitflags! {
    pub struct VmEntryCtrlFlags: u64 {
        const IA32E_MODE =            0x00000200;
        const SMM =                   0x00000400;
        const DEACT_DUAL_MONITOR =    0x00000800;
        const LOAD_PERF_GLOBAL_CTRL = 0x00002000;
        const LOAD_GUEST_PAT =        0x00004000;
        const LOAD_GUEST_EFER =       0x00008000;
        const LOAD_BNDCFGS =          0x00010000;
    }
}

bitflags! {
    pub struct SecondaryExecFlags: u64 {
        const VIRTUALIZE_APIC_ACCESSES = 0x00000001;
        const ENABLE_EPT =               0x00000002;
        const DESCRIPTOR_TABLE_EXITING = 0x00000004;
        const ENABLE_RDTSCP =            0x00000008;
        const VIRTUALIZE_X2APIC_MODE =   0x00000010;
        const ENABLE_VPID =              0x00000020;
        const WBINVD_EXITING =           0x00000040;
        const UNRESTRICTED_GUEST =       0x00000080;
        const APIC_REGISTER_VIRT =       0x00000100;
        const VIRTUAL_INTR_DELIVERY =    0x00000200;
        const PAUSE_LOOP_EXITING =       0x00000400;
        const ENABLE_INVPCID =           0x00001000;
        const ENABLE_VM_FUNCTIONS =      0x00002000;
        const ENABLE_VMCS_SHADOWING =    0x00004000;
        const ENABLE_PML =               0x00020000;
        const ENABLE_VIRT_EXCEPTIONS =   0x00040000;
        const XSAVES =                   0x00100000;
        const TSC_SCALING =              0x02000000;
    }
}

bitflags! {
    pub struct InterruptibilityState: u64 {
        const STI_BLOCKING      = 0x00000001;
        const MOV_SS_BLOCKING   = 0x00000002;
        const SMI_BLOCKING      = 0x00000004;
        const NMI_BLOCKING      = 0x00000008;
        const ENCLAVE_INTERRUPT = 0x00000010;
    }
}

#[derive(Copy, Clone, TryFromPrimitive)]
#[repr(u32)]
pub enum ActivityState {
    Active = 0,
    Hlt = 1,
    Shutdown = 2,
    WaitForSipi = 3,
}

fn vmcs_write_with_fixed(
    field: VmcsField,
    value: u64,
    msr: u32,
) -> Result<u64> {
    let mut required_value = value;
    let fixed = unsafe { rdmsr(msr) };
    let low = fixed & 0x00000000ffffffff;
    let high = fixed >> 32;

    required_value &= high; /* bit == 0 in high word ==> must be zero */
    required_value |= low; /* bit == 1 in low word  ==> must be one  */

    if (value & !required_value) != 0 {
        return Err(Error::Vmcs(format!(
            "Requested field ({:?}) bit not allowed by MSR (requested=0x{:x} forbidden=0x{:x} required=0x{:x} res=0x{:x})",
            field,
            value,
            high,
            low,
            required_value
        )));
    }

    vmcs_write(field, required_value)?;
    Ok(required_value)
}

fn vmcs_write(field: VmcsField, value: u64) -> Result<()> {
    let rflags = unsafe {
        let rflags: u64;
        asm!(
            "vmwrite rax, rdx",
            "pushf",
            "pop {}",
            lateout(reg) rflags,
            in("rdx") value,
            in("rax") field as u64,
        );
        rflags
    };

    error::check_vm_insruction(
        rflags,
        format!("Failed to write 0x{:x} to field {:?}", value, field),
    )
}

fn vmcs_read(field: VmcsField) -> Result<u64> {
    let value = unsafe {
        let value: u64;
        asm!(
            "vmread rax, rdx",
            out("rax") value,
            in("rdx") field as u64
        );
        value
    };

    Ok(value)
}

fn vmcs_activate(vmcs: &mut Vmcs, _vmx: &vmx::Vmx) -> Result<()> {
    let revision_id = vmx::Vmx::revision();
    let vmcs_region_addr = &mut *vmcs.frame as *mut Raw4kPage;
    let region_revision = vmcs_region_addr as *mut u32;
    unsafe {
        *region_revision = revision_id;
    }
    let rflags = unsafe {
        let rflags: u64;
        asm!(
            "vmptrld [{}]",
            "pushf",
            "pop {}",
            in(reg) &vmcs_region_addr,
            lateout(reg) rflags
        );
        rflags
    };

    error::check_vm_insruction(rflags, "Failed to activate VMCS".into())
}

fn vmcs_clear(vmcs_page: &mut Raw4kPage) -> Result<()> {
    let rflags = unsafe {
        let rflags: u64;
        asm!(
            "vmclear [{}]",
            "pushf",
            "pop {}",
            in(reg) &(vmcs_page as *const _ as u64),
            lateout(reg) rflags
        );
        rflags
    };
    error::check_vm_insruction(rflags, "Failed to clear VMCS".into())
}

pub struct Vmcs {
    frame: Box<Raw4kPage>,
}

impl Vmcs {
    pub fn new() -> Result<Self> {
        Ok(Vmcs {
            frame: Box::new(Raw4kPage::default()),
        })
    }

    pub fn activate(self, vmx: vmx::Vmx) -> Result<ActiveVmcs> {
        ActiveVmcs::new(self, vmx)
    }

    pub fn with_active_vmcs<T>(
        &mut self,
        vmx: &mut vmx::Vmx,
        mut callback: impl FnMut(TemporaryActiveVmcs) -> Result<T>,
    ) -> Result<T> {
        (callback)(TemporaryActiveVmcs::new(self, vmx)?)
    }
}

pub struct ActiveVmcs {
    vmcs: Vmcs,
    pub vmx: vmx::Vmx,
}

impl ActiveVmcs {
    fn new(mut vmcs: Vmcs, vmx: vmx::Vmx) -> Result<Self> {
        vmcs_activate(&mut vmcs, &vmx)?;
        Ok(Self { vmcs, vmx })
    }

    pub fn read_field(&self, field: VmcsField) -> Result<u64> {
        vmcs_read(field)
    }

    pub fn write_field(&mut self, field: VmcsField, value: u64) -> Result<()> {
        vmcs_write(field, value)
    }

    pub fn write_with_fixed(
        &mut self,
        field: VmcsField,
        value: u64,
        msr: u32,
    ) -> Result<u64> {
        vmcs_write_with_fixed(field, value, msr)
    }

    pub fn deactivate(mut self) -> Result<(Vmcs, vmx::Vmx)> {
        vmcs_clear(&mut self.vmcs.frame)?;
        Ok((self.vmcs, self.vmx))
    }
}

impl fmt::Display for ActiveVmcs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let read_field =
            |field: VmcsField| -> core::result::Result<u64, fmt::Error> {
                self.read_field(field).map_err(|_| fmt::Error)
            };

        write!(f, "VMCS:\n")?;
        write!(f, " Guest State:\n")?;
        write!(
            f,
            "  CR0=0x{:x}(shadow=0x{:x}) ",
            read_field(VmcsField::GuestCr0)?,
            read_field(VmcsField::Cr0ReadShadow)?
        )?;
        write!(f, "CR3=0x{:x} ", read_field(VmcsField::GuestCr3)?)?;
        write!(
            f,
            "CR4=0x{:x}(shadow=0x{:x})\n",
            read_field(VmcsField::GuestCr4)?,
            read_field(VmcsField::Cr4ReadShadow)?
        )?;

        write!(f, "  EFER=0x{:x}\n", read_field(VmcsField::GuestIa32Efer)?)?;

        write!(f, "  RSP=0x{:x} ", read_field(VmcsField::GuestRsp)?)?;
        write!(f, "RIP=0x{:x}\n", read_field(VmcsField::GuestRip)?)?;

        write!(f, "  RFLAGS=0x{:x} ", read_field(VmcsField::GuestRflags)?)?;
        write!(f, "DR7=0x{:x}\n", read_field(VmcsField::GuestDr7)?)?;

        // Guest selectors
        write!(f, "  CS: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestCsSelector)?,
            read_field(VmcsField::GuestCsBase)?,
            read_field(VmcsField::GuestCsLimit)?,
            read_field(VmcsField::GuestCsArBytes)?
        )?;
        write!(f, "  DS: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestDsSelector)?,
            read_field(VmcsField::GuestDsBase)?,
            read_field(VmcsField::GuestDsLimit)?,
            read_field(VmcsField::GuestDsArBytes)?
        )?;
        write!(f, "  SS: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestSsSelector)?,
            read_field(VmcsField::GuestSsBase)?,
            read_field(VmcsField::GuestSsLimit)?,
            read_field(VmcsField::GuestSsArBytes)?
        )?;
        write!(f, "  ES: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestEsSelector)?,
            read_field(VmcsField::GuestEsBase)?,
            read_field(VmcsField::GuestEsLimit)?,
            read_field(VmcsField::GuestEsArBytes)?
        )?;
        write!(f, "  FS: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestFsSelector)?,
            read_field(VmcsField::GuestFsBase)?,
            read_field(VmcsField::GuestFsLimit)?,
            read_field(VmcsField::GuestFsArBytes)?
        )?;
        write!(f, "  GS: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestGsSelector)?,
            read_field(VmcsField::GuestGsBase)?,
            read_field(VmcsField::GuestGsLimit)?,
            read_field(VmcsField::GuestGsArBytes)?
        )?;
        write!(f, "  LDTR: ")?;
        write!(
            f,
            "selector=0x{:x} base=0x{:x} limit=0x{:x} ar=0x{:x}\n",
            read_field(VmcsField::GuestLdtrSelector)?,
            read_field(VmcsField::GuestLdtrBase)?,
            read_field(VmcsField::GuestLdtrLimit)?,
            read_field(VmcsField::GuestLdtrArBytes)?
        )?;
        write!(f, "  GDTR: ")?;
        write!(
            f,
            "base=0x{:x} limit=0x{:x}\n",
            read_field(VmcsField::GuestGdtrBase)?,
            read_field(VmcsField::GuestGdtrLimit)?
        )?;
        write!(f, "  IDTR: ")?;
        write!(
            f,
            "base=0x{:x} limit=0x{:x}\n",
            read_field(VmcsField::GuestIdtrBase)?,
            read_field(VmcsField::GuestIdtrLimit)?
        )?;
        Ok(())
    }
}

pub struct TemporaryActiveVmcs<'a> {
    vmcs: &'a mut Vmcs,
    pub vmx: &'a mut vmx::Vmx,
}

impl<'a> TemporaryActiveVmcs<'a> {
    fn new(vmcs: &'a mut Vmcs, vmx: &'a mut vmx::Vmx) -> Result<Self> {
        vmcs_activate(vmcs, vmx)?;
        Ok(Self { vmcs, vmx: vmx })
    }

    pub fn read_field(&mut self, field: VmcsField) -> Result<u64> {
        vmcs_read(field)
    }

    pub fn write_field(&mut self, field: VmcsField, value: u64) -> Result<()> {
        vmcs_write(field, value)
    }

    pub fn write_with_fixed(
        &mut self,
        field: VmcsField,
        value: u64,
        msr: u32,
    ) -> Result<u64> {
        vmcs_write_with_fixed(field, value, msr)
    }
}

impl<'a> Drop for TemporaryActiveVmcs<'a> {
    fn drop(&mut self) {
        vmcs_clear(&mut self.vmcs.frame)
            .expect("Failed to clear TemporaryActiveVmcs");
    }
}
