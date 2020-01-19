use crate::device::{DeviceMap, EmulatedDevice, Port, PortIoValue};
use crate::error::{self, Error, Result};
use crate::memory::{self, GuestAddressSpace, GuestPhysAddr, HostPhysAddr};
use crate::registers::{GdtrBase, IdtrBase};
use crate::{vmcs, vmexit, vmx};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::marker::PhantomData;
use spin::RwLock;
use x86::bits64::segmentation::{rdfsbase, rdgsbase};
use x86::controlregs::{cr0, cr3, cr4};
use x86::msr;

pub trait VmServices {
    fn read_file(&self, path: &str) -> Result<Vec<u8>>;
    fn acpi_addr(&self) -> Result<HostPhysAddr>;
}

pub struct VirtualMachineConfig {
    cpus: Vec<u8>,
    images: Vec<(String, GuestPhysAddr)>,
    devices: DeviceMap,
    memory: u64, // number of 4k pages
}

impl VirtualMachineConfig {
    pub fn new(cpus: Vec<u8>, memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            cpus: cpus,
            images: vec![],
            devices: DeviceMap::default(),
            memory: memory,
        }
    }

    pub fn load_image(&mut self, image: String, addr: GuestPhysAddr) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }

    pub fn device_map(&mut self) -> &mut DeviceMap {
        &mut self.devices
    }
}

pub struct VirtualMachine {
    pub config: VirtualMachineConfig,
    pub guest_space: GuestAddressSpace,
}

impl VirtualMachine {
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
        let alloc = services.allocator();
        for (i, chunk) in image.chunks(4096 as usize).enumerate() {
            let mut host_frame = alloc.allocate_frame()?;

            let frame_ptr = host_frame.start_address().as_u64() as *mut u8;
            let chunk_ptr = chunk.as_ptr();
            unsafe {
                core::ptr::copy_nonoverlapping(chunk_ptr, frame_ptr, chunk.len());
            }

            space.map_frame(
                alloc,
                memory::GuestPhysAddr::new(addr.as_u64() + (i as u64 * 4096) as u64),
                host_frame,
                false,
            )?;
        }
        Ok(())
    }

    fn setup_ept(
        config: &VirtualMachineConfig,
        services: &mut impl VmServices,
    ) -> Result<GuestAddressSpace> {
        let alloc = services.allocator();
        let mut guest_space = GuestAddressSpace::new(alloc)?;

        // FIXME: For now, just map 320MB of RAM
        for i in 0..81920 {
            let mut host_frame = alloc.allocate_frame()?;

            guest_space.map_frame(
                alloc,
                memory::GuestPhysAddr::new((i as u64 * 4096) as u64),
                host_frame,
                false,
            )?;
        }

        for image in config.images.iter() {
            Self::map_image(&image.0, &image.1, &mut guest_space, services)?;
        }

        Ok(guest_space)
    }
}
