use crate::acpi;
use crate::memory::HostPhysAddr;
use alloc::{string::String, vec::Vec};

/// The abstract 'info' provided by the boot environment. This could be
/// bios-multiboot, bios-multiboot2, efi-multiboot2, etc.
///
#[derive(Default)]
pub struct BootInfo {
    pub modules: Vec<BootModule>,
    pub rsdp: Option<acpi::rsdp::RSDP>,
}

impl BootInfo {
    pub fn find_module(&self, ident: impl AsRef<str>) -> Option<&BootModule> {
        self.modules
            .iter()
            .filter(|module| {
                if let Some(id) = &module.identifier {
                    id == ident.as_ref()
                } else {
                    false
                }
            })
            .next()
    }
}

pub struct BootModule {
    pub identifier: Option<String>,
    pub address: HostPhysAddr,
    pub size: usize,
}

impl BootModule {
    pub fn data(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.address.as_u64() as *const u8,
                self.size,
            )
        }
    }
}
