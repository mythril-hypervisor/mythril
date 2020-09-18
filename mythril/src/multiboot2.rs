use crate::acpi;
use crate::boot_info::{self, BootInfo};
use crate::global_alloc;
use crate::memory::HostPhysAddr;
use alloc::vec::Vec;

extern "C" {
    pub static MULTIBOOT2_HEADER_START: u32;
    pub static MULTIBOOT2_HEADER_END: u32;
}

// NOTE: this primarily exists so the above symbols will be used. This causes
// rust to retain the multiboot2 symbols when linking the mythril rlib, which
// in turn makes them available to the linker script when linking the binary.
pub fn header_location() -> (u32, u32) {
    unsafe { (MULTIBOOT2_HEADER_START, MULTIBOOT2_HEADER_END) }
}

fn setup_global_alloc_region(info: &multiboot2::BootInformation) -> (u64, u64) {
    let mem_tag = info
        .memory_map_tag()
        .expect("Missing multiboot memory map tag");

    let available = mem_tag
        .memory_areas()
        .map(|area| (area.start_address(), area.end_address()));

    debug!("Modules:");
    let modules = info.module_tags().map(|module| {
        debug!(
            "  0x{:x}-0x{:x}",
            module.start_address(),
            module.end_address()
        );
        (module.start_address() as u64, module.end_address() as u64)
    });

    let sections_tag = info
        .elf_sections_tag()
        .expect("Missing multiboot elf sections tag");

    debug!("Elf sections:");
    let sections = sections_tag.sections().map(|section| {
        debug!(
            "  0x{:x}-0x{:x}",
            section.start_address(),
            section.end_address()
        );
        (section.start_address(), section.end_address())
    });

    // Avoid allocating over the BootInformation structure itself
    let multiboot_info =
        [(info.start_address() as u64, info.end_address() as u64)];
    debug!(
        "Multiboot Info: 0x{:x}-0x{:x}",
        info.start_address(),
        info.end_address()
    );

    let excluded = modules
        .chain(sections)
        .chain(multiboot_info.iter().copied());

    // TODO: For now, we just use the portion of the largest available
    // region that is above the highest excluded region.
    let max_excluded = excluded
        .max_by(|left, right| left.1.cmp(&right.1))
        .expect("No max excluded region");

    let largest_region = available
        .max_by(|left, right| (left.1 - left.0).cmp(&(right.1 - right.0)))
        .expect("No largest region");

    if largest_region.0 > max_excluded.1 {
        largest_region
    } else if max_excluded.1 > largest_region.0
        && max_excluded.1 < largest_region.1
    {
        (max_excluded.1, largest_region.1)
    } else {
        panic!("Unable to find suitable global alloc region")
    }
}

pub fn early_init_multiboot2(addr: HostPhysAddr) -> BootInfo {
    let multiboot_info = unsafe { multiboot2::load(addr.as_u64() as usize) };

    let alloc_region = setup_global_alloc_region(&multiboot_info);

    info!(
        "Allocating from 0x{:x}-{:x}",
        alloc_region.0, alloc_region.1
    );

    unsafe {
        global_alloc::Allocator::allocate_from(alloc_region.0, alloc_region.1);
    }

    let modules = multiboot_info
        .module_tags()
        .map(|tag| boot_info::BootModule {
            address: HostPhysAddr::new(tag.start_address() as u64),
            size: (tag.end_address() - tag.start_address()) as usize,
            identifier: Some(tag.name().into()),
        })
        .collect::<Vec<_>>();

    let rsdp = multiboot_info
        .rsdp_v2_tag()
        .filter(|tag| tag.checksum_is_valid())
        .map(|rsdp_v2| acpi::rsdp::RSDP::V2 {
            xsdt_addr: rsdp_v2.xsdt_address() as u64,
            oemid: {
                let mut oemid = [0u8; 6];
                if let Some(id) = rsdp_v2.oem_id() {
                    oemid.copy_from_slice(id.as_bytes());
                }
                oemid
            },
        })
        .or_else(|| {
            // If there is no v2 tag or if the checksum is invalid, try to
            // use the v1 tag.
            multiboot_info
                .rsdp_v1_tag()
                .filter(|tag| tag.checksum_is_valid())
                .map(|rsdp_v1| acpi::rsdp::RSDP::V1 {
                    rsdt_addr: rsdp_v1.rsdt_address() as u32,
                    oemid: {
                        let mut oemid = [0u8; 6];
                        if let Some(id) = rsdp_v1.oem_id() {
                            oemid.copy_from_slice(id.as_bytes());
                        }
                        oemid
                    },
                })
        });

    BootInfo {
        modules: modules,
        rsdp: rsdp,
    }
}
