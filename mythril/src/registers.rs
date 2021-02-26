#[repr(C)]
#[repr(packed)]
struct IdtInfo {
    limit: u16,
    base_addr: u64,
}

pub struct IdtrBase;
impl IdtrBase {
    pub fn read() -> u64 {
        unsafe {
            let mut info = IdtInfo {
                limit: 0,
                base_addr: 0,
            };
            asm!(
                "sidt fword ptr [{0}]",
                in(reg) &mut info,
                options(nostack)
            );
            info.base_addr
        }
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
    pub fn read() -> u64 {
        unsafe {
            let mut info = GdtInfo { size: 0, offset: 0 };
            asm!(
                "sgdt fword ptr [{0}]",
                in(reg) &mut info,
                options(nostack)
            );
            info.offset
        }
    }
}
