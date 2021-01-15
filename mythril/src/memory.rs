use crate::error::{Error, Result};
use crate::vmcs;
use alloc::boxed::Box;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::convert::TryFrom;
use core::default::Default;
use core::fmt;
use core::ops::{Add, Deref, Index, IndexMut};
use num_enum::TryFromPrimitive;
use spin::RwLock;
use ux;
use x86::bits64::paging::*;
use x86::controlregs::Cr0;

#[repr(align(4096))]
pub struct Raw4kPage(pub [u8; BASE_PAGE_SIZE]);
impl Raw4kPage {
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}

impl Default for Raw4kPage {
    fn default() -> Self {
        Raw4kPage([0u8; BASE_PAGE_SIZE])
    }
}

#[inline]
fn pml4_index(addr: u64) -> ux::u9 {
    ux::u9::new(((addr >> 39usize) & 0b111111111) as u16)
}

#[inline]
fn pdpt_index(addr: u64) -> ux::u9 {
    ux::u9::new(((addr >> 30usize) & 0b111111111) as u16)
}

#[inline]
fn pd_index(addr: u64) -> ux::u9 {
    ux::u9::new(((addr >> 21usize) & 0b111111111) as u16)
}

#[inline]
fn pt_index(addr: u64) -> ux::u9 {
    ux::u9::new(((addr >> 12usize) & 0b111111111) as u16)
}

#[inline]
fn page_offset(addr: u64) -> ux::u12 {
    ux::u12::new((addr & 0b111111111111) as u16)
}

#[inline]
fn large_page_offset(addr: u64) -> ux::u21 {
    ux::u21::new((addr & 0x1fffff) as u32)
}

#[inline]
fn huge_page_offset(addr: u64) -> ux::u30 {
    ux::u30::new((addr & 0x3fffffff) as u32)
}

#[derive(Copy, Clone, Debug)]
pub enum GuestVirtAddr {
    NoPaging(GuestPhysAddr),
    Paging4Level(Guest4LevelPagingAddr),
    //TODO: 5 level paging
}

impl GuestVirtAddr {
    // Convert a 64 bit number to a virtual address in the context of the current
    // guest configuration (as read from a VMCS)
    pub fn new(val: u64, vmcs: &vmcs::ActiveVmcs) -> Result<Self> {
        let cr0 = Cr0::from_bits_truncate(
            vmcs.read_field(vmcs::VmcsField::GuestCr0)? as usize,
        );
        if cr0.contains(Cr0::CR0_ENABLE_PAGING) {
            Ok(GuestVirtAddr::Paging4Level(Guest4LevelPagingAddr::new(val)))
        } else {
            Ok(GuestVirtAddr::NoPaging(GuestPhysAddr::new(val)))
        }
    }

    pub fn as_u64(&self) -> u64 {
        match self {
            Self::NoPaging(addr) => addr.as_u64(),
            Self::Paging4Level(addr) => addr.as_u64(),
        }
    }
}

impl Add<usize> for GuestVirtAddr {
    type Output = GuestVirtAddr;

