use crate::device::DeviceMap;
use crate::error::Result;
use crate::memory::{
    self, GuestAddressSpace, GuestPhysAddr, HostPhysAddr, HostPhysFrame, Raw4kPage,
};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

/// The abstract 'services' required for a given platform to support Mythril
///
/// For example, if implementing BIOS boot support for Mythril, `read_path`,
/// might use a filesystem driver in conjunction with INT 13. A UEFI-based
/// implementation might use the EFI boot-services to support file reading.
pub trait VmServices {
    fn read_file<'a>(&'a self, path: &str) -> Result<&'a [u8]>;
    fn acpi_addr(&self) -> Result<HostPhysAddr>;
}

/// A configuration for a `VirtualMachine`
pub struct VirtualMachineConfig {
    _cpus: Vec<u8>,
    images: Vec<(String, GuestPhysAddr)>,
    devices: DeviceMap,
    _memory: u64, // number of 4k pages
}

impl VirtualMachineConfig {
    /// Creates a new configuration for a `VirtualMachine`
    ///
    /// # Arguments
    ///
    /// * `cpus` - A list of the cores used by the VM (by APIC id)
    /// * `memory` - The amount of VM memory (in MB)
    pub fn new(cpus: Vec<u8>, memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            _cpus: cpus,
            images: vec![],
            devices: DeviceMap::default(),
            _memory: memory,
        }
    }

    /// Specify that the given image 'path' should be mapped to the given address
    ///
    /// The precise meaning of `image` will vary by platform. This will be a
    /// value suitable to be passed to `VmServices::read_file`.
    pub fn load_image(&mut self, image: String, addr: GuestPhysAddr) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }

    /// Access the configurations `DeviceMap`
    pub fn device_map(&mut self) -> &mut DeviceMap {
        &mut self.devices
    }
}

/// A virtual machine
pub struct VirtualMachine {
    /// The configuration for this virtual machine (including the `DeviceMap`)
    pub config: VirtualMachineConfig,

    /// The guest virtual address space
    ///
    /// This will be shared by all `VCpu`s associated with this VM.
    pub guest_space: GuestAddressSpace,
}

impl VirtualMachine {
    /// Construct a new `VirtualMachine` using the given config
    ///
    /// This creates the guest address space (allocating the needed memory),
    /// and maps in the requested images.
    pub fn new(
        config: VirtualMachineConfig,
        services: &mut impl VmServices,
    ) -> Result<Arc<RwLock<Self>>> {
        let guest_space = Self::setup_ept(&config, services)?;
        Ok(Arc::new(RwLock::new(Self {
            config: config,
            guest_space: guest_space,
        })))
    }

    fn map_image(
        image: &str,
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
        services: &mut impl VmServices,
    ) -> Result<()> {
        let image = services.read_file(image)?;
        for (i, chunk) in image.chunks(4096 as usize).enumerate() {
            let frame_ptr = Box::into_raw(Box::new(Raw4kPage::default())) as *mut u8;
            let frame = HostPhysFrame::from_start_address(HostPhysAddr::new(frame_ptr as u64))?;
            let chunk_ptr = chunk.as_ptr();
            unsafe {
                core::ptr::copy_nonoverlapping(chunk_ptr, frame_ptr, chunk.len());
            }

            space.map_frame(
                memory::GuestPhysAddr::new(addr.as_u64() + (i as u64 * 4096) as u64),
                frame,
                false,
            )?;
        }
        Ok(())
    }

    fn setup_ept(
        config: &VirtualMachineConfig,
        services: &mut impl VmServices,
    ) -> Result<GuestAddressSpace> {
        let mut guest_space = GuestAddressSpace::new()?;

        // FIXME: For now, just map 320MB of RAM
        for i in 0..81920 {
            guest_space
                .map_new_frame(memory::GuestPhysAddr::new((i as u64 * 4096) as u64), false)?;
        }

        for image in config.images.iter() {
            Self::map_image(&image.0, &image.1, &mut guest_space, services)?;
        }

        Ok(guest_space)
    }
}
