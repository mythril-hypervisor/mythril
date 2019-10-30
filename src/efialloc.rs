use crate::error::{self, Error, Result};
use crate::memory::PhysFrame;
use uefi::prelude::ResultExt;
use uefi::table::boot::{AllocateType, BootServices, MemoryType};
use x86::bits64::paging::PAddr;

pub trait FrameAllocator {
    fn allocate_frame(&mut self) -> Result<PhysFrame>;
    fn deallocate_frame(&mut self, frame: PhysFrame) -> Result<()>;
}

pub struct EfiAllocator<'a> {
    bt: &'a BootServices,
}

impl<'a> EfiAllocator<'a> {
    pub fn new(bt: &'a BootServices) -> Self {
        EfiAllocator { bt: bt }
    }
}

impl<'a> FrameAllocator for EfiAllocator<'a> {
    fn allocate_frame(&mut self) -> Result<PhysFrame> {
        let ty = AllocateType::AnyPages;
        let mem_ty = MemoryType::LOADER_DATA;
        let pg = self
            .bt
            .allocate_pages(ty, mem_ty, 1)
            .log_warning()
            .map_err(|_| Error::Uefi("EfiAllocator failed to allocate frame".into()))?;

        //FIXME: For now, zero every frame we allocate
        let ptr = pg as *mut u8;
        unsafe {
            core::ptr::write_bytes(ptr, 0, 4096);
        }

        PhysFrame::from_start_address(PAddr::from(pg))
    }

    fn deallocate_frame(&mut self, frame: PhysFrame) -> Result<()> {
        self.bt
            .free_pages(frame.start_address().as_u64(), 1)
            .log_warning()
            .map_err(|_| Error::Uefi("EfiAllocator failed to deallocate frame".into()))
    }
}
