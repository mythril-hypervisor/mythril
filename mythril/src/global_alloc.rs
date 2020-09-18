use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use linked_list_allocator::{self, LockedHeap};

pub enum Allocator {
    Unavailable,
    Available(LockedHeap),
}

impl Allocator {
    pub unsafe fn allocate_from(start: u64, end: u64) {
        match ALLOCATOR {
            Allocator::Unavailable => {
                ALLOCATOR = Allocator::Available(LockedHeap::new(
                    start as usize,
                    end as usize,
                ));
            }
            _ => panic!("Allocator has already been initialized"),
        }
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

// Tests use the std global allocator, but this symbol must still be defined
// for the library to compile, so define it here but do not set it as the
// global allocator.
#[cfg(test)]
static mut ALLOCATOR: Allocator = Allocator::Unavailable;

#[cfg(not(test))]
#[global_allocator]
static mut ALLOCATOR: Allocator = Allocator::Unavailable;
