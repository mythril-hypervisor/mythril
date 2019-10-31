use crate::efialloc::FrameAllocator;
use crate::error::{self, Error, Result};
use bitflags::bitflags;
use core::convert::TryFrom;
use core::ops::{Index, IndexMut};
use core::ptr::NonNull;
use derive_try_from_primitive::TryFromPrimitive;
use ux;
use x86::bits64::paging::{self, PAddr, VAddr};

pub struct PhysFrame(u64);
impl PhysFrame {
    pub fn from_start_address(addr: PAddr) -> Result<Self> {
        if !addr.is_base_page_aligned() {
            Err(Error::InvalidValue("Invalid start address for PhysFrame".into()))
        } else {
            Ok(PhysFrame(addr.as_u64()))
        }
    }

    pub fn start_address(&self) -> PAddr {
        PAddr::from(self.0)
    }
}

pub struct GuestAddressSpace {
    frame: PhysFrame,
    root: NonNull<EptPml4Table>,
}

impl GuestAddressSpace {
    pub fn new(alloc: &mut impl FrameAllocator) -> Result<Self> {
        let mut ept_pml4_frame = alloc
            .allocate_frame()
            .map_err(|_| Error::AllocError("Failed to allocate address space EPT root"))?;

        let ept_pml4 =
            unsafe { (ept_pml4_frame.start_address().as_u64() as *mut EptPml4Table).as_mut() }
                .unwrap();

        Ok(GuestAddressSpace {
            frame: ept_pml4_frame,
            root: NonNull::from(ept_pml4),
        })
    }

    pub fn map_frame(
        &mut self,
        alloc: &mut impl FrameAllocator,
        guest_addr: GuestPhysAddr,
        host_frame: PhysFrame,
        readonly: bool,
    ) -> Result<()> {
        map_guest_memory(
            alloc,
            unsafe { self.root.as_mut() },
            guest_addr,
            host_frame,
            readonly,
        )
    }

    pub fn eptp(&self) -> u64 {
        // //TODO: check available memory types
        self.frame.start_address().as_u64() | (4 - 1) << 3 | 6
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GuestPhysAddr(VAddr);
impl GuestPhysAddr {
    pub fn new(addr: u64) -> Self {
        Self(VAddr::from_u64(addr))
    }

    pub fn as_u64(&self) -> u64 {
        self.0.as_u64()
    }

    pub fn p1_index(&self) -> ux::u9 {
        ux::u9::new(paging::pt_index(self.0) as u16)
    }

    pub fn p2_index(&self) -> ux::u9 {
        ux::u9::new(paging::pd_index(self.0) as u16)
    }

    pub fn p3_index(&self) -> ux::u9 {
        ux::u9::new(paging::pdpt_index(self.0) as u16)
    }

    pub fn p4_index(&self) -> ux::u9 {
        ux::u9::new(paging::pml4_index(self.0) as u16)
    }
}

type GuestVirtAddr = VAddr;

#[repr(align(4096))]
pub struct EptTable<T> {
    entries: [T; 512],
}

impl<T> EptTable<T> {
    pub fn new(frame: &mut PhysFrame) -> Result<&mut Self> {
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

    pub fn addr(&self) -> PAddr {
        PAddr::from(self.entry & 0x000fffff_fffff000)
    }

    pub fn set_addr(&mut self, addr: PAddr, flags: EptTableFlags) {
        assert!(addr.is_base_page_aligned());
        self.entry = (addr.as_u64()) | flags.bits();
    }

    pub fn set_flags(&mut self, flags: EptTableFlags) {
        self.entry = self.addr().as_u64() | flags.bits();
    }
}

#[derive(Copy, Clone, TryFromPrimitive)]
#[repr(u8)]
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

    pub fn addr(&self) -> PAddr {
        PAddr::from(self.entry & 0x000fffff_fffff000)
    }

    pub fn mem_type(&self) -> EptMemoryType {
        EptMemoryType::try_from(((self.entry & (0b111 << 5)) >> 5) as u8)
            .expect("Invalid EPT memory type")
    }

    pub fn set_addr(&mut self, addr: PAddr, flags: EptTableFlags) {
        assert!(addr.is_base_page_aligned());
        self.entry = (addr.as_u64()) | flags.bits() | ((self.mem_type() as u64) << 5);
    }

    pub fn set_flags(&mut self, flags: EptTableFlags) {
        self.entry = self.addr().as_u64() | flags.bits() | ((self.mem_type() as u64) << 5);
    }

    pub fn set_mem_type(&mut self, mem_type: EptMemoryType) {
        self.entry &= !(0b111u64 << 5);
        self.entry |= ((mem_type as u8) << 5) as u64;
    }
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

fn map_guest_memory(
    alloc: &mut impl FrameAllocator,
    guest_ept_base: &mut EptPml4Table,
    guest_addr: GuestPhysAddr,
    host_frame: PhysFrame,
    readonly: bool,
) -> Result<()> {
    let default_flags = EptTableFlags::READ_ACCESS
        | EptTableFlags::WRITE_ACCESS
        | EptTableFlags::PRIV_EXEC_ACCESS
        | EptTableFlags::USERMODE_EXEC_ACCESS;

    let ept_pml4e = &mut guest_ept_base[guest_addr.p4_index()];
    if ept_pml4e.is_unused() {
        let ept_pdpt_frame = alloc
            .allocate_frame()
            .map_err(|_| Error::AllocError("Failed to allocate pdpt"))?;
        ept_pml4e.set_addr(ept_pdpt_frame.start_address(), default_flags);
    }

    let ept_pdpt = ept_pml4e.addr().as_u64() as *mut EptPageDirectoryPointerTable;
    let ept_pdpe = unsafe { &mut (*ept_pdpt)[guest_addr.p3_index()] };
    if ept_pdpe.is_unused() {
        let ept_pdt_frame = alloc
            .allocate_frame()
            .map_err(|_| Error::AllocError("Failed to allocate pdt"))?;
        ept_pdpe.set_addr(ept_pdt_frame.start_address(), default_flags);
    }

    let ept_pdt = ept_pdpe.addr().as_u64() as *mut EptPageDirectory;
    let ept_pde = unsafe { &mut (*ept_pdt)[guest_addr.p2_index()] };
    if ept_pde.is_unused() {
        let ept_pt_frame = alloc
            .allocate_frame()
            .map_err(|_| Error::AllocError("Failed to allocate pt"))?;
        ept_pde.set_addr(ept_pt_frame.start_address(), default_flags);
    }

    let ept_pt = ept_pde.addr().as_u64() as *mut EptPageTable;
    let ept_pte = unsafe { &mut (*ept_pt)[guest_addr.p1_index()] };

    if !ept_pte.is_unused() {
        return Err(Error::AllocError("Duplicate mapping"));
    }

    let mut page_flags = EptTableFlags::READ_ACCESS
        | EptTableFlags::PRIV_EXEC_ACCESS
        | EptTableFlags::USERMODE_EXEC_ACCESS
        | EptTableFlags::IGNORE_PAT;
    if !readonly {
        page_flags |= EptTableFlags::WRITE_ACCESS;
    }

    ept_pte.set_addr(host_frame.start_address(), page_flags);
    ept_pte.set_mem_type(EptMemoryType::WriteBack);

    Ok(())
}
