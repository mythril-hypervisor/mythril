use crate::error::Result;
use crate::memory::PhysFrame;

pub trait FrameAllocator {
    fn allocate_frame(&mut self) -> Result<PhysFrame>;
    fn deallocate_frame(&mut self, frame: PhysFrame) -> Result<()>;
}
