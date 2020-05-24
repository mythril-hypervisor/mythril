use multiboot2::BootInformation;
use mythril_core::acpi;
use mythril_core::error::{Error, Result};
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
                let size =
                    (module.end_address() - module.start_address()) as usize;
                let data: &'static [u8] = unsafe {
                    core::slice::from_raw_parts(
                        module.start_address() as *const u8,
                        size,
                    )
                };
                return Ok(data);
            }
        }
        Err(Error::InvalidValue(format!(
            "Invalid multiboot module path: {}",
            path
        )))
    }

    fn rsdp(&self) -> Result<acpi::rsdp::RSDP> {
        // Get a v1 tag if possible
        self.info
            .rsdp_v1_tag()
            .map(|tag| {
                // use empty string if no OEM id provided
                let id = tag.oem_id().unwrap_or("      ").as_bytes();
                let mut arr: [u8; 6] = [0; 6];
                arr.copy_from_slice(&id[0..6]);
                acpi::rsdp::RSDP::V1 {
                    oemid: arr,
                    rsdt_addr: tag.rsdt_address() as u32,
                }
            })
            .ok_or(Error::NotFound)
    }
}
