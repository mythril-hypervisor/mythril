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
        self.set_flags(
            IdtFlags::PRESENT | IdtFlags::RING_0 | IdtFlags::INTERRUPT,
        );
        self.set_offset(8, func as usize);
    }
}

pub static mut IDT: [IdtEntry; 256] = [IdtEntry::new(); 256];

#[allow(dead_code)]
#[repr(packed)]
pub struct InterruptState {
    pub rip: usize,
    pub cs: usize,
    pub rflags: usize,
}

#[allow(dead_code)]
#[repr(packed)]
pub struct FaultState {
    pub error: usize,
    pub rip: usize,
    pub cs: usize,
    pub rflags: usize,
}

macro_rules! push_regs {
    () => {
        #[rustfmt::skip]
        asm!(
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "push rdi",
            "push rsi",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
        )
    };
}

macro_rules! pop_regs {
    () => {
        #[rustfmt::skip]
        asm!(
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rsi",
            "pop rdi",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
        )
    };
}

macro_rules! interrupt_fn_impl {
     ($name:ident, $stack:ident, $func:block, $type:ty) => {
         pub unsafe extern fn $name () {
             #[inline(never)]
             unsafe fn inner($stack: &$type) {
                 $func
             }

             push_regs!();

             let rbp: usize;
             asm!(
                "mov {}, rbp",
                out(reg) rbp,
                options(nomem, nostack)
             );

             // Plus usize to skip the old rpb value pushed in the preamble
             let stack = &*( (rbp + core::mem::size_of::<usize>()) as *const $type);
             inner(stack);

             pop_regs!();

             // Remove this stack frame before the iretq. This should work
             // whether the above 'rbp' local variable is stack allocated or not.
             asm!("mov rsp, rbp",
                  "pop rbp",
                  "iretq");
         }
     }
}

#[allow(unused_macros)]
macro_rules! interrupt_fn {
    ($name:ident, $stack:ident, $func:block) => {
        interrupt_fn_impl!(
            $name,
            $stack,
            $func,
            $crate::interrupt::idt::InterruptState
        );
    };
}

macro_rules! fault_fn {
    ($name:ident, $stack:ident, $func:block) => {
        interrupt_fn_impl!(
            $name,
            $stack,
            $func,
            $crate::interrupt::idt::FaultState
        );
    };
}

fault_fn!(nmi_handler, state, {
    panic!("Non-maskable interrupt (rip=0x{:x})", state.rip);
});

fault_fn!(protection_fault_handler, state, {
    panic!(
        "General protection fault handler (rip=0x{:x} error={:x})",
        state.rip, state.error
    );
});

fault_fn!(page_fault_handler, state, {
    panic!("Page fault handler (rip=0x{:x})", state.rip);
});

fault_fn!(zero_division_handler, state, {
    panic!("Divide by zero handler (rip=0x{:x})", state.rip);
});

pub unsafe fn init() {
    IDT[0].set_func(zero_division_handler);
    IDT[2].set_func(nmi_handler);
    IDT[13].set_func(protection_fault_handler);
    IDT[14].set_func(page_fault_handler);

    ap_init();
}

pub unsafe fn ap_init() {
    let idt = DescriptorTablePointer::new_from_slice(&IDT);
    lidt(&idt);
}
