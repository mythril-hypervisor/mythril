use bitflags::bitflags;
use x86::dtables::{lidt, DescriptorTablePointer};

bitflags! {
    pub struct IdtFlags: u8 {
        const PRESENT = 1 << 7;
        const RING_0 = 0 << 5;
        const RING_1 = 1 << 5;
        const RING_2 = 2 << 5;
        const RING_3 = 3 << 5;
        const SS = 1 << 4;
        const INTERRUPT = 0xE;
        const TRAP = 0xF;
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(packed)]
pub struct IdtEntry {
    offsetl: u16,
    selector: u16,
    zero: u8,
    attribute: u8,
    offsetm: u16,
    offseth: u32,
    zero2: u32,
}

impl IdtEntry {
    pub const fn new() -> IdtEntry {
        IdtEntry {
            offsetl: 0,
            selector: 0,
            zero: 0,
            attribute: 0,
            offsetm: 0,
            offseth: 0,
            zero2: 0,
        }
    }

    pub fn set_flags(&mut self, flags: IdtFlags) {
        self.attribute = flags.bits;
    }

    pub fn set_offset(&mut self, selector: u16, base: usize) {
        self.selector = selector;
        self.offsetl = base as u16;
        self.offsetm = (base >> 16) as u16;
        self.offseth = (base >> 32) as u32;
    }

    // A function to set the offset more easily
    pub fn set_func(&mut self, func: unsafe extern "C" fn()) {
        self.set_flags(IdtFlags::PRESENT | IdtFlags::RING_0 | IdtFlags::INTERRUPT);
        self.set_offset(8, func as usize);
    }
}

pub static mut IDT: [IdtEntry; 256] = [IdtEntry::new(); 256];

extern "C" fn nmi_handler() {
    panic!("Non-maskable interrupt");
}

extern "C" fn protection_fault_handler() {
    panic!("General protection fault handler");
}

extern "C" fn page_fault_handler() {
    panic!("Page fault handler");
}

extern "C" fn zero_division_handler() {
    panic!("Divide by zero");
}

pub unsafe fn init() {
    IDT[0].set_func(zero_division_handler);
    IDT[2].set_func(nmi_handler);
    IDT[13].set_func(protection_fault_handler);
    IDT[14].set_func(page_fault_handler);

    let idt = DescriptorTablePointer::new_from_slice(&IDT);
    lidt(&idt);
}
