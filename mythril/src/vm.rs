use crate::boot_info::BootInfo;
use crate::error::{Error, Result};
use crate::memory::{
    self, GuestAddressSpace, GuestPhysAddr, HostPhysAddr, HostPhysFrame,
    Raw4kPage,
};
use crate::physdev;
use crate::vcpu;
use crate::virtdev::{
    DeviceEvent, DeviceInteraction, DeviceMap, Event, MemReadRequest,
    MemWriteRequest, Port, PortReadRequest, PortWriteRequest,
    ResponseEventArray,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

pub static mut VM_MAP: Option<BTreeMap<usize, Arc<RwLock<VirtualMachine>>>> =
    None;

#[derive(Default)]
pub struct PhysicalDeviceConfig {
    /// The physical serial connection for this VM (if any).
    pub serial: Option<physdev::com::Uart8250>,
}

/// A configuration for a `VirtualMachine`
pub struct VirtualMachineConfig {
    _cpus: Vec<u8>,
    images: Vec<(String, GuestPhysAddr)>,
    bios: Option<String>,
    virtual_devices: DeviceMap,
    physical_devices: PhysicalDeviceConfig,
    memory: u64, // in MB
}

impl VirtualMachineConfig {
    /// Creates a new configuration for a `VirtualMachine`
    ///
    /// # Arguments
    ///
    /// * `cpus` - A list of the cores used by the VM (by APIC id)
    /// * `memory` - The amount of VM memory (in MB)
    pub fn new(
        cpus: Vec<u8>,
        memory: u64,
        physical_devices: PhysicalDeviceConfig,
    ) -> VirtualMachineConfig {
        VirtualMachineConfig {
            _cpus: cpus,
            images: vec![],
            virtual_devices: DeviceMap::default(),
            physical_devices: physical_devices,
            bios: None,
            memory: memory,
        }
    }

    /// Specify that the given image 'path' should be mapped to the given address
    ///
    /// The precise meaning of `image` will vary by platform. This will be a
    /// value suitable to be passed to `VmServices::read_file`.
    pub fn map_image(
        &mut self,
        image: String,
        addr: GuestPhysAddr,
    ) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }

    /// Specify that the given image 'path' should be mapped as the BIOS
    ///
    /// The precise meaning of `image` will vary by platform. This will be a
    /// value suitable to be passed to `VmServices::read_file`.
    ///
    /// The BIOS image will be mapped such that the end of the image is at
    /// 0xffffffff and 0xfffff (i.e., it will be mapped in two places)
    pub fn map_bios(&mut self, bios: String) -> Result<()> {
        self.bios = Some(bios);
        Ok(())
    }

    /// Access the configurations virtual `DeviceMap`
    pub fn virtual_devices(&self) -> &DeviceMap {
        &self.virtual_devices
    }

    /// Access the configurations virtual `DeviceMap` mutably
    pub fn virtual_devices_mut(&mut self) -> &mut DeviceMap {
        &mut self.virtual_devices
    }

    pub fn physical_devices(&self) -> &PhysicalDeviceConfig {
        &self.physical_devices
    }

    pub fn physical_devices_mut(&mut self) -> &mut PhysicalDeviceConfig {
        &mut self.physical_devices
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
        info: &BootInfo,
    ) -> Result<Arc<RwLock<Self>>> {
        let guest_space = Self::setup_ept(&config, info)?;

        Ok(Arc::new(RwLock::new(Self {
            config: config,
            guest_space: guest_space,
        })))
    }

    pub fn dispatch_event(
        &mut self,
        ident: impl DeviceInteraction + core::fmt::Debug,
        kind: DeviceEvent,
        vcpu: &crate::vcpu::VCpu,
        responses: &mut ResponseEventArray,
    ) -> Result<()> {
        let dev = self
            .config
            .virtual_devices()
            .find_device(ident)
            .ok_or_else(|| {
                Error::MissingDevice("Unable to dispatch event".into())
            })?;

        let space = crate::memory::GuestAddressSpaceViewMut::from_vmcs(
            &vcpu.vmcs,
            &mut self.guest_space,
        )?;

        let event = Event::new(kind, space, vcpu, responses)?;

        dev.write().on_event(event)
    }

    fn map_data(
        image: &[u8],
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
    ) -> Result<()> {
        for (i, chunk) in image.chunks(4096 as usize).enumerate() {
            let frame_ptr =
                Box::into_raw(Box::new(Raw4kPage::default())) as *mut u8;
            let frame = HostPhysFrame::from_start_address(HostPhysAddr::new(
                frame_ptr as u64,
            ))?;
            let chunk_ptr = chunk.as_ptr();
            unsafe {
                core::ptr::copy_nonoverlapping(
                    chunk_ptr,
                    frame_ptr,
                    chunk.len(),
                );
            }

            space.map_frame(
                memory::GuestPhysAddr::new(
                    addr.as_u64() + (i as u64 * 4096) as u64,
                ),
                frame,
                false,
            )?;
        }
        Ok(())
    }

    fn map_image(
        image: &str,
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
        info: &BootInfo,
    ) -> Result<()> {
        let data = info
            .find_module(image)
            .ok_or_else(|| {
                Error::InvalidValue(format!("No such module '{}'", image))
            })?
            .data();
        Self::map_data(data, addr, space)
    }

    fn map_bios(
        bios: &str,
        space: &mut GuestAddressSpace,
        info: &BootInfo,
    ) -> Result<()> {
        let data = info
            .find_module(bios)
            .ok_or_else(|| {
                Error::InvalidValue(format!("No such bios '{}'", bios))
            })?
            .data();
        let bios_size = data.len() as u64;
        Self::map_data(
            data,
            &memory::GuestPhysAddr::new((1024 * 1024) - bios_size),
            space,
        )?;
        Self::map_data(
            data,
            &memory::GuestPhysAddr::new((4 * 1024 * 1024 * 1024) - bios_size),
            space,
        )
    }

    fn setup_ept(
        config: &VirtualMachineConfig,
        info: &BootInfo,
    ) -> Result<GuestAddressSpace> {
        let mut guest_space = GuestAddressSpace::new()?;

        // First map the bios
        if let Some(ref bios) = config.bios {
            Self::map_bios(&bios, &mut guest_space, info)?;
        }

        // Now map any guest iamges
        for image in config.images.iter() {
            Self::map_image(&image.0, &image.1, &mut guest_space, info)?;
        }

        // Iterate over each page
        for i in 0..(config.memory << 8) {
            match guest_space.map_new_frame(
                memory::GuestPhysAddr::new((i as u64 * 4096) as u64),
                false,
            ) {
                Ok(_) | Err(Error::DuplicateMapping(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(guest_space)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_vm_creation() {
        let info = BootInfo::default();
        let phys_config = PhysicalDeviceConfig::default();

        let config = VirtualMachineConfig::new(vec![1], 0, phys_config);
        VirtualMachine::new(config, &info).unwrap();
    }
}
