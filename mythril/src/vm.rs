#![deny(missing_docs)]

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
use crate::vcpu;
use crate::virtdev::{
    self, DeviceEvent, DeviceInteraction, DeviceMap, Event, ResponseEventArray,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use arraydeque::ArrayDeque;
use arrayvec::ArrayVec;
use core::mem;
use core::sync::atomic::AtomicU32;
use spin::RwLock;

static BIOS_BLOB: &'static [u8] = include_bytes!("blob/bios.bin");

//TODO(alschwalm): this should always be reported by the relevant MSR
/// The location of the local apic in the guest address space
pub const GUEST_LOCAL_APIC_ADDR: GuestPhysAddr = GuestPhysAddr::new(0xfee00000);

static VIRTUAL_MACHINES: RoAfterInit<VirtualMachineSet> =
    RoAfterInit::uninitialized();

/// The maximum numer of cores that can be assigned to a single VM
pub const MAX_PER_VM_CORE_COUNT: usize = 32;

const MAX_PENDING_MSG: usize = 100;

/// Initialize the global VirtualMachineSet
///
/// This method must be called before calling 'virtual_machines'
pub unsafe fn init_virtual_machines(machines: VirtualMachineSet) {
    RoAfterInit::init(&VIRTUAL_MACHINES, machines);
}

/// Get the global VirtualMachineSet
pub fn virtual_machines() -> &'static VirtualMachineSet {
    &*VIRTUAL_MACHINES
}

/// A message for inter-core or inter-VM communication
///
/// These messages can be sent and received through the
/// VirtualMachineSet type, accessible via the 'virtual_machines'
/// method after startup
pub enum VirtualMachineMsg {
    /// Transfer control of a physical serial console to another VM
    GrantConsole(physdev::com::Uart8250),

    /// Cancel a the given timer
    CancelTimer(time::TimerId),

    /// Start a core at the given physical address
    StartVcpu(GuestPhysAddr),

    /// Inject a guest interrupt with the given vector
    GuestInterrupt {
        /// The type of the injected interrupt
        kind: vcpu::InjectedInterruptType,

        /// The injected interrupt vector
        vector: u8,
    },
}

struct VirtualMachineContext {
    vm: Arc<VirtualMachine>,

    /// The per-core RX message queue
    msgqueue: RwLock<ArrayDeque<[VirtualMachineMsg; MAX_PENDING_MSG]>>,
}

/// The set of configured virtual machines
///
/// This structure represents the set of all configured machines
/// in the hypervisor and can be used to transmit and receive
/// inter-vm or inter-core messages.
pub struct VirtualMachineSet {
    machine_count: u32,
    map: BTreeMap<percore::CoreId, VirtualMachineContext>,
}

impl VirtualMachineSet {
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
                if context.vm.id == id {
                    Some(context)
                } else {
                    None
                }
            })
            .next()
    }

    /// Returns the number of VMs
    pub fn count(&self) -> u32 {
        self.machine_count
    }

    /// Returns whether a given CoreId is associated with any VM
    pub fn is_assigned_core_id(&self, core_id: percore::CoreId) -> bool {
        self.map.contains_key(&core_id)
    }

    /// Get the virtual machine that owns the core with the given core id
    ///
    /// This method is unsafe as it should almost certainly not be used (use message
    /// passing instead of directly accessing the remote VM).
    pub unsafe fn get_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<Arc<VirtualMachine>> {
        self.context_by_core_id(core_id)
            .map(|context| context.vm.clone())
    }

    /// Get a VirtualMachine by its vmid
    pub fn get_by_vm_id(&self, vmid: u32) -> Option<Arc<VirtualMachine>> {
        self.context_by_vm_id(vmid)
            .map(|context| context.vm.clone())
    }

    /// Get the CoreId for the BSP core of a VM by its vmid
    pub fn bsp_core_id(&self, vmid: u32) -> Option<percore::CoreId> {
        self.get_by_vm_id(vmid).map(|vm| vm.config.bsp_id())
    }

    /// Send the given message to a specific core
    ///
    /// If 'notify' is true, an interrupt will be sent to the recipient.
    pub fn send_msg_core(
        &self,
        msg: VirtualMachineMsg,
        core_id: percore::CoreId,
        notify: bool,
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

        if !notify {
            return Ok(());
        }

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
                interrupt::vector::IPC,
            );
        }

        Ok(())
    }

    /// Send the given message to a specific virtual machine
    ///
    /// The sent message will be received by the BSP of the target
    /// virtual machine. If 'notify' is true, an interrupt will
    /// be sent to the recipient.
    pub fn send_msg(
        &self,
        msg: VirtualMachineMsg,
        vm_id: u32,
        notify: bool,
    ) -> Result<()> {
        let vm_bsp = self.bsp_core_id(vm_id).ok_or_else(|| {
            Error::InvalidValue(format!(
                "Unable to find BSP for VM id '{}'",
                vm_id
            ))
        })?;
        self.send_msg_core(msg, vm_bsp, notify)
    }

    /// Receive any pending message for the current core
    pub fn recv_msg(&self) -> Option<VirtualMachineMsg> {
        let context = self
            .context_by_core_id(percore::read_core_id())
            .expect("No VirtualMachineContext for apic id");
        context.msgqueue.write().pop_front()
    }

    /// Receive all pending messages for the current core
    pub fn recv_all_msgs(&self) -> impl Iterator<Item = VirtualMachineMsg> {
        let context = self
            .context_by_core_id(percore::read_core_id())
            .expect("No VirtualMachineContext for apic id");
        let pending_messages = context.msgqueue.write().split_off(0);
        pending_messages.into_iter()
    }
}