    fn add(self, rhs: usize) -> Self::Output {
        match self {
            Self::NoPaging(addr) => Self::NoPaging(addr + rhs),
            Self::Paging4Level(addr) => Self::Paging4Level(addr + rhs),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Guest4LevelPagingAddr(u64);
impl Guest4LevelPagingAddr {
    pub fn new(addr: u64) -> Self {
        Self(addr)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn p1_index(&self) -> ux::u9 {
        pt_index(self.0)
    }

    pub fn p2_index(&self) -> ux::u9 {
        pd_index(self.0)
    }

    pub fn p3_index(&self) -> ux::u9 {
        pdpt_index(self.0)
    }

    pub fn p4_index(&self) -> ux::u9 {
        pml4_index(self.0)
    }

    pub fn page_offset(&self) -> ux::u12 {
        page_offset(self.0)
    }

    pub fn large_page_offset(&self) -> ux::u21 {
        large_page_offset(self.0)
    }

    pub fn huge_page_offset(&self) -> ux::u30 {
        huge_page_offset(self.0)
    }
}

impl Add<usize> for Guest4LevelPagingAddr {
    type Output = Guest4LevelPagingAddr;

    fn add(self, rhs: usize) -> Self::Output {
        Guest4LevelPagingAddr(self.0 + (rhs as u64))
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct GuestPhysAddr(u64);

impl GuestPhysAddr {
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn p1_index(&self) -> ux::u9 {
        pt_index(self.0)
    }

    pub fn p2_index(&self) -> ux::u9 {
        pd_index(self.0)
    }

    pub fn p3_index(&self) -> ux::u9 {
        pdpt_index(self.0)
    }

    pub fn p4_index(&self) -> ux::u9 {
        pml4_index(self.0)
    }

    pub fn offset(&self) -> ux::u12 {
        page_offset(self.0)
    }
}

impl Add<usize> for GuestPhysAddr {
    type Output = GuestPhysAddr;

    fn add(self, rhs: usize) -> Self::Output {
        GuestPhysAddr(self.0 + (rhs as u64))
    }
}

impl fmt::Debug for GuestPhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("GuestPhysAddr")
            .field(&format_args!("0x{:x}", self.0))
            .finish()
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct HostPhysAddr(u64);

impl HostPhysAddr {
    pub fn new(addr: u64) -> Self {
        Self(addr)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn is_frame_aligned(&self) -> bool {
        (self.0 & 0b111111111111) == 0
    }
}

impl fmt::Debug for HostPhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HostPhysAddr")
            .field(&format_args!("0x{:x}", self.0))
            .finish()
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct HostPhysFrame(HostPhysAddr);
impl HostPhysFrame {
    pub const SIZE: usize = BASE_PAGE_SIZE;

    pub fn from_start_address(addr: HostPhysAddr) -> Result<Self> {
        if !addr.is_frame_aligned() {
            Err(Error::InvalidValue(
                "Invalid start address for HostPhysFrame".into(),
            ))
        } else {
            Ok(HostPhysFrame(addr))
        }
    }

    pub fn start_address(&self) -> HostPhysAddr {
        self.0
    }

    pub unsafe fn as_array(&self) -> &[u8; Self::SIZE] {
        let ptr = self.0.as_u64() as *const [u8; Self::SIZE];
        ptr.as_ref().unwrap()
    }

    pub unsafe fn as_mut_array(&mut self) -> &mut [u8; Self::SIZE] {
        let ptr = self.0.as_u64() as *mut [u8; Self::SIZE];
        ptr.as_mut().unwrap()
    }
}

pub struct GuestAddressSpace {
    root: RwLock<Box<EptPml4Table>>,
}

#[derive(Copy, Clone, Debug)]
pub struct PrivilegeLevel(pub u8);

#[derive(Copy, Clone, Debug)]
pub enum GuestAccess {
    Read(PrivilegeLevel),
    Write(PrivilegeLevel),
    Fetch(PrivilegeLevel),
}

impl GuestAddressSpace {
    pub fn new() -> Result<Self> {
        Ok(GuestAddressSpace {
            root: RwLock::new(Box::new(EptPml4Table::default())),
        })
    }

    pub fn map_frame(
        &self,
        guest_addr: GuestPhysAddr,
        host_frame: HostPhysFrame,
        readonly: bool,
    ) -> Result<()> {
        map_guest_memory(
            &mut self.root.write(),
            guest_addr,
            host_frame,
            readonly,
        )
    }

    pub fn map_new_frame(
        &self,
        guest_addr: GuestPhysAddr,
        readonly: bool,
    ) -> Result<()> {
        let page = Box::into_raw(Box::new(Raw4kPage::default()));
        let page =
            HostPhysFrame::from_start_address(HostPhysAddr::new(page as u64))?;
        self.map_frame(guest_addr, page, readonly)
    }

    pub fn eptp(&self) -> u64 {
        // //TODO: check available memory types
        (&*(*self.root.read()) as *const _ as u64) | (4 - 1) << 3 | 6
    }

    pub fn translate_linear_address(
        &self,
        cr3: GuestPhysAddr,
        addr: GuestVirtAddr,
        access: GuestAccess,
    ) -> Result<GuestPhysAddr> {
        match addr {
            GuestVirtAddr::NoPaging(addr) => {
                Ok(GuestPhysAddr::new(addr.as_u64()))
            }
            GuestVirtAddr::Paging4Level(vaddr) => {
                self.translate_pl4_address(cr3, vaddr, access)
            }
        }
    }

    //FIXME: this should check that the pages exist, access restrictions, guest page size,
    //       and lots of other things
    fn translate_pl4_address(
        &self,
        cr3: GuestPhysAddr,
        addr: Guest4LevelPagingAddr,
        _access: GuestAccess,
    ) -> Result<GuestPhysAddr> {
        let guest_pml4_root = self.find_host_frame(cr3)?;

        let guest_pml4 =
            guest_pml4_root.start_address().as_u64() as *const PML4;
        let guest_pml4e =
            unsafe { (*guest_pml4)[u16::from(addr.p4_index()) as usize] };
        let guest_pml4e_addr =
            GuestPhysAddr::new(guest_pml4e.address().as_u64());
        let guest_pml4e_host_frame = self.find_host_frame(guest_pml4e_addr)?;

        let guest_pdpt =
            guest_pml4e_host_frame.start_address().as_u64() as *const PDPT;
        let guest_pdpte =
            unsafe { (*guest_pdpt)[u16::from(addr.p3_index()) as usize] };
        let guest_pdpte_addr =
            GuestPhysAddr::new(guest_pdpte.address().as_u64());
        let guest_pdpte_host_frame = self.find_host_frame(guest_pdpte_addr)?;

        let guest_pdt =
            guest_pdpte_host_frame.start_address().as_u64() as *const PD;
        let guest_pdte =
            unsafe { (*guest_pdt)[u16::from(addr.p2_index()) as usize] };
        let _guest_pdte_addr =
            GuestPhysAddr::new(guest_pdte.address().as_u64());

        let translated_vaddr = guest_pdte.address().as_u64()
            + (u32::from(addr.large_page_offset()) as u64);

        Ok(GuestPhysAddr::new(translated_vaddr))
    }

    //FIXME this ignores read/write/exec permissions and 2MB/1GB pages (and lots of other stuff)
    pub fn find_host_frame(
        &self,
        addr: GuestPhysAddr,
    ) -> Result<HostPhysFrame> {
        let ept_pml4e = &self.root.read()[addr.p4_index()];
        if ept_pml4e.is_unused() {
            return Err(Error::InvalidValue(
                "No PML4 entry for GuestPhysAddr".into(),
            ));
        }
        let ept_pdpt =
            ept_pml4e.addr().as_u64() as *const EptPageDirectoryPointerTable;
        let ept_pdpe = unsafe { &(*ept_pdpt)[addr.p3_index()] };
        if ept_pdpe.is_unused() {
            return Err(Error::InvalidValue(
                "No PDP entry for GuestPhysAddr".into(),
            ));
        }
        let ept_pdt = ept_pdpe.addr().as_u64() as *const EptPageDirectory;
        let ept_pde = unsafe { &(*ept_pdt)[addr.p2_index()] };
        if ept_pde.is_unused() {
            return Err(Error::InvalidValue(
                "No PD entry for GuestPhysAddr".into(),
            ));
        }
        let ept_pt = ept_pde.addr().as_u64() as *const EptPageTable;
        let ept_pte = unsafe { &(*ept_pt)[addr.p1_index()] };
        if ept_pte.is_unused() {
            return Err(Error::InvalidValue(
                "No PT entry for GuestPhysAddr".into(),
            ));
        }
        HostPhysFrame::from_start_address(ept_pte.addr())
    }

    pub fn frame_iter(
        &self,
        cr3: GuestPhysAddr,
        addr: GuestVirtAddr,
        access: GuestAccess,
    ) -> Result<FrameIter> {
        //TODO: align the addr to BASE_PAGE_SIZE boundary
        Ok(FrameIter {
            view: GuestAddressSpaceView::new(cr3, self),
            addr: addr,
            access: access,
        })
    }

    pub fn read_bytes(
        &self,
        cr3: GuestPhysAddr,
        addr: GuestVirtAddr,
        mut length: usize,
        access: GuestAccess,
    ) -> Result<Vec<u8>> {
        let mut out = vec![];
        let iter = self.frame_iter(cr3, addr, access)?;

        let mut start_offset = addr.as_u64() as usize % HostPhysFrame::SIZE;
        for frame in iter {
            let frame = frame?;
            let array = unsafe { frame.as_array() };
            let slice = if start_offset + length <= HostPhysFrame::SIZE {
                &array[start_offset..start_offset + length]
            } else {
                &array[start_offset..]
            };
            out.extend_from_slice(slice);

            length -= slice.len();

            if length == 0 {
                break;
            }

            // All frames after the first have no start_offset
            start_offset = 0;
        }

        Ok(out)
    }

    pub fn write_bytes(
        &self,
        cr3: GuestPhysAddr,
        addr: GuestVirtAddr,
        mut bytes: &[u8],
        access: GuestAccess,
    ) -> Result<()> {
        let iter = self.frame_iter(cr3, addr, access)?;

        let mut start_offset = addr.as_u64() as usize % HostPhysFrame::SIZE;
        for frame in iter {
            let mut frame = frame?;
            let array = unsafe { frame.as_mut_array() };
            if start_offset + bytes.len() <= HostPhysFrame::SIZE {
                array[start_offset..start_offset + bytes.len()]
                    .copy_from_slice(&bytes);
                break;
            } else {
                &array[start_offset..].copy_from_slice(
                    &bytes[..(HostPhysFrame::SIZE - start_offset)],
                );
                bytes = &bytes[(HostPhysFrame::SIZE - start_offset)..];
            }

            // All frames after the first have no start_offset
            start_offset = 0;
        }

        Ok(())
    }
}

pub struct GuestAddressSpaceView<'a> {
    space: &'a GuestAddressSpace,
    cr3: GuestPhysAddr,
}

impl<'a> GuestAddressSpaceView<'a> {
    pub fn new(cr3: GuestPhysAddr, space: &'a GuestAddressSpace) -> Self {
        Self { space, cr3 }
    }

    pub fn from_vmcs(
        vmcs: &vmcs::ActiveVmcs,
        space: &'a GuestAddressSpace,
    ) -> Result<Self> {
        let cr3 = vmcs.read_field(vmcs::VmcsField::GuestCr3)?;
        let cr3 = GuestPhysAddr::new(cr3);
        Ok(Self { space, cr3 })
    }

    pub fn frame_iter(
        &self,
        addr: GuestVirtAddr,
        access: GuestAccess,
    ) -> Result<FrameIter> {
        self.space.frame_iter(self.cr3, addr, access)
    }

    pub fn read_bytes(
        &self,
        addr: GuestVirtAddr,
        length: usize,
        access: GuestAccess,
    ) -> Result<Vec<u8>> {
        self.space.read_bytes(self.cr3, addr, length, access)
    }

    pub fn translate_linear_address(
        &self,
        addr: GuestVirtAddr,
        access: GuestAccess,
    ) -> Result<GuestPhysAddr> {
        self.space.translate_linear_address(self.cr3, addr, access)
    }

    pub fn write_bytes(
        &self,
        addr: GuestVirtAddr,
        bytes: &[u8],
        access: GuestAccess,
    ) -> Result<()> {
        self.space.write_bytes(self.cr3, addr, bytes, access)
    }
}

impl<'a> Deref for GuestAddressSpaceView<'a> {
    type Target = GuestAddressSpace;

    fn deref(&self) -> &'a Self::Target {
        self.space
    }
}

pub struct FrameIter<'a> {
    view: GuestAddressSpaceView<'a>,
    addr: GuestVirtAddr,
    access: GuestAccess,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = Result<HostPhysFrame>;

    //TODO: stop at end of address space
    fn next(&mut self) -> Option<Self::Item> {
        let old = self.addr;

        // This is the smallest possible guest page size, so permissions
        // can't change except at this granularity
        self.addr = self.addr + BASE_PAGE_SIZE;

        let physaddr =
            match self.view.translate_linear_address(old, self.access) {
                Ok(addr) => addr,
                Err(e) => return Some(Err(e)),
            };
        Some(self.view.find_host_frame(physaddr))
    }
}

#[repr(align(4096))]
pub struct EptTable<T> {
    entries: [T; PAGE_SIZE_ENTRIES],
}
impl<T> Default for EptTable<T>
where
    T: Copy + Default,
{
    fn default() -> Self {
        Self {
            entries: [T::default(); PAGE_SIZE_ENTRIES],
        }
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

#[derive(Copy, Clone, Default)]
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

    pub fn addr(&self) -> HostPhysAddr {
        HostPhysAddr::new(self.entry & 0x000fffff_fffff000)
    }

    pub fn set_addr(&mut self, addr: HostPhysAddr, flags: EptTableFlags) {
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

#[derive(Copy, Clone, Default)]
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

    pub fn addr(&self) -> HostPhysAddr {
        HostPhysAddr::new(self.entry & 0x000fffff_fffff000)
    }

    pub fn mem_type(&self) -> EptMemoryType {
        EptMemoryType::try_from(((self.entry & (0b111 << 5)) >> 5) as u8)
            .expect("Invalid EPT memory type")
    }

    pub fn set_addr(&mut self, addr: HostPhysAddr, flags: EptTableFlags) {
        assert!(addr.is_frame_aligned());
        self.entry =
            (addr.as_u64()) | flags.bits() | ((self.mem_type() as u64) << 5);
    }

    pub fn set_flags(&mut self, flags: EptTableFlags) {
        self.entry = self.addr().as_u64()
            | flags.bits()
            | ((self.mem_type() as u64) << 5);
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
    guest_ept_base: &mut EptPml4Table,
    guest_addr: GuestPhysAddr,
    host_frame: HostPhysFrame,
    readonly: bool,
) -> Result<()> {
    let default_flags = EptTableFlags::READ_ACCESS
        | EptTableFlags::WRITE_ACCESS
        | EptTableFlags::PRIV_EXEC_ACCESS
        | EptTableFlags::USERMODE_EXEC_ACCESS;

    let ept_pml4e = &mut guest_ept_base[guest_addr.p4_index()];
    if ept_pml4e.is_unused() {
        let ept_pdpt_frame =
            Box::into_raw(Box::new(EptPageDirectoryPointerTable::default()));
        let ept_pdpt_addr = HostPhysAddr::new(ept_pdpt_frame as u64);
        ept_pml4e.set_addr(ept_pdpt_addr, default_flags);
    }

    let ept_pdpt =
        ept_pml4e.addr().as_u64() as *mut EptPageDirectoryPointerTable;
    let ept_pdpe = unsafe { &mut (*ept_pdpt)[guest_addr.p3_index()] };
    if ept_pdpe.is_unused() {
        let ept_pdt_frame =
            Box::into_raw(Box::new(EptPageDirectory::default()));
        let ept_pdt_addr = HostPhysAddr::new(ept_pdt_frame as u64);
        ept_pdpe.set_addr(ept_pdt_addr, default_flags);
    }

    let ept_pdt = ept_pdpe.addr().as_u64() as *mut EptPageDirectory;
    let ept_pde = unsafe { &mut (*ept_pdt)[guest_addr.p2_index()] };
    if ept_pde.is_unused() {
        let ept_pt_frame = Box::into_raw(Box::new(EptPageTable::default()));
        let ept_pt_addr = HostPhysAddr::new(ept_pt_frame as u64);
        ept_pde.set_addr(ept_pt_addr, default_flags);
    }

    let ept_pt = ept_pde.addr().as_u64() as *mut EptPageTable;
    let ept_pte = unsafe { &mut (*ept_pt)[guest_addr.p1_index()] };

    if !ept_pte.is_unused() {
        return Err(Error::DuplicateMapping(format!(
            "Duplicate mapping for address 0x{:x}",
            guest_addr.as_u64()
        )));
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
