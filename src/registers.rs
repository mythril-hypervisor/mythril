// This module is essentially just things that should be upstreamed
// to the x86_64 crate
use x86_64::registers::model_specific::Msr;
use x86_64::VirtAddr;

pub const IA32_VMX_CR0_FIXED0_MSR: u32 = 0x486;
pub const IA32_VMX_CR4_FIXED0_MSR: u32 = 0x488;

#[repr(C)]
#[repr(packed)]
struct IdtInfo {
    limit: u16,
    base_addr: u64
}

pub struct IdtrBase;
impl IdtrBase {
    pub fn read() -> VirtAddr {
        let addr = unsafe {
            let mut info = IdtInfo { limit: 0, base_addr: 0 };
            asm!("sidt ($0)"
                 :
                 : "a"(&mut info)
                 : "memory");
            info.base_addr
        };
        VirtAddr::new(addr)
    }
}

#[repr(C)]
#[repr(packed)]
struct GdtInfo {
    size: u16,
    offset: u64
}

pub struct GdtrBase;
impl GdtrBase {
    pub fn read() -> VirtAddr {
        let addr = unsafe {
            let mut info = GdtInfo { size: 0, offset: 0 };
            asm!("sgdt ($0)"
                 :
                 : "a"(&mut info)
                 : "memory");
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
