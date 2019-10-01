use uefi::table::boot::{AllocateType, BootServices, MemoryType};
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::PageSize;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};
use x86_64::PhysAddr;

pub struct EfiAllocator<'a> {
    bt: &'a BootServices,
}

impl<'a> EfiAllocator<'a> {
    pub fn new(bt: &'a BootServices) -> Self {
        EfiAllocator { bt: bt }
    }
}

unsafe impl<'a> FrameAllocator<Size4KiB> for EfiAllocator<'a> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let ty = AllocateType::AnyPages;
        let mem_ty = MemoryType::LOADER_DATA;
        let pg = self
            .bt
            .allocate_pages(ty, mem_ty, 1)
            .ok()?
            .expect("EfiAllocator failed to allocate page");

        //FIXME: For now, zero every frame we allocate
        let ptr = pg as *mut u8;
        unsafe {
            core::ptr::write_bytes(ptr, 0, Size4KiB::SIZE as usize);
        }

        PhysFrame::from_start_address(PhysAddr::new(pg)).ok()
    }
}

impl<'a> FrameDeallocator<Size4KiB> for EfiAllocator<'a> {
    fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let _ = self
            .bt
            .free_pages(frame.start_address().as_u64(), 1)
            .expect("EfiAllocator failed to deallocate frame");
    }
}