/// A structure to build up the set of VirtualMachines
pub struct VirtualMachineSetBuilder {
    /// The number of virtual machines added to the builder
    machine_count: u32,

    /// Mapping of core_id to VirtualMachine
    map: BTreeMap<percore::CoreId, Arc<VirtualMachine>>,
}

impl VirtualMachineSetBuilder {
    /// Returns a new VirtualMachineSetBuilder
    pub fn new() -> Self {
        Self {
            machine_count: 0,
            map: BTreeMap::new(),
        }
    }

    /// Add a VirtualMachine to the set
    pub fn insert_machine(&mut self, vm: Arc<VirtualMachine>) -> Result<()> {
        self.machine_count += 1;
        for cpu in vm.config.cpus() {
            self.map.insert(percore::CoreId::from(*cpu), vm.clone());
        }
        Ok(())
    }

    /// Get the virtual machine that owns the core with the given core id
    pub fn get_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<Arc<VirtualMachine>> {
        self.map.get(&core_id).map(|vm| vm.clone())
    }

    /// Finish building the VirtualMachineSet
    pub fn finalize(self) -> VirtualMachineSet {
        VirtualMachineSet {
            machine_count: self.machine_count,
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

/// A set of physical hardware that may be attached to a VM
#[derive(Default)]
pub struct PhysicalDeviceConfig {
    /// The physical serial connection for this VM (if any).
    pub serial: RwLock<Option<physdev::com::Uart8250>>,

    /// The physical ps2 keyboard connection for this VM (if any).
    pub ps2_keyboard: RwLock<Option<physdev::keyboard::Ps2Controller>>,
}

/// A configuration for a `VirtualMachine`
pub struct VirtualMachineConfig {
    cpus: ArrayVec<[percore::CoreId; MAX_PER_VM_CORE_COUNT]>,
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
        cpus: &[percore::CoreId],
        memory: u64,
        physical_devices: PhysicalDeviceConfig,
    ) -> Result<VirtualMachineConfig> {
        let mut cpu_array = ArrayVec::new();
        cpu_array.try_extend_from_slice(cpus)?;
        Ok(VirtualMachineConfig {
            cpus: cpu_array,
            images: vec![],
            virtual_devices: DeviceMap::default(),
            physical_devices: physical_devices,
            memory: memory,
        })
    }

    /// Specify that the given image 'path' should be mapped to the given address
    ///
    /// The precise meaning of `image` will vary by platform. On multiboot2 platforms
    /// it is a module.
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

    /// Access the configurations physical hardware
    pub fn physical_devices(&self) -> &PhysicalDeviceConfig {
        &self.physical_devices
    }

    /// Get the list of CoreIds assicated with this VM
    pub fn cpus(&self) -> &ArrayVec<[percore::CoreId; MAX_PER_VM_CORE_COUNT]> {
        &self.cpus
    }

    /// Get the CoreId of the BSP for this VM
    pub fn bsp_id(&self) -> percore::CoreId {
        self.cpus[0]
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

    /// The APIC access page
    ///
    /// See section 29.4 of the Intel software developer's manual
    pub apic_access_page: Raw4kPage,

    /// Portions of the per-core Local APIC state needed for logical addressing
    pub logical_apic_state:
        BTreeMap<percore::CoreId, virtdev::lapic::LogicalApicState>,

    /// The number of vcpus that are up and waiting to start
    cpus_ready: AtomicU32,
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
    ) -> Result<Arc<Self>> {
        let guest_space = Self::setup_ept(&config, info)?;

        // Prepare the portion of per-core local apic state that is stored at the
        // VM level (as needed for logical addressing)
        let mut logical_apic_states = BTreeMap::new();
        for core in config.cpus.iter() {
            logical_apic_states.insert(
                core.clone(),
                virtdev::lapic::LogicalApicState::default(),
            );
        }

        let vm = Arc::new(Self {
            id: id,
            config: config,
            guest_space: guest_space,
            apic_access_page: Raw4kPage([0u8; 4096]),
            logical_apic_state: logical_apic_states,
            cpus_ready: AtomicU32::new(0),
        });

        // Map the guest local apic addr to the access page. This will be set in each
        // core's vmcs
        let apic_frame = memory::HostPhysFrame::from_start_address(
            memory::HostPhysAddr::new(vm.apic_access_page.as_ptr() as u64),
        )?;
        vm.guest_space
            .map_frame(GUEST_LOCAL_APIC_ADDR, apic_frame, false)?;

        Ok(vm)
    }

    /// Notify this VirtualMachine that the current core is ready to start
    ///
    /// Each core associated with this VirtualMachine must call this method
    /// in order for 'all_cores_ready' to return true. Cores must _not_
    /// invoke this method more than once.
    pub fn notify_ready(&self) {
        self.cpus_ready
            .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    }

    /// Returns true when all VirtualMachine cores have called 'notify_ready'
    pub fn all_cores_ready(&self) -> bool {
        self.cpus_ready.load(core::sync::atomic::Ordering::SeqCst)
            == self.config.cpus.len() as u32
    }

    /// Process the given DeviceEvent on the virtual hardware matching 'ident'
    ///
    /// # Arguments
    ///
    /// * `ident` - A DeviceInteraction like a Port I/O port or address used to
    ///             find the relevant device.
    /// * `kind` - The DeviceEvent kind to dispatch
    /// * `vcpu` - A handle to the current vcpu
    /// * `responses` - A ResponseEventArray for any responses from the virtual
    ///                 hardware
    pub fn dispatch_event(
        &self,
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

        let space = crate::memory::GuestAddressSpaceView::from_vmcs(
            &vcpu.vmcs,
            &self.guest_space,
        )?;

        let event = Event::new(kind, space, responses)?;

        dev.write().on_event(event)
    }

    /// Returns an iterator of the CoreIds that are logically addressed by the given mask
    ///
    /// # Arguments
    ///
    /// * `mask` - The APIC ICR address contents (e.g., the upper 32 bits) for a
    ///            logically addressed IPC
    pub fn logical_apic_destination(
        &self,
        mask: u32,
    ) -> Result<impl Iterator<Item = &percore::CoreId>> {
        // FIXME: currently we only support the 'Flat Model' logical mode
        // (so we just ignore the destination format register). See 10.6.2.2
        // of Volume 3A of the Intel software developer's manual
        Ok(self.config.cpus.iter().filter(move |core| {
            let apic_state = self
                .logical_apic_state
                .get(core)
                .expect("Missing logical state for core");
            // TODO(alschwalm): This may not need to be as strict as SeqCst
            let destination = apic_state
                .logical_destination
                .load(core::sync::atomic::Ordering::SeqCst);
            destination & mask != 0
        }))
    }

    /// Notify the VirtualMachine of a change in the Logical Destination register
    /// for the current core
    pub fn update_core_logical_destination(&self, dest: u32) {
        let apic_state = self
            .logical_apic_state
            .get(&percore::read_core_id())
            .expect("Missing logical state for core");
        apic_state
            .logical_destination
            .store(dest, core::sync::atomic::Ordering::SeqCst);
    }

    /// Resolve a guest GSI to a specific CoreId, vector and interrupt type
    pub fn gsi_destination(
        &self,
        gsi: u32,
    ) -> Result<(percore::CoreId, u8, vcpu::InjectedInterruptType)> {
        //TODO(alschwalm): For now just route the UART interrupts to the BSP,
        // but this should ulimately do actual interrupt routing based on the
        // guest IO APICs. For now just blindly translate GSI to vector based
        // on this basic formula.
        let vector = (gsi + 48) as u8;
        if gsi == interrupt::gsi::UART {
            Ok((
                self.config.bsp_id(),
                vector,
                vcpu::InjectedInterruptType::ExternalInterrupt,
            ))
        } else {
            Ok((
                percore::read_core_id(),
                vector,
                vcpu::InjectedInterruptType::ExternalInterrupt,
            ))
        }
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
            &[percore::CoreId::from(1)],
            0,
            phys_config,
        )
        .unwrap();
        VirtualMachine::new(0, config, &info).unwrap();
    }
}
