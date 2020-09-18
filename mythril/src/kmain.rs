use crate::acpi;
use crate::ap;
use crate::apic;
use crate::boot_info::BootInfo;
use crate::interrupt;
use crate::linux;
use crate::logger;
use crate::memory;
use crate::multiboot2;
use crate::percore;
use crate::physdev;
use crate::time;
use crate::vcpu;
use crate::virtdev;
use crate::vm;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::{debug, info};
use spin::RwLock;

extern "C" {
    static AP_STARTUP_ADDR: u16;
    static mut AP_STACK_ADDR: u64;
    static mut AP_IDX: u64;
    static mut AP_READY: u8;
}

// Temporary helper function to create a vm for a single core
fn default_vm(
    core: usize,
    mem: u64,
    info: &BootInfo,
) -> Arc<RwLock<vm::VirtualMachine>> {
    let mut config = vm::VirtualMachineConfig::new(vec![core as u8], mem);

    // FIXME: When `map_bios` may return an error, log the error.
    config.map_bios("seabios.bin".into()).unwrap_or(());

    let device_map = config.device_map();
    device_map
        .register_device(virtdev::acpi::AcpiRuntime::new(0xb000).unwrap())
        .unwrap();
    device_map
        .register_device(virtdev::debug::DebugPort::new(core as u64, 0x402))
        .unwrap();
    device_map
        .register_device(virtdev::vga::VgaController::new())
        .unwrap();
    device_map
        .register_device(virtdev::dma::Dma8237::new())
        .unwrap();
    device_map
        .register_device(virtdev::ignore::IgnoredDevice::new())
        .unwrap();
    device_map
        .register_device(virtdev::pci::PciRootComplex::new())
        .unwrap();
    device_map
        .register_device(virtdev::pic::Pic8259::new())
        .unwrap();
    device_map
        .register_device(virtdev::keyboard::Keyboard8042::new())
        .unwrap();
    device_map
        .register_device(virtdev::pit::Pit8254::new())
        .unwrap();
    device_map
        .register_device(virtdev::pos::ProgrammableOptionSelect::new())
        .unwrap();
    device_map
        .register_device(virtdev::rtc::CmosRtc::new(mem))
        .unwrap();

    //TODO: this should actually be per-vcpu
    device_map
        .register_device(virtdev::lapic::LocalApic::new())
        .unwrap();

    let mut fw_cfg_builder = virtdev::qemu_fw_cfg::QemuFwCfgBuilder::new();

    // The 'linuxboot' file is an option rom that loads the linux kernel
    // via qemu_fw_cfg
    fw_cfg_builder
        .add_file(
            "genroms/linuxboot_dma.bin",
            info.find_module("linuxboot_dma.bin").unwrap().data(),
        )
        .unwrap();

    // Passing the bootorder file automatically selects the option rom
    // as the default boot device
    fw_cfg_builder
        .add_file(
            "bootorder",
            "/rom@genroms/linuxboot_dma.bin\nHALT".as_bytes(),
        )
        .unwrap();

    linux::load_linux(
        "kernel",
        "initramfs",
        core::concat!(
            "rodata=0 nopti disableapic acpi=off ",
            "earlyprintk=serial,0x3f8,115200 ",
            "console=ttyS0 debug nokaslr noapic mitigations=off ",
            "root=/dev/ram0 rdinit=/bin/sh\0"
        )
        .as_bytes(),
        mem,
        &mut fw_cfg_builder,
        info,
    )
    .unwrap();
    device_map.register_device(fw_cfg_builder.build()).unwrap();

    vm::VirtualMachine::new(config, info).expect("Failed to create vm")
}

#[no_mangle]
pub extern "C" fn ap_entry(_ap_data: &ap::ApData) -> ! {
    unsafe { interrupt::idt::ap_init() };

    let local_apic =
        apic::LocalApic::init().expect("Failed to initialize local APIC");

    info!(
        "X2APIC:\tid={}\tbase=0x{:x}\tversion=0x{:x})",
        local_apic.id(),
        local_apic.raw_base(),
        local_apic.version()
    );

    unsafe { interrupt::enable_interrupts() };

    vcpu::mp_entry_point()
}

