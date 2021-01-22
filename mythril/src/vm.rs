#![deny(missing_docs)]

use crate::apic;
use crate::boot_info::BootInfo;
use crate::error::{Error, Result};
use crate::interrupt;
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
use arraydeque::ArrayDeque;
use arrayvec::ArrayVec;
use core::default::Default;
use core::mem;
use core::pin::Pin;
use core::sync::atomic::AtomicU32;
use spin::RwLock;

static BIOS_BLOB: &'static [u8] = include_bytes!("blob/bios.bin");

// TODO(alschwalm): this should always be reported by the relevant MSR
/// The location of the local apic in the guest address space
pub const GUEST_LOCAL_APIC_ADDR: GuestPhysAddr = GuestPhysAddr::new(0xfee00000);

/// The maximum number of VirtualMachines that can be defined by a user
pub const MAX_VM_COUNT: usize = 64;

/// The maximum numer of cores that can be assigned to a single VM
pub const MAX_PER_VM_CORE_COUNT: usize = 32;

/// The maximum number of VCpus that can be defined
pub const MAX_VCPU_COUNT: usize = MAX_VM_COUNT * MAX_PER_VM_CORE_COUNT;

static mut VIRTUAL_MACHINE_SET: VirtualMachineSet = VirtualMachineSet::new();

const MAX_DYNAMIC_VIRTUAL_DEVICES: usize = 32;

const MAX_PENDING_MSG: usize = 100;

const MAX_IMAGE_MAPPING_PER_VM: usize = 16;

/// Initialize the global VirtualMachineSet
///
/// This method must be called before calling 'virtual_machines'
pub unsafe fn init_virtual_machines(
    machines: impl Iterator<Item = VirtualMachine>,
) -> Result<()> {
    for machine in machines {
        VIRTUAL_MACHINE_SET.insert(machine)?;
    }
    Ok(())
}

/// Get the global VirtualMachineSet
pub fn virtual_machines() -> &'static VirtualMachineSet {
    unsafe { &VIRTUAL_MACHINE_SET }
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
    core_id: percore::CoreId,

    vm: Pin<&'static VirtualMachine>,

    /// The per-core RX message queue
    msgqueue: RwLock<ArrayDeque<[VirtualMachineMsg; MAX_PENDING_MSG]>>,
}

/// The set of configured virtual machines
///
/// This structure represents the set of all configured machines
/// in the hypervisor and can be used to transmit and receive
/// inter-vm or inter-core messages.
pub struct VirtualMachineSet {
    contexts: ArrayVec<[VirtualMachineContext; MAX_VCPU_COUNT]>,
    vms: ArrayVec<[VirtualMachine; MAX_VM_COUNT]>,
}

impl VirtualMachineSet {
    /// Create a new VirtualMachineSet
    const fn new() -> Self {
        Self {
            contexts: ArrayVec::<[VirtualMachineContext; MAX_VCPU_COUNT]>::new(
            ),
            vms: ArrayVec::<[VirtualMachine; MAX_VM_COUNT]>::new(),
        }
    }

    /// Add a VirtualMachine to this set
    pub fn insert(&'static mut self, vm: VirtualMachine) -> Result<()> {
        // Push the VM into our static set. After this, we can do any late phase
        // initialization of VM state, as it can never move again.
        self.vms.push(vm);

        let idx = self.vms.len() - 1;
        let vm = &mut self.vms[idx];

        // Register all the static devices with the virtual device map
        for dev in vm.static_virtual_devices.devices() {
            vm.virtual_device_map.register_device(dev)?;
        }

        // Register all the dynamic devices as well
        for dev in vm.dynamic_virtual_devices.iter() {
            vm.virtual_device_map.register_device(dev)?;
        }

        // Initialize the per-VM local apic access page
        Pin::static_ref(vm).setup_guest_local_apic_page()?;

        // Create the communication queues
        for cpu in vm.cpus.iter() {
            self.contexts.push(VirtualMachineContext {
                core_id: *cpu,
                vm: Pin::static_ref(vm),
                msgqueue: RwLock::new(ArrayDeque::new()),
            })
        }

        Ok(())
    }

    fn context_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<&VirtualMachineContext> {
        for context in self.contexts.iter() {
            if context.core_id == core_id {
                return Some(context);
            }
        }
        None
    }

