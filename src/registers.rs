// This module is essentially just things that should be upstreamed
// to the x86_64 crate
use x86_64::registers::model_specific::Msr;
use x86_64::VirtAddr;

pub const MSR_IA32_VMX_BASIC: u32 = 0x480;
pub const MSR_IA32_VMX_PINBASED_CTLS: u32 = 0x481;
pub const MSR_IA32_VMX_PROCBASED_CTLS: u32 = 0x482;
pub const MSR_IA32_VMX_EXIT_CTLS: u32 = 0x483;
pub const MSR_IA32_VMX_ENTRY_CTLS: u32 = 0x484;
pub const MSR_IA32_VMX_MISC: u32 = 0x485;
pub const MSR_IA32_VMX_CR0_FIXED0: u32 = 0x486;
pub const MSR_IA32_VMX_CR0_FIXED1: u32 = 0x487;
pub const MSR_IA32_VMX_CR4_FIXED0: u32 = 0x488;
pub const MSR_IA32_VMX_CR4_FIXED1: u32 = 0x489;
pub const MSR_IA32_VMX_VMCS_ENUM: u32 = 0x48a;
pub const MSR_IA32_VMX_PROCBASED_CTLS2: u32 = 0x48b;
pub const MSR_IA32_VMX_EPT_VPID_CAP: u32 = 0x48c;
pub const MSR_IA32_VMX_TRUE_PINBASED_CTLS: u32 = 0x48d;
pub const MSR_IA32_VMX_TRUE_PROCBASED_CTLS: u32 = 0x48e;
pub const MSR_IA32_VMX_TRUE_EXIT_CTLS: u32 = 0x48f;
pub const MSR_IA32_VMX_TRUE_ENTRY_CTLS: u32 = 0x490;
pub const MSR_IA32_VMX_VMFUNC: u32 = 0x491;

#[repr(C)]
#[repr(packed)]
struct IdtInfo {
    limit: u16,
    base_addr: u64,
}

pub struct IdtrBase;
impl IdtrBase {
    pub fn read() -> VirtAddr {
        let addr = unsafe {
            let mut info = IdtInfo {
                limit: 0,
                base_addr: 0,
            };
            asm!("sidt ($0)"
                 :
                 : "r"(&mut info)
                 : "memory"
                 : "volatile");
            info.base_addr
        };
        VirtAddr::new(addr)
    }
}

#[repr(C)]
#[repr(packed)]
struct GdtInfo {
    size: u16,
    offset: u64,
}

pub struct GdtrBase;
impl GdtrBase {
    pub fn read() -> VirtAddr {
        let addr = unsafe {
            let mut info = GdtInfo { size: 0, offset: 0 };
            asm!("sgdtq ($0)"
                 :
                 : "r"(&mut info)
                 : "memory"
                 : "volatile");
            info.offset
        };
        VirtAddr::new(addr)
    }
}

pub struct Cr4;
impl Cr4 {
    //TODO: this should return a Cr4Flags
    pub fn read() -> u64 {
        let mut current_cr4: u64;
        unsafe {
            asm!("movq %cr4, %rax;"
                 : "={rax}"(current_cr4)
                 ::: "volatile");
        }
        current_cr4
    }
}
