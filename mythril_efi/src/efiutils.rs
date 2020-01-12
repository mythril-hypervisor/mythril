use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::mem::{self, MaybeUninit};
use mythril_core::allocator::FrameAllocator;
use mythril_core::error::{Error, Result};
use mythril_core::memory::{HostPhysAddr, HostPhysFrame};
use mythril_core::vm::VmServices;
use uefi::data_types::Handle;
use uefi::prelude::ResultExt;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::pi::mp::MPServices;
use uefi::table::boot::{AllocateType, BootServices, EventType, MemoryType, Tpl};
use uefi::Event;

extern "C" fn ap_startup_callback(param: *mut c_void) {
    let callback: &'static Box<dyn Fn()> = unsafe { mem::transmute(param) };
    callback()
}

fn notify_callback(_: Event) {
    unreachable!()
}

pub fn run_on_all_aps(bt: &BootServices, proc: Box<Box<dyn Fn()>>) -> Result<()> {
    let mp = bt
        .locate_protocol::<MPServices>()
        .expect_success("Failed to find MP service");
    let mp = unsafe { &mut *mp.get() };

    if mp
        .get_number_of_processors()
        .expect_success("Failed to get number of processors")
        .total
        < 2
    {
        return Ok(());
    }

    let proc: &'static Box<dyn Fn()> = Box::leak(proc);

    //TODO: this should probably not be TIMER event type
    let event = unsafe {
        bt.create_event(
            EventType::SIGNAL_EXIT_BOOT_SERVICES,
            Tpl::CALLBACK,
            Some(notify_callback),
        )
        .expect_success("Failed to create event")
    };

    let param: *mut c_void = unsafe { mem::transmute(proc) };

    mp.startup_all_aps(false, ap_startup_callback, param, None, Some(event))
        .expect_success("Failed to start on all aps");
    Ok(())
}

pub struct EfiVmServices<'a> {
    bt: &'a BootServices,
    alloc: EfiAllocator<'a>,
}

impl<'a> VmServices for EfiVmServices<'a> {
    type Allocator = EfiAllocator<'a>;
    fn allocator(&mut self) -> &mut EfiAllocator<'a> {
        &mut self.alloc
    }
    fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        read_file(self.bt, path)
    }
    fn acpi_addr(&self) -> Result<HostPhysAddr> {
        Ok(HostPhysAddr::new(0))
    }
}

impl<'a> EfiVmServices<'a> {
    pub fn new(bt: &'a BootServices) -> Self {
        Self {
            bt: bt,
            alloc: EfiAllocator::new(bt),
        }
    }
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
    fn allocate_frame(&mut self) -> Result<HostPhysFrame> {
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
            core::ptr::write_bytes(ptr, 0, HostPhysFrame::SIZE);
        }

        HostPhysFrame::from_start_address(HostPhysAddr::new(pg))
    }

    fn deallocate_frame(&mut self, frame: HostPhysFrame) -> Result<()> {
        self.bt
            .free_pages(frame.start_address().as_u64(), 1)
            .log_warning()
            .map_err(|_| Error::Uefi("EfiAllocator failed to deallocate frame".into()))
    }
}

//FIXME this whole function is rough
fn read_file(services: &BootServices, path: &str) -> Result<Vec<u8>> {
    let fs = uefi::table::boot::SearchType::from_proto::<SimpleFileSystem>();
    let num_handles = services
        .locate_handle(fs, None)
        .log_warning()
        .map_err(|_| Error::Uefi("Failed to get number of FS handles".into()))?;

    let mut volumes: Vec<Handle> =
        vec![unsafe { MaybeUninit::uninit().assume_init() }; num_handles];
    let _ = services
        .locate_handle(fs, Some(&mut volumes))
        .log_warning()
        .map_err(|_| Error::Uefi("Failed to read FS handles".into()))?;

    for volume in volumes.into_iter() {
        let proto = services
            .handle_protocol::<SimpleFileSystem>(volume)
            .log_warning()
            .map_err(|_| Error::Uefi("Failed to protocol for FS handle".into()))?;
        let fs = unsafe { proto.get().as_mut() }
            .ok_or(Error::NullPtr("FS Protocol ptr was NULL".into()))?;

        let mut root = fs
            .open_volume()
            .log_warning()
            .map_err(|_| Error::Uefi("Failed to open volume".into()))?;

        let handle = match root
            .open(path, FileMode::Read, FileAttribute::READ_ONLY)
            .log_warning()
        {
            Ok(f) => f,
            Err(_) => continue,
        };
        let file = handle
            .into_type()
            .log_warning()
            .map_err(|_| Error::Uefi(format!("Failed to convert file")))?;

        match file {
            FileType::Regular(mut f) => {
                info!("Reading file: {}", path);
                let mut contents = vec![];
                let mut buff = [0u8; 1024];
                while f
                    .read(&mut buff)
                    .log_warning()
                    .map_err(|_| Error::Uefi(format!("Failed to read file: {}", path)))?
                    > 0
                {
                    contents.extend_from_slice(&buff);
                }
                return Ok(contents);
            }
            _ => return Err(Error::Uefi(format!("Image file {} was a directory", path))),
        }
    }

    Err(Error::MissingFile(format!(
        "Unable to find image file {}",
        path
    )))
}