    fn context_by_vm_id(&self, id: u32) -> Option<&VirtualMachineContext> {
        self.contexts
            .iter()
            .filter_map(|context| {
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
        self.vms.len() as u32
    }

    /// Returns whether a given CoreId is associated with any VM
    pub fn is_assigned_core_id(&self, core_id: percore::CoreId) -> bool {
        self.context_by_core_id(core_id).is_some()
    }

    /// Get the virtual machine that owns the core with the given core id
    ///
    /// This method is unsafe as it should almost certainly not be used (use message
    /// passing instead of directly accessing the remote VM).
    pub unsafe fn get_by_core_id(
        &self,
        core_id: percore::CoreId,
    ) -> Option<Pin<&'static VirtualMachine>> {
        self.context_by_core_id(core_id).map(|context| context.vm)
    }

    /// Get a VirtualMachine by its vmid
    pub fn get_by_vm_id(
        &self,
        vmid: u32,
    ) -> Option<Pin<&'static VirtualMachine>> {
        self.context_by_vm_id(vmid).map(|context| context.vm)
    }

    /// Get the CoreId for the BSP core of a VM by its vmid
    pub fn bsp_core_id(&self, vmid: u32) -> Option<percore::CoreId> {
        self.get_by_vm_id(vmid).map(|vm| vm.bsp_id())
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
            error!("RX queue is full for core_id = {}", core_id);
            Error::InvalidValue
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
            error!("Unable to find BSP for VM id '{}'", vm_id);
            Error::InvalidValue
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

/// Emulated versions of the devices always presented to a guest
pub struct StaticVirtualDevices {
    acpi_runtime: RwLock<virtdev::acpi::AcpiRuntime>,
    vga_controller: RwLock<virtdev::vga::VgaController>,
    pci_root: RwLock<virtdev::pci::PciRootComplex>,
    pic: RwLock<virtdev::pic::Pic8259>,
    keyboard: RwLock<virtdev::keyboard::Keyboard8042>,
    pit: RwLock<virtdev::pit::Pit8254>,
    rtc: RwLock<virtdev::rtc::CmosRtc>,

    // TODO(alschwalm): In reality the number of ioapics is variable,
    // but for now just have one in here
    io_apic: RwLock<virtdev::ioapic::IoApic>,
}

impl StaticVirtualDevices {
    fn new(config: &VirtualMachineConfig) -> Result<Self> {
        Ok(Self {
            acpi_runtime: RwLock::new(virtdev::acpi::AcpiRuntime::new(0x600)?),
            vga_controller: RwLock::new(virtdev::vga::VgaController::new()?),
            pci_root: RwLock::new(virtdev::pci::PciRootComplex::new()?),
            pic: RwLock::new(virtdev::pic::Pic8259::new()?),
            keyboard: RwLock::new(virtdev::keyboard::Keyboard8042::new()?),
            pit: RwLock::new(virtdev::pit::Pit8254::new()?),
            rtc: RwLock::new(virtdev::rtc::CmosRtc::new(config.memory)?),
            io_apic: RwLock::new(virtdev::ioapic::IoApic::new()?),
        })
    }

    fn devices(
        &self,
    ) -> impl Iterator<Item = &RwLock<dyn virtdev::EmulatedDevice>> {
        core::array::IntoIter::new([
            &self.acpi_runtime as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.vga_controller as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.pci_root as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.pic as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.keyboard as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.pit as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.rtc as &RwLock<dyn virtdev::EmulatedDevice>,
            &self.io_apic as &RwLock<dyn virtdev::EmulatedDevice>,
        ])
    }
}

/// A set of physical hardware that may be attached to a VM
#[derive(Default)]
pub struct HostPhysicalDevices {
    /// The physical serial connection for this VM (if any).
    pub serial: RwLock<Option<physdev::com::Uart8250>>,

    /// The physical ps2 keyboard connection for this VM (if any).
    pub ps2_keyboard: RwLock<Option<physdev::keyboard::Ps2Controller>>,
}

/// A configuration for a `VirtualMachine`
pub struct VirtualMachineConfig {
    /// The cores assigned as part of this configuration
    pub cpus: ArrayVec<[percore::CoreId; MAX_PER_VM_CORE_COUNT]>,

    /// The images that will be mapped into the address space of this virtual machine
    pub images: ArrayVec<[(String, GuestPhysAddr); MAX_IMAGE_MAPPING_PER_VM]>,

    /// The 'dnyamic' virtual devices assigned to this virtual machine
    pub virtual_devices: ArrayVec<
        [RwLock<virtdev::DynamicVirtualDevice>; MAX_DYNAMIC_VIRTUAL_DEVICES],
    >,

    /// The host physical devices assigned to this virtual machine
    pub host_devices: HostPhysicalDevices,

    /// The size of this machines physical address space in MiB
    pub memory: u64,
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
        physical_devices: HostPhysicalDevices,
    ) -> Result<VirtualMachineConfig> {
        let mut cpu_array = ArrayVec::new();
        cpu_array.try_extend_from_slice(cpus)?;
        Ok(VirtualMachineConfig {
            cpus: cpu_array,
            images: ArrayVec::new(),
            virtual_devices: ArrayVec::new(),
            host_devices: physical_devices,
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
}

/// A virtual machine
pub struct VirtualMachine {
    /// The numeric ID of this virtual machine
    pub id: u32,

    /// The cores allocated to this virtual machine
    pub cpus: ArrayVec<[percore::CoreId; MAX_PER_VM_CORE_COUNT]>,

    /// Size of guest physical memory in MB
    pub memory: u64,

    /// The set of host physical devices available to this guest
    pub host_devices: HostPhysicalDevices,

    /// Virtual devices that are not part of guest core platform
    pub dynamic_virtual_devices: ArrayVec<
        [RwLock<virtdev::DynamicVirtualDevice>; MAX_DYNAMIC_VIRTUAL_DEVICES],
    >,

    /// A mapping of Port I/O and guest address ranges to virtual hardware
    pub virtual_device_map: DeviceMap<'static>,

    /// The virtual devices required to be presented to the guests
    pub static_virtual_devices: StaticVirtualDevices,

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
    ) -> Result<Self> {
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

        let static_devices = StaticVirtualDevices::new(&config)?;

        Ok(Self {
            id: id,
            host_devices: config.host_devices,
            cpus: config.cpus,
            memory: config.memory,
            dynamic_virtual_devices: config.virtual_devices,
            static_virtual_devices: static_devices,
            virtual_device_map: virtdev::DeviceMap::default(),
            guest_space: guest_space,
            apic_access_page: Raw4kPage([0u8; 4096]),
            logical_apic_state: logical_apic_states,
            cpus_ready: AtomicU32::new(0),
        })
    }

    /// Get the CoreId of the BSP for this VM
    pub fn bsp_id(&self) -> percore::CoreId {
        self.cpus[0]
    }

    /// Setup the local APIC access page for the guest. This _must_ be called
    /// only once the VirtualMachine is in a final location.
    pub fn setup_guest_local_apic_page(self: Pin<&'static Self>) -> Result<()> {
        // Map the guest local apic addr to the access page. This will be set in each
        // core's vmcs
        let apic_frame = memory::HostPhysFrame::from_start_address(
            memory::HostPhysAddr::new(self.apic_access_page.as_ptr() as u64),
        )?;
        self.guest_space
            .map_frame(GUEST_LOCAL_APIC_ADDR, apic_frame, false)?;

        Ok(())
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
            == self.cpus.len() as u32
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
        let dev = match self.virtual_device_map.find_device(ident) {
            Some(dev) => dev,
            None => {
                // TODO(alschwalm): port operations can produce GP faults
                return match kind {
                    DeviceEvent::PortRead(_, mut req) => {
                        // Port reads from unknown devices return 0
                        req.copy_from_u32(0);
                        Ok(())
                    }
                    DeviceEvent::PortWrite(_, _) => {
                        // Just ignore writes to unknown ports
                        Ok(())
                    }
                    _ => Err(Error::MissingDevice(
                        "Unable to dispatch event".into(),
                    )),
                };
            }
        };

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
        Ok(self.cpus.iter().filter(move |core| {
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
                self.bsp_id(),
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
                error!("No such module '{}'", image);
                Error::InvalidValue
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
                Ok(_) | Err(Error::DuplicateMapping) => continue,
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
        let host_devices = HostPhysicalDevices::default();

        let config = VirtualMachineConfig::new(
            &[percore::CoreId::from(1)],
            32,
            host_devices,
        )
        .unwrap();

        VirtualMachine::new(0, config, &info).unwrap();
    }
}
