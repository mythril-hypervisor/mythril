use alloc::vec::Vec;
use multiboot2::BootInformation;
use mythril_core::error::{Error, Result};
use mythril_core::memory::HostPhysAddr;
use mythril_core::vm::VmServices;

pub struct Multiboot2Services {
    info: BootInformation,
}

impl Multiboot2Services {
    pub fn new(info: BootInformation) -> Self {
        Self { info }
    }
}

impl VmServices for Multiboot2Services {
    fn read_file<'a>(&'a self, path: &str) -> Result<&'a [u8]> {
        for module in self.info.module_tags() {
            if module.name() == path {
                let size = (module.end_address() - module.start_address()) as usize;
                let data: &'static [u8] = unsafe {
                    core::slice::from_raw_parts(module.start_address() as *const u8, size)
                };
                return Ok(data);
            }
        }
        Err(Error::InvalidValue(format!(
            "Invalid multiboot module path: {}",
            path
        )))
    }

    fn acpi_addr(&self) -> Result<HostPhysAddr> {
        Ok(HostPhysAddr::new(0))
    }
}
