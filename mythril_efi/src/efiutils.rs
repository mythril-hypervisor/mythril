use alloc::vec::Vec;
use core::mem::MaybeUninit;
use mythril_core::error::{Error, Result};
use mythril_core::memory::HostPhysAddr;
use mythril_core::vm::VmServices;
use uefi::data_types::Handle;
use uefi::prelude::ResultExt;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::BootServices;

pub struct EfiVmServices<'a> {
    bt: &'a BootServices,
}

impl<'a> VmServices for EfiVmServices<'a> {
    fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        read_file(self.bt, path)
    }
    fn acpi_addr(&self) -> Result<HostPhysAddr> {
        Ok(HostPhysAddr::new(0))
    }
}

impl<'a> EfiVmServices<'a> {
    pub fn new(bt: &'a BootServices) -> Self {
        Self { bt: bt }
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
