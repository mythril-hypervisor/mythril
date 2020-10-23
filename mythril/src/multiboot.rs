use crate::boot_info::{self, BootInfo};
use crate::global_alloc;
use crate::memory::HostPhysAddr;
use alloc::vec::Vec;

extern "C" {
    pub static MULTIBOOT_HEADER_START: u32;
    pub static MULTIBOOT_HEADER_END: u32;

    // The _value_ of the last byte of the mythril binary. The
    // address of this symbol is the actual end.
    pub static END_OF_BINARY: u8;
}

// NOTE: see multiboot2::header_location for more information
pub fn header_location() -> (u32, u32) {
    unsafe { (MULTIBOOT_HEADER_START, MULTIBOOT_HEADER_END) }
}

fn setup_global_alloc_region<'a, F>(
    info: &'a multiboot::Multiboot<'a, F>,
) -> (u64, u64)
where
    F: Fn(u64, usize) -> Option<&'a [u8]>,
{
    let regions = info
        .memory_regions()
        .expect("Missing multiboot memory regions");

    let available = regions.filter_map(|region| match region.memory_type() {
        multiboot::MemoryType::Available => Some((
            region.base_address(),
            region.base_address() + region.length(),
        )),
        _ => None,
    });

    debug!("Modules:");
    let modules =
        info.modules()
            .expect("No multiboot modules found")
            .map(|module| {
                debug!("  0x{:x}-0x{:x}", module.start, module.end);
                (module.start, module.end)
            });

    // Avoid allocating over the actual mythril binary (just use 0 as the start
    // for now).
    let mythril_bounds =
        [(0 as u64, unsafe { &END_OF_BINARY as *const u8 as u64 })];
    debug!(
        "Mythril binary bounds: 0x{:x}-0x{:x}",
        mythril_bounds[0].0, mythril_bounds[0].1
    );

    let excluded = modules.chain(mythril_bounds.iter().copied());

    // TODO(alschwalm): For now, we just use the portion of the largest available
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

pub fn early_init_multiboot(addr: HostPhysAddr) -> BootInfo {
    fn multiboot_addr_translate<'a>(
        paddr: u64,
        size: usize,
    ) -> Option<&'a [u8]> {
        unsafe { Some(core::slice::from_raw_parts(paddr as *const u8, size)) }
    }
    let multiboot_info = unsafe {
        multiboot::Multiboot::new(addr.as_u64(), multiboot_addr_translate)
            .expect("Failed to create Multiboot structure")
    };

    let alloc_region = setup_global_alloc_region(&multiboot_info);

    info!(
        "Allocating from 0x{:x}-{:x}",
        alloc_region.0, alloc_region.1
    );

    unsafe {
        global_alloc::Allocator::allocate_from(alloc_region.0, alloc_region.1);
    }

    let modules = multiboot_info
        .modules()
        .expect("No multiboot modules found")
        .map(|module| boot_info::BootModule {
            address: HostPhysAddr::new(module.start),
            size: (module.end - module.start) as usize,
            identifier: module.string.map(alloc::string::String::from),
        })
        .collect::<Vec<_>>();

    BootInfo {
        modules: modules,
        rsdp: None,
    }
}