static LOGGER: logger::DirectLogger = logger::DirectLogger::new();

#[no_mangle]
pub unsafe extern "C" fn kmain_multiboot2(multiboot_info_addr: usize) -> ! {
    let boot_info = multiboot2::early_init_multiboot2(
        memory::HostPhysAddr::new(multiboot_info_addr as u64),
    );
    kmain(boot_info)
}

unsafe fn kmain(mut boot_info: BootInfo) -> ! {
    // Setup the actual interrupt handlers
    interrupt::idt::init();

    // Setup our (com0) logger
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .expect("Failed to set logger");

    // Calibrate the global time source
    time::init_global_time().expect("Failed to init global timesource");

    // physdev::keyboard::Ps2Controller::init().expect("Failed to init ps2 controller");

    // let uart = physdev::com::Uart8250::new(0x3f8);
    // unsafe {interrupt::enable_interrupts()};

    // loop {}

    // If the boot method provided an RSDT, use that one. Otherwise, search the
    // BIOS areas for it.
    let rsdt = boot_info
        .rsdp
        .get_or_insert_with(|| {
            acpi::rsdp::RSDP::find().expect("Failed to find the RSDP")
        })
        .rsdt()
        .expect("Failed to read RSDT");

    // Initialize the BSP local APIC
    let local_apic =
        apic::LocalApic::init().expect("Failed to initialize local APIC");

    let madt_sdt = rsdt.find_entry(b"APIC").expect("No MADT found");
    let madt = acpi::madt::MADT::new(&madt_sdt);

    let apic_ids = madt
        .structures()
        .filter_map(|ics| match ics {
            // TODO(dlrobertson): Check the flags to ensure we can acutally
            // use this APIC.
            Ok(acpi::madt::Ics::LocalApic { apic_id, .. }) => {
                Some(apic_id as u32)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    percore::init_sections(apic_ids.len())
        .expect("Failed to initialize per-core sections");

    let mut map = BTreeMap::new();
    for apic_id in apic_ids.iter() {
        map.insert(
            *apic_id as usize,
            default_vm(*apic_id as usize, 256, &boot_info),
        );
    }

    vm::VM_MAP = Some(map);

    debug!("AP_STARTUP address: 0x{:x}", AP_STARTUP_ADDR);

    // for (idx, apic_id) in apic_ids.into_iter().enumerate() {
    //     if apic_id == local_apic.id() as u32 {
    //         continue;
    //     }

    //     // Allocate a stack for the AP
    //     let stack = vec![0u8; 100 * 1024];

    //     // Get the the bottom of the stack and align
    //     let stack_bottom =
    //         (stack.as_ptr() as u64 + stack.len() as u64) & 0xFFFFFFFFFFFFFFF0;

    //     core::mem::forget(stack);

    //     core::ptr::write_volatile(&mut AP_STACK_ADDR as *mut u64, stack_bottom);

    //     // Map the APIC ids to a sequential list and pass it to the AP
    //     core::ptr::write_volatile(&mut AP_IDX as *mut u64, idx as u64);

    //     // mfence to ensure that the APs see the new stack address
    //     core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    //     debug!("Send INIT to AP id={}", apic_id);
    //     local_apic.send_ipi(
    //         apic_id,
    //         apic::DstShorthand::NoShorthand,
    //         apic::TriggerMode::Edge,
    //         apic::Level::Assert,
    //         apic::DstMode::Physical,
    //         apic::DeliveryMode::Init,
    //         0,
    //     );

    //     debug!("Send SIPI to AP id={}", apic_id);
    //     local_apic.send_ipi(
    //         apic_id,
    //         apic::DstShorthand::NoShorthand,
    //         apic::TriggerMode::Edge,
    //         apic::Level::Assert,
    //         apic::DstMode::Physical,
    //         apic::DeliveryMode::StartUp,
    //         (AP_STARTUP_ADDR >> 12) as u8,
    //     );

    //     // Wait until the AP reports that it is done with startup
    //     while core::ptr::read_volatile(&AP_READY as *const u8) != 1 {}

    //     // Once the AP is done, clear the ready flag
    //     core::ptr::write_volatile(&mut AP_READY as *mut u8, 0);
    // }

    vcpu::mp_entry_point()
}
