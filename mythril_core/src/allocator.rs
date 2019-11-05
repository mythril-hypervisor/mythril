use crate::error::Result;
use crate::memory::HostPhysFrame;

pub trait FrameAllocator {
    fn allocate_frame(&mut self) -> Result<HostPhysFrame>;
    fn deallocate_frame(&mut self, frame: HostPhysFrame) -> Result<()>;
}
