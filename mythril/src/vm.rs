use crate::apic;
use crate::boot_info::BootInfo;
use crate::error::{Error, Result};
use crate::interrupt;
use crate::lock::ro_after_init::RoAfterInit;
use crate::memory::{
    self, GuestAddressSpace, GuestPhysAddr, HostPhysAddr, HostPhysFrame,
    Raw4kPage,
};
use crate::percore;
use crate::physdev;
use crate::time;
use crate::virtdev::{
    DeviceEvent, DeviceInteraction, DeviceMap, Event, ResponseEventArray,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use arraydeque::ArrayDeque;
use core::mem;
use spin::RwLock;

static BIOS_BLOB: &'static [u8] = include_bytes!("blob/bios.bin");

static VIRTUAL_MACHINES: RoAfterInit<VirtualMachines> =
    RoAfterInit::uninitialized();

pub unsafe fn init_virtual_machines(machines: VirtualMachines) {
    RoAfterInit::init(&VIRTUAL_MACHINES, machines);
}

/// Get the virtual machine that owns the core with the given core id
///
/// This method is unsafe as it should almost certainly not be used (use message
/// passing instead of directly access the remote VM).
pub unsafe fn get_vm_for_core_id(
    core_id: percore::CoreId,
) -> Option<Arc<RwLock<VirtualMachine>>> {
    VIRTUAL_MACHINES.get_by_core_id(core_id)
}

//FIXME(alschwalm): this breaks if the current VM is already locked
pub fn send_vm_msg(msg: VirtualMachineMsg, vmid: u32) -> Result<()> {
    VIRTUAL_MACHINES.send_msg(msg, vmid)
}

pub fn send_vm_msg_core(
    msg: VirtualMachineMsg,
    core_id: percore::CoreId,
) -> Result<()> {
    VIRTUAL_MACHINES.send_msg_core(msg, core_id)
}

pub fn recv_vm_msg() -> Option<VirtualMachineMsg> {
    VIRTUAL_MACHINES.resv_msg()
}

pub fn max_vm_id() -> u32 {
    VIRTUAL_MACHINES.len() as u32
}

const MAX_PENDING_MSG: usize = 100;

pub enum VirtualMachineMsg {
    GrantConsole(physdev::com::Uart8250),
    CancelTimer(time::TimerId),
}

struct VirtualMachineContext {
    vm: Arc<RwLock<VirtualMachine>>,
    msgqueue: RwLock<ArrayDeque<[VirtualMachineMsg; MAX_PENDING_MSG]>>,
}

pub struct VirtualMachines {
    map: BTreeMap<percore::CoreId, VirtualMachineContext>,
}

impl VirtualMachines {
    fn context_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<&VirtualMachineContext> {
        self.map.get(&core_id)
    }

    fn context_by_vm_id(&self, id: u32) -> Option<&VirtualMachineContext> {
        self.map
            .iter()
            .filter_map(|(_core_id, context)| {
                if context.vm.read().id == id {
                    Some(context)
                } else {
                    None
                }
            })
            .next()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn get_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<Arc<RwLock<VirtualMachine>>> {
        self.context_by_core_id(core_id)
            .map(|context| context.vm.clone())
    }

    pub fn get_by_vm_id(&self, id: u32) -> Option<Arc<RwLock<VirtualMachine>>> {
        self.context_by_vm_id(id).map(|context| context.vm.clone())
    }

    pub fn send_msg_core(
        &self,
        msg: VirtualMachineMsg,
        core_id: percore::CoreId,
    ) -> Result<()> {
        let context = self
            .context_by_core_id(core_id)
            .ok_or_else(|| Error::NotFound)?;
        context.msgqueue.write().push_back(msg).map_err(|_| {
            Error::InvalidValue(format!(
                "RX queue is full for core_id = {}",
                core_id
            ))
        })?;

        // Transmit the IPC external interrupt vector to the other vm, so it will
        // process the message.
        unsafe {
            let localapic = apic::get_local_apic_mut();
            localapic.send_ipi(
                core_id.raw.into(), //TODO(alschwalm): convert core_id to APIC ID
                apic::DstShorthand::NoShorthand,
                apic::TriggerMode::Edge,
                apic::Level::Assert,
                apic::DstMode::Physical,
                apic::DeliveryMode::Fixed,
                interrupt::IPC_VECTOR,
            );
        }

        Ok(())
    }

    pub fn send_msg(&self, msg: VirtualMachineMsg, vm_id: u32) -> Result<()> {
        //TODO(alschwalm): this should actually be the BSP core_id of the other vm
        self.send_msg_core(msg, vm_id.into())
    }

    pub fn resv_msg(&self) -> Option<VirtualMachineMsg> {
        let context = self
            .context_by_core_id(percore::read_core_id())
            .expect("No VirtualMachineContext for apic id");
        context.msgqueue.write().pop_front()
    }
}

pub struct VirtualMachineBuilder {
    // Mapping of core_id to VirtualMachine
    map: BTreeMap<percore::CoreId, Arc<RwLock<VirtualMachine>>>,
}

impl VirtualMachineBuilder {
    pub fn new() -> Self {
        VirtualMachineBuilder {
            map: BTreeMap::new(),
        }
    }

    pub fn insert_machine(
        &mut self,
        vm: Arc<RwLock<VirtualMachine>>,
    ) -> Result<()> {
        for cpu in vm.read().config.cpus() {
            self.map.insert(percore::CoreId::from(*cpu), vm.clone());
        }
        Ok(())
    }

    pub fn get_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<Arc<RwLock<VirtualMachine>>> {
        self.map.get(&core_id).map(|vm| vm.clone())
    }

    pub fn finalize(self) -> VirtualMachines {
        VirtualMachines {
            map: self
                .map
                .into_iter()
                .map(|(core_id, vm)| {
                    (
                        core_id,
                        VirtualMachineContext {
                            vm: vm,
                            msgqueue: RwLock::new(ArrayDeque::new()),
                        },
                    )
                })
                .collect(),
        }
    }
}

#[derive(Default)]
pub struct PhysicalDeviceConfig {
    /// The physical serial connection for this VM (if any).
    pub serial: Option<physdev::com::Uart8250>,

    /// The physical ps2 keyboard connection for this VM (if any).
    pub ps2_keyboard: Option<physdev::keyboard::Ps2Controller>,
}

/// A configuration for a `VirtualMachine`
pub struct VirtualMachineConfig {
    cpus: Vec<percore::CoreId>,
    images: Vec<(String, GuestPhysAddr)>,
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
        cpus: Vec<percore::CoreId>,
        memory: u64,
        physical_devices: PhysicalDeviceConfig,
    ) -> VirtualMachineConfig {
        VirtualMachineConfig {
            cpus: cpus,
            images: vec![],
            virtual_devices: DeviceMap::default(),
            physical_devices: physical_devices,
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

    pub fn cpus(&self) -> &Vec<percore::CoreId> {
        &self.cpus
    }
}

/// A virtual machine
pub struct VirtualMachine {
    /// The numeric ID of this virtual machine
    pub id: u32,

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
        id: u32,
        config: VirtualMachineConfig,
        info: &BootInfo,
    ) -> Result<Arc<RwLock<Self>>> {
        let guest_space = Self::setup_ept(&config, info)?;

        Ok(Arc::new(RwLock::new(Self {
            id: id,
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

        let event = Event::new(kind, space, responses)?;

        dev.write().on_event(event)
    }

    fn map_data(
        image: &[u8],
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
    ) -> Result<()> {
        for (i, chunk) in image.chunks(mem::size_of::<Raw4kPage>()).enumerate()
        {
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
                    addr.as_u64()
                        + (i as u64 * mem::size_of::<Raw4kPage>() as u64),
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

    fn map_bios(space: &mut GuestAddressSpace) -> Result<()> {
        let bios_size = BIOS_BLOB.len() as u64;
        Self::map_data(
            BIOS_BLOB,
            &memory::GuestPhysAddr::new((1024 * 1024) - bios_size),
            space,
        )?;
        Self::map_data(
            BIOS_BLOB,
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
        Self::map_bios(&mut guest_space)?;

        // Now map any guest iamges
        for image in config.images.iter() {
            Self::map_image(&image.0, &image.1, &mut guest_space, info)?;
        }

        // Iterate over each page
        for i in 0..(config.memory << 8) {
            match guest_space.map_new_frame(
                memory::GuestPhysAddr::new(
                    i as u64 * mem::size_of::<Raw4kPage>() as u64,
                ),
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

        let config = VirtualMachineConfig::new(
            vec![percore::CoreId::from(1)],
            0,
            phys_config,
        );
        VirtualMachine::new(0, config, &info).unwrap();
    }
}
