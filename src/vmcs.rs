use crate::error::{Error, Result};
use crate::vmx;
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};
use x86_64::PhysAddr;

#[allow(dead_code)]
pub enum VmcsField {
    VirtualProcessorId            = 0x00000000,
    PostedIntrNv                  = 0x00000002,
    GuestEsSelector               = 0x00000800,
    GuestCsSelector               = 0x00000802,
    GuestSsSelector               = 0x00000804,
    GuestDsSelector               = 0x00000806,
    GuestFsSelector               = 0x00000808,
    GuestGsSelector               = 0x0000080a,
    GuestLdtrSelector             = 0x0000080c,
    GuestTrSelector               = 0x0000080e,
    GuestIntrStatus               = 0x00000810,
    GuestPmlIndex                 = 0x00000812,
    HostEsSelector                = 0x00000c00,
    HostCsSelector                = 0x00000c02,
    HostSsSelector                = 0x00000c04,
    HostDsSelector                = 0x00000c06,
    HostFsSelector                = 0x00000c08,
    HostGsSelector                = 0x00000c0a,
    HostTrSelector                = 0x00000c0c,
    IoBitmapA                     = 0x00002000,
    IoBitmapAHigh                 = 0x00002001,
    IoBitmapB                     = 0x00002002,
    IoBitmapBHigh                 = 0x00002003,
    MsrBitmap                     = 0x00002004,
    MsrBitmapHigh                 = 0x00002005,
    VmExitMsrStoreAddr            = 0x00002006,
    VmExitMsrStoreAddrHigh        = 0x00002007,
    VmExitMsrLoadAddr             = 0x00002008,
    VmExitMsrLoadAddrHigh         = 0x00002009,
    VmEntryMsrLoadAddr            = 0x0000200a,
    VmEntryMsrLoadAddrHigh        = 0x0000200b,
    PmlAddress                    = 0x0000200e,
    PmlAddressHigh                = 0x0000200f,
    TscOffset                     = 0x00002010,
    TscOffsetHigh                 = 0x00002011,
    VirtualApicPageAddr           = 0x00002012,
    VirtualApicPageAddrHigh       = 0x00002013,
    ApicAccessAddr                = 0x00002014,
    ApicAccessAddrHigh            = 0x00002015,
    PostedIntrDescAddr            = 0x00002016,
    PostedIntrDescAddrHigh        = 0x00002017,
    EptPointer                    = 0x0000201a,
    EptPointerHigh                = 0x0000201b,
    EoiExitBitmap0                = 0x0000201c,
    EoiExitBitmap0High            = 0x0000201d,
    EoiExitBitmap1                = 0x0000201e,
    EoiExitBitmap1High            = 0x0000201f,
    EoiExitBitmap2                = 0x00002020,
    EoiExitBitmap2High            = 0x00002021,
    EoiExitBitmap3                = 0x00002022,
    EoiExitBitmap3High            = 0x00002023,
    VmreadBitmap                  = 0x00002026,
    VmreadBitmapHigh              = 0x00002027,
    VmwriteBitmap                 = 0x00002028,
    VmwriteBitmapHigh             = 0x00002029,
    XssExitBitmap                 = 0x0000202C,
    XssExitBitmapHigh             = 0x0000202D,
    TscMultiplier                 = 0x00002032,
    TscMultiplierHigh             = 0x00002033,
    GuestPhysicalAddress          = 0x00002400,
    GuestPhysicalAddressHigh      = 0x00002401,
    VmcsLinkPointer               = 0x00002800,
    VmcsLinkPointerHigh           = 0x00002801,
    GuestIa32Debugctl             = 0x00002802,
    GuestIa32DebugctlHigh         = 0x00002803,
    GuestIa32Pat                  = 0x00002804,
    GuestIa32PatHigh              = 0x00002805,
    GuestIa32Efer                 = 0x00002806,
    GuestIa32EferHigh             = 0x00002807,
    GuestIa32PerfGlobalCtrl       = 0x00002808,
    GuestIa32PerfGlobalCtrlHigh   = 0x00002809,
    GuestPdptr0                   = 0x0000280a,
    GuestPdptr0High               = 0x0000280b,
    GuestPdptr1                   = 0x0000280c,
    GuestPdptr1High               = 0x0000280d,
    GuestPdptr2                   = 0x0000280e,
    GuestPdptr2High               = 0x0000280f,
    GuestPdptr3                   = 0x00002810,
    GuestPdptr3High               = 0x00002811,
    GuestBndcfgs                  = 0x00002812,
    GuestBndcfgsHigh              = 0x00002813,
    HostIa32Pat                   = 0x00002c00,
    HostIa32PatHigh               = 0x00002c01,
    HostIa32Efer                  = 0x00002c02,
    HostIa32EferHigh              = 0x00002c03,
    HostIa32PerfGlobalCtrl        = 0x00002c04,
    HostIa32PerfGlobalCtrlHigh    = 0x00002c05,
    PinBasedVmExecControl         = 0x00004000,
    CpuBasedVmExecControl         = 0x00004002,
    ExceptionBitmap               = 0x00004004,
    PageFaultErrorCodeMask        = 0x00004006,
    PageFaultErrorCodeMatch       = 0x00004008,
    Cr3TargetCount                = 0x0000400a,
    VmExitControls                = 0x0000400c,
    VmExitMsrStoreCount           = 0x0000400e,
    VmExitMsrLoadCount            = 0x00004010,
    VmEntryControls               = 0x00004012,
    VmEntryMsrLoadCount           = 0x00004014,
    VmEntryIntrInfoField          = 0x00004016,
    VmEntryExceptionErrorCode     = 0x00004018,
    VmEntryInstructionLen         = 0x0000401a,
    TprThreshold                  = 0x0000401c,
    SecondaryVmExecControl        = 0x0000401e,
    PleGap                        = 0x00004020,
    PleWindow                     = 0x00004022,
    VmInstructionError            = 0x00004400,
    VmExitReason                  = 0x00004402,
    VmExitIntrInfo                = 0x00004404,
    VmExitIntrErrorCode           = 0x00004406,
    IdtVectoringInfoField         = 0x00004408,
    IdtVectoringErrorCode         = 0x0000440a,
    VmExitInstructionLen          = 0x0000440c,
    VmxInstructionInfo            = 0x0000440e,
    GuestEsLimit                  = 0x00004800,
    GuestCsLimit                  = 0x00004802,
    GuestSsLimit                  = 0x00004804,
    GuestDsLimit                  = 0x00004806,
    GuestFsLimit                  = 0x00004808,
    GuestGsLimit                  = 0x0000480a,
    GuestLdtrLimit                = 0x0000480c,
    GuestTrLimit                  = 0x0000480e,
    GuestGdtrLimit                = 0x00004810,
    GuestIdtrLimit                = 0x00004812,
    GuestEsArBytes                = 0x00004814,
    GuestCsArBytes                = 0x00004816,
    GuestSsArBytes                = 0x00004818,
    GuestDsArBytes                = 0x0000481a,
    GuestFsArBytes                = 0x0000481c,
    GuestGsArBytes                = 0x0000481e,
    GuestLdtrArBytes              = 0x00004820,
    GuestTrArBytes                = 0x00004822,
    GuestInterruptibilityInfo     = 0x00004824,
    GuestActivityState            = 0x00004826,
    GuestSysenterCs               = 0x0000482A,
    VmxPreemptionTimerValue       = 0x0000482E,
    HostIa32SysenterCs            = 0x00004c00,
    Cr0GuestHostMask              = 0x00006000,
    Cr4GuestHostMask              = 0x00006002,
    Cr0ReadShadow                 = 0x00006004,
    Cr4ReadShadow                 = 0x00006006,
    Cr3TargetValue0               = 0x00006008,
    Cr3TargetValue1               = 0x0000600a,
    Cr3TargetValue2               = 0x0000600c,
    Cr3TargetValue3               = 0x0000600e,
    ExitQualification             = 0x00006400,
    GuestLinearAddress            = 0x0000640a,
    GuestCr0                      = 0x00006800,
    GuestCr3                      = 0x00006802,
    GuestCr4                      = 0x00006804,
    GuestEsBase                   = 0x00006806,
    GuestCsBase                   = 0x00006808,
    GuestSsBase                   = 0x0000680a,
    GuestDsBase                   = 0x0000680c,
    GuestFsBase                   = 0x0000680e,
    GuestGsBase                   = 0x00006810,
    GuestLdtrBase                 = 0x00006812,
    GuestTrBase                   = 0x00006814,
    GuestGdtrBase                 = 0x00006816,
    GuestIdtrBase                 = 0x00006818,
    GuestDr7                      = 0x0000681a,
    GuestRsp                      = 0x0000681c,
    GuestRip                      = 0x0000681e,
    GuestRflags                   = 0x00006820,
    GuestPendingDbgExceptions     = 0x00006822,
    GuestSysenterEsp              = 0x00006824,
    GuestSysenterEip              = 0x00006826,
    HostCr0                       = 0x00006c00,
    HostCr3                       = 0x00006c02,
    HostCr4                       = 0x00006c04,
    HostFsBase                    = 0x00006c06,
    HostGsBase                    = 0x00006c08,
    HostTrBase                    = 0x00006c0a,
    HostGdtrBase                  = 0x00006c0c,
    HostIdtrBase                  = 0x00006c0e,
    HostIa32SysenterEsp           = 0x00006c10,
    HostIa32SysenterEip           = 0x00006c12,
    HostRsp                       = 0x00006c14,
    HostRip                       = 0x00006c16,
}

