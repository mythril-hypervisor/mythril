use crate::error::{Error, Result};
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::PhysAddr;

struct EptPageTable;
struct EptPageTableEntry;


struct GuestPhysAddr(PhysAddr);
struct GuestFame;


fn map_guest_memory(guest_frame: GuestFame, host_frame: PhysFrame<Size4KiB>) -> Result<()> {
    Ok(())
}
