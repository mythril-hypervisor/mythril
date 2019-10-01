use crate::error::{Error, Result};
use bitflags::bitflags;
use core::ops::{Index, IndexMut};
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::PageSize;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::page_table::PageTable;
use x86_64::structures::paging::page_table::PageTableFlags;
use x86_64::structures::paging::FrameAllocator;
use x86_64::ux;
use x86_64::PhysAddr;
use x86_64::VirtAddr;

#[repr(align(4096))]
pub struct EptTable<T> {
    entries: [T; 512],
}

impl<T> EptTable<T> {
    pub fn new(frame: &mut PhysFrame<Size4KiB>) -> Result<&mut Self> {
        unsafe { (frame.start_address().as_u64() as *mut Self).as_mut() }
            .ok_or(Error::AllocError("EptTable given invalid frame"))
    }
}

impl<T> Index<ux::u9> for EptTable<T> {
    type Output = T;

    fn index(&self, index: ux::u9) -> &Self::Output {
        &self.entries[u16::from(index) as usize]
    }
}

impl<T> IndexMut<ux::u9> for EptTable<T> {
    fn index_mut(&mut self, index: ux::u9) -> &mut Self::Output {
        &mut self.entries[u16::from(index) as usize]
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct EptTableEntry {
    entry: u64,
}

impl EptTableEntry {
    pub fn new() -> Self {
        Self { entry: 0 }
    }

    pub fn is_unused(&self) -> bool {
        self.entry == 0
    }

    pub fn set_unused(&mut self) {
        self.entry = 0;
    }

    pub fn flags(&self) -> EptTableFlags {
        EptTableFlags::from_bits_truncate(self.entry)
    }

    pub fn addr(&self) -> PhysAddr {
        PhysAddr::new(self.entry & 0x000fffff_fffff000)
    }

    pub fn frame(&self) -> Result<PhysFrame> {
        Ok(PhysFrame::containing_address(self.addr()))
    }

    pub fn set_addr(&mut self, addr: PhysAddr, flags: EptTableFlags) {
        assert!(addr.is_aligned(Size4KiB::SIZE));
        self.entry = (addr.as_u64()) | flags.bits();
    }

    pub fn set_flags(&mut self, flags: EptTableFlags) {
        self.entry = self.addr().as_u64() | flags.bits();
    }
}

#[derive(Copy, Clone)]
pub enum EptMemoryType {
    Uncacheable = 0,
    WriteCache = 1,
    WriteThrough = 4,
    WriteP = 5, // I can't find an expansion of the 'WP' in the spec
    WriteBack = 6,
}

#[derive(Clone)]
#[repr(transparent)]
pub struct EptPageTableEntry {
    entry: u64,
}

impl EptPageTableEntry {
    pub fn new() -> Self {
        Self { entry: 0 }
    }

    pub fn is_unused(&self) -> bool {
        self.entry == 0
    }

    pub fn set_unused(&mut self) {
        self.entry = 0;
    }

    pub fn flags(&self) -> EptTableFlags {
        EptTableFlags::from_bits_truncate(self.entry)
    }

    pub fn addr(&self) -> PhysAddr {
        PhysAddr::new(self.entry & 0x000fffff_fffff000)
    }

    pub fn frame(&self) -> Result<PhysFrame> {
        Ok(PhysFrame::containing_address(self.addr()))
    }

    pub fn set_addr(&mut self, addr: PhysAddr, flags: EptTableFlags) {
        assert!(addr.is_aligned(Size4KiB::SIZE));
        self.entry = (addr.as_u64()) | flags.bits();
    }

    pub fn set_flags(&mut self, flags: EptTableFlags) {
        self.entry = self.addr().as_u64() | flags.bits();
    }

    pub fn set_mem_type(&mut self, mem_type: EptMemoryType) {
        self.entry |= ((mem_type as u8) << 5) as u64;
    }

    //TODO: get mem type
}

bitflags! {
    //NOTE: Not all flags are valid for all tables
    pub struct EptTableFlags: u64 {
        const READ_ACCESS =          1 << 0;
        const WRITE_ACCESS =         1 << 1;
        const PRIV_EXEC_ACCESS =     1 << 2;
        const IGNORE_PAT =           1 << 6;
        const ACCESSED =             1 << 8;
        const DIRTY =                1 << 9;
        const USERMODE_EXEC_ACCESS = 1 << 10;
        const SUPRESS_VE =           1 << 63;
    }
}

pub type EptPml4Entry = EptTableEntry;
pub type EptPageDirectoryPointerEntry = EptTableEntry;
pub type EptPageDirectoryEntry = EptTableEntry;

pub type EptPml4Table = EptTable<EptPml4Entry>;
pub type EptPageDirectoryPointerTable = EptTable<EptPageDirectoryPointerEntry>;
pub type EptPageDirectory = EptTable<EptPageDirectoryEntry>;
pub type EptPageTable = EptTable<EptPageTableEntry>;

pub struct GuestPhysAddr(pub VirtAddr);

pub fn map_guest_memory(
    alloc: &mut impl FrameAllocator<Size4KiB>,
    guest_ept_base: &mut EptPml4Table,
    guest_addr: GuestPhysAddr,
    host_frame: PhysFrame<Size4KiB>,
) -> Result<()> {
    let default_flags = EptTableFlags::READ_ACCESS
        | EptTableFlags::WRITE_ACCESS
        | EptTableFlags::PRIV_EXEC_ACCESS
        | EptTableFlags::USERMODE_EXEC_ACCESS;

    let ept_pml4e = &mut guest_ept_base[guest_addr.0.p4_index()];
    if ept_pml4e.is_unused() {
        let ept_pdpt_frame = alloc
            .allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate pdpt"))?;
        ept_pml4e.set_addr(ept_pdpt_frame.start_address(), default_flags);
        info!("Allocated new EPT PDP Table")
    }

    let ept_pdpt = ept_pml4e.addr().as_u64() as *mut EptPageDirectoryPointerTable;
    let ept_pdpe = unsafe { &mut (*ept_pdpt)[guest_addr.0.p3_index()] };
    if ept_pdpe.is_unused() {
        let ept_pdt_frame = alloc
            .allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate pdt"))?;
        ept_pdpe.set_addr(ept_pdt_frame.start_address(), default_flags);
        info!("Allocated new PD Table")
    }

    let ept_pdt = ept_pdpe.addr().as_u64() as *mut EptPageDirectory;
    let ept_pde = unsafe { &mut (*ept_pdt)[guest_addr.0.p2_index()] };
    if ept_pde.is_unused() {
        let ept_pt_frame = alloc
            .allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate pt"))?;
        ept_pde.set_addr(ept_pt_frame.start_address(), default_flags);
        info!("Allocated new PT")
    }

    let ept_pt = ept_pde.addr().as_u64() as *mut EptPageTable;
    let ept_pte = unsafe { &mut (*ept_pt)[guest_addr.0.p1_index()] };

    if !ept_pte.is_unused() {
        return Err(Error::AllocError("Duplicate mapping"));
    }
    ept_pte.set_addr(
        host_frame.start_address(),
        EptTableFlags::READ_ACCESS
            | EptTableFlags::WRITE_ACCESS
            | EptTableFlags::PRIV_EXEC_ACCESS
            | EptTableFlags::USERMODE_EXEC_ACCESS
            | EptTableFlags::IGNORE_PAT,
    );
    ept_pte.set_mem_type(EptMemoryType::WriteBack);

    Ok(())
}
