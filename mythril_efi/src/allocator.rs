use core::ptr::{self, NonNull};
use slab_allocator::{self, LockedHeap};
use spin::Mutex;
use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use uefi::prelude::*;
use uefi::table::boot::{BootServices, MemoryType, MemoryMapIter};

pub struct EarlyAllocator {
    boot_services: &'static BootServices
}
impl EarlyAllocator {
    pub fn new(boot_services: &BootServices) -> EarlyAllocator {
        Self {
            // Safe because we will not use the efi boot services after
            // we exit boot services. Necessary because the global allocator
            // must have static lifetime
            boot_services: unsafe {
                core::mem::transmute(boot_services)
            }
        }
    }
}

pub type LateAllocator = LockedHeap;

pub enum Allocator {
    Unavailable,
    Early(EarlyAllocator),
    Late(LateAllocator)
}

impl Allocator {
    pub unsafe fn init(boot_services: &BootServices) {
        match ALLOCATOR {
            Allocator::Unavailable => {
                ALLOCATOR = Allocator::Early(
                    EarlyAllocator::new(boot_services));
            },
            _ => panic!("Allocator has already been initialized")
        }
    }

    //TODO: should this take a specific descriptor?
    pub unsafe fn allocate_from<'a>(iter: MemoryMapIter<'a>) {
        let descriptor = iter.max_by(|left, right|{
            left.page_count.cmp(&right.page_count)
        }).unwrap();

        //TODO: check that this is within the descriptor range
        let addr = (descriptor.phys_start + 4096 - 1) & !(4096 - 1);

        // slab_allocator requires that the size be a multiple of the min heap size (8 pages)
        let size = (descriptor.page_count * 4096) & !(slab_allocator::MIN_HEAP_SIZE as u64 - 1);

        ALLOCATOR = Allocator::Late(
            LateAllocator::new(addr as usize, size as usize)
        );
    }
}

// Some of this impl is taken from https://github.com/rust-osdev/uefi-rs
unsafe impl GlobalAlloc for EarlyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mem_ty = MemoryType::LOADER_DATA;
        let size = layout.size();
        let align = layout.align();

        // TODO: add support for other alignments.
        if align > 8 {
            // Unsupported alignment for allocation, UEFI can only allocate 8-byte aligned addresses
            ptr::null_mut()
        } else {
            self.boot_services
                .allocate_pool(mem_ty, size)
                .warning_as_error()
                .unwrap_or(ptr::null_mut())
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        self.boot_services
            .free_pool(ptr)
            .warning_as_error()
            .unwrap()
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self {
            Allocator::Unavailable => ptr::null_mut(),
            Allocator::Early(alloc) => alloc.alloc(layout),
            Allocator::Late(alloc) => alloc.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match self {
            Allocator::Unavailable => (),
            Allocator::Early(alloc) => alloc.dealloc(ptr, layout),
            Allocator::Late(alloc) => alloc.dealloc(ptr, layout)
        }
    }
}

#[global_allocator]
static mut ALLOCATOR: Allocator = Allocator::Unavailable;