fn vmcs_write(field: VmcsField, value: u64) -> Result<()> {
    let rflags = unsafe {
        let rflags: u64;
        asm!("vmwrite %rdx, %rax; pushfq; popq $0"
             : "=r"(rflags)
             :"{rdx}"(value), "{rax}"(field as u64)
             :"rflags"
             : "volatile");
        rflags
    };

    let rflags = rflags::RFlags::from_bits_truncate(rflags);

    if rflags.contains(RFlags::CARRY_FLAG) {
        Err(Error::VmFailInvalid)
    } else if rflags.contains(RFlags::ZERO_FLAG) {
        Err(Error::VmFailValid)
    } else {
        Ok(())
    }
}

fn vmcs_read(field: VmcsField) -> Result<u64> {
    let value = unsafe {
        let value: u64;
        asm!("vmread %rax, %rdx;"
             :"={rdx}"(value)
             :"{rax}"(field)
             :"rflags"
             : "volatile");
        value
    };

    Ok(value)
}

pub struct Vmcs {
    frame: PhysFrame<Size4KiB>,
}

impl Vmcs {
    pub fn new(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<Self> {
        let vmcs_region = alloc
            .allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate vmcs frame"))?;
        Ok(Vmcs { frame: vmcs_region })
    }

    pub fn activate(self, vmx: &mut vmx::Vmx) -> Result<ActiveVmcs> {
        let revision_id = vmx::Vmx::revision();
        let vmcs_region_addr = self.frame.start_address().as_u64();
        let region_revision = vmcs_region_addr as *mut u32;
        unsafe {
            *region_revision = revision_id;
        }

        let rflags = unsafe {
            let rflags: u64;
            asm!("vmptrld $1; pushfq; popq $0"
                 : "=r"(rflags)
                 : "m"(vmcs_region_addr)
                 : "rflags");
            rflags::RFlags::from_bits_truncate(rflags)
        };

        if rflags.contains(RFlags::CARRY_FLAG) {
            Err(Error::VmFailInvalid)
        } else if rflags.contains(RFlags::ZERO_FLAG) {
            Err(Error::VmFailValid)
        } else {
            Ok(ActiveVmcs {
                vmx: vmx,
                vmcs: self,
            })
        }
    }
}

pub struct ActiveVmcs<'a> {
    vmcs: Vmcs,
    vmx: &'a mut vmx::Vmx,
}

impl<'a> ActiveVmcs<'a> {
    pub fn read_field(&self, field: VmcsField) -> Result<u64> {
        vmcs_read(field)
    }

    pub fn write_field(&self, field: VmcsField, value: u64) -> Result<()> {
        vmcs_write(field, value)
    }

    pub fn deactivate(self) -> Vmcs {
        //TODO: should we set the VMCS to NULL?
        self.vmcs
    }
}

struct VmcsHost {
    stack: PhysAddr,
}

struct VmcsGuest {}

struct VmcsInfo {
    host: VmcsHost,
    guest: VmcsGuest,
    vpid: u64,
}
