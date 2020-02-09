use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use linked_list_allocator::{self, LockedHeap};

pub enum Allocator {
    Unavailable,
    Available(MultibootAllocator),
}

impl Allocator {
    pub unsafe fn allocate_from(start: u64, end: u64) {
        match ALLOCATOR {
            Allocator::Unavailable => {
                ALLOCATOR = Allocator::Available(MultibootAllocator::new(start, end));
            }
            _ => panic!("Allocator has already been initialized"),
        }
    }
}

struct MultibootAllocator(LockedHeap);
impl MultibootAllocator {
    fn new(start: u64, end: u64) -> Self {
        Self(unsafe { LockedHeap::new(start as usize, end as usize) })
    }
}

unsafe impl GlobalAlloc for MultibootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0.dealloc(ptr, layout)
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self {
            Allocator::Unavailable => ptr::null_mut(),
            Allocator::Available(alloc) => alloc.alloc(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match self {
            Allocator::Unavailable => (),
            Allocator::Available(alloc) => alloc.dealloc(ptr, layout),
        }
    }
}

#[global_allocator]
static mut ALLOCATOR: Allocator = Allocator::Unavailable;
