use crate::device::EmulatedDevice;
use crate::error::{self, Error, Result};
use crate::memory::{self, GuestAddressSpace, GuestPhysAddr};
use crate::percpu;
use crate::registers::{self, Cr4, GdtrBase, IdtrBase};
use crate::vmcs;
use crate::vmx;
use alloc::vec::Vec;
use x86_64::registers::control::{Cr0, Cr3};
use x86_64::registers::model_specific::{Efer, FsBase, GsBase, Msr};
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::{PageSize, Size4KiB};
use x86_64::structures::paging::FrameAllocator;
use x86_64::PhysAddr;

pub static mut VMS: percpu::PerCpu<Option<VirtualMachineRunning>> =
    percpu::PerCpu::<Option<VirtualMachineRunning>>::new();

pub struct VirtualMachineConfig {
    start_addr: GuestPhysAddr,
    images: Vec<(Vec<u8>, GuestPhysAddr)>,
    devices: Vec<EmulatedDevice>,
    memory: u64, // number of 4k pages
}

impl VirtualMachineConfig {
    pub fn new(start_addr: GuestPhysAddr, memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            start_addr: start_addr,
            images: vec![],
            devices: vec![],
            memory: memory,
        }
    }

    pub fn load_image(&mut self, image: Vec<u8>, addr: GuestPhysAddr) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }

    pub fn register_device(&mut self, device: EmulatedDevice) -> Result<()> {
        self.devices.push(device);
        Ok(())
    }
}

pub struct VirtualMachine {
    vmcs: vmcs::Vmcs,
    config: VirtualMachineConfig,
    addr_space: GuestAddressSpace,
    stack: PhysFrame<Size4KiB>,
}

impl VirtualMachine {
    pub fn new(
        vmx: &mut vmx::Vmx,
        alloc: &mut impl FrameAllocator<Size4KiB>,
        config: VirtualMachineConfig,
    ) -> Result<Self> {
        let mut vmcs = vmcs::Vmcs::new(alloc)?;

        let stack = alloc
            .allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate VM stack"))?;

        let addr_space = vmcs.with_active_vmcs(vmx, |mut vmcs| {
            let addr_space = Self::setup_ept(&mut vmcs, alloc, &config)?;
            Self::initialize_host_vmcs(&mut vmcs, &stack)?;
            Self::initialize_guest_vmcs(&mut vmcs)?;
            Self::initialize_ctrl_vmcs(&mut vmcs, alloc)?;
            Ok(addr_space)
        })?;

        Ok(Self {
            vmcs: vmcs,
            config: config,
            stack: stack,
            addr_space: addr_space,
        })
    }

    fn map_image(
        image: &Vec<u8>,
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<()> {
        for (i, chunk) in image.chunks(Size4KiB::SIZE as usize).enumerate() {
            let mut host_frame = alloc
                .allocate_frame()
                .expect("Failed to allocate host frame");

            let frame_ptr = host_frame.start_address().as_u64() as *mut u8;
            let chunk_ptr = chunk.as_ptr();
            unsafe {
                core::ptr::copy_nonoverlapping(chunk_ptr, frame_ptr, chunk.len());
            }

            space.map_frame(
                alloc,
                memory::GuestPhysAddr::new(addr.as_u64() + (i as u64 * Size4KiB::SIZE) as u64),
                host_frame,
                false,
            )?;
        }
        Ok(())
    }

    fn setup_ept(
        vmcs: &mut vmcs::TemporaryActiveVmcs,
        alloc: &mut impl FrameAllocator<Size4KiB>,
        config: &VirtualMachineConfig,
    ) -> Result<GuestAddressSpace> {
        let mut guest_space = GuestAddressSpace::new(alloc)?;
        for image in config.images.iter() {
            Self::map_image(&image.0, &image.1, &mut guest_space, alloc)?;
        }

        vmcs.write_field(vmcs::VmcsField::EptPointer, guest_space.eptp())?;
        Ok(guest_space)
    }

    fn initialize_host_vmcs(
        vmcs: &mut vmcs::TemporaryActiveVmcs,
        stack: &PhysFrame<Size4KiB>,
    ) -> Result<()> {
        //TODO: Check with MSR_IA32_VMX_CR0_FIXED0/1 that these bits are valid
        vmcs.write_field(vmcs::VmcsField::HostCr0, Cr0::read().bits())?;

        let current_cr3 = Cr3::read();
        vmcs.write_field(
            vmcs::VmcsField::HostCr3,
            current_cr3.0.start_address().as_u64() | current_cr3.1.bits(),
        )?;
        vmcs.write_field(vmcs::VmcsField::HostCr4, Cr4::read())?;

        vmcs.write_field(vmcs::VmcsField::HostEsSelector, 0x00)?;

        //FIXME: The segment selector values are valid for OVMF specifically
        vmcs.write_field(vmcs::VmcsField::HostCsSelector, 0x38)?;
        vmcs.write_field(vmcs::VmcsField::HostSsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostDsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostEsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostFsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostGsSelector, 0x30)?;
        //vmcs.write_field(vmcs::VmcsField::HostTrSelector, 0x)?;

        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterCs, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEsp, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEip, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::HostIdtrBase, IdtrBase::read().as_u64())?;
        vmcs.write_field(vmcs::VmcsField::HostGdtrBase, GdtrBase::read().as_u64())?;

        vmcs.write_field(vmcs::VmcsField::HostFsBase, FsBase::read().as_u64())?;
        vmcs.write_field(vmcs::VmcsField::HostGsBase, GsBase::read().as_u64())?;

        vmcs.write_field(vmcs::VmcsField::HostRsp, stack.start_address().as_u64())?;
        vmcs.write_field(vmcs::VmcsField::HostIa32Efer, Efer::read().bits())?;

        vmcs.write_field(vmcs::VmcsField::HostRip, vmx::vmexit_handler_wrapper as u64)?;

        Ok(())
    }

    fn initialize_guest_vmcs(vmcs: &mut vmcs::TemporaryActiveVmcs) -> Result<()> {
        vmcs.write_field(vmcs::VmcsField::GuestEsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestEsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestIdtrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGdtrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestEsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestIdtrLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestGdtrLimit, 0xffff)?;

        vmcs.write_field(vmcs::VmcsField::GuestEsArBytes, 0xc093)?; // read/write
        vmcs.write_field(vmcs::VmcsField::GuestSsArBytes, 0xc093)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsArBytes, 0xc093)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsArBytes, 0xc093)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsArBytes, 0xc093)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsArBytes, 0xc09b)?; // exec/read

        vmcs.write_field(vmcs::VmcsField::GuestLdtrArBytes, 0x0082)?; // LDT
        vmcs.write_field(vmcs::VmcsField::GuestTrArBytes, 0x008b)?; // TSS (busy)

        vmcs.write_field(vmcs::VmcsField::GuestInterruptibilityInfo, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestActivityState, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDr7, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestRsp, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestRflags, 1 << 1)?; // Reserved rflags

        vmcs.write_field(vmcs::VmcsField::VmcsLinkPointer, 0xffffffff)?;
        vmcs.write_field(vmcs::VmcsField::VmcsLinkPointerHigh, 0xffffffff)?;

        //TODO: get actual EFER (use MSR for vt-x v1)
        vmcs.write_field(vmcs::VmcsField::GuestIa32Efer, 0x00)?;

        let (guest_cr0, guest_cr4) = unsafe {
            let mut cr0_fixed0 = Msr::new(registers::MSR_IA32_VMX_CR0_FIXED0).read();
            cr0_fixed0 &= !(1 << 0); // disable PE
            cr0_fixed0 &= !(1 << 31); // disable PG
            let cr4_fixed0 = Msr::new(registers::MSR_IA32_VMX_CR4_FIXED0).read();
            (cr0_fixed0, cr4_fixed0)
        };
        vmcs.write_field(vmcs::VmcsField::GuestCr0, guest_cr0)?;
        vmcs.write_field(vmcs::VmcsField::GuestCr4, guest_cr4)?;

        vmcs.write_field(vmcs::VmcsField::GuestCr3, 0x00)?;

        //TODO: set to a value from the config
        vmcs.write_field(vmcs::VmcsField::GuestRip, 0x1000)?;

        Ok(())
    }

    fn initialize_ctrl_vmcs(
        vmcs: &mut vmcs::TemporaryActiveVmcs,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<()> {
        vmcs.write_with_fixed(
            vmcs::VmcsField::CpuBasedVmExecControl,
            vmcs::CpuBasedCtrlFlags::ACTIVATE_SECONDARY_CONTROLS.bits(),
            registers::MSR_IA32_VMX_PROCBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::SecondaryVmExecControl,
            (vmcs::SecondaryExecFlags::ENABLE_EPT
                | vmcs::SecondaryExecFlags::ENABLE_VPID
                | vmcs::SecondaryExecFlags::UNRESTRICTED_GUEST)
                .bits(),
            registers::MSR_IA32_VMX_PROCBASED_CTLS2,
        )?;
        vmcs.write_field(vmcs::VmcsField::VirtualProcessorId, 1)?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::PinBasedVmExecControl,
            0,
            registers::MSR_IA32_VMX_PINBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmExitControls,
            (vmcs::VmExitCtrlFlags::IA32E_MODE | vmcs::VmExitCtrlFlags::LOAD_HOST_EFER).bits(),
            registers::MSR_IA32_VMX_EXIT_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmEntryControls,
            0,
            registers::MSR_IA32_VMX_ENTRY_CTLS,
        )?;

        vmcs.write_field(vmcs::VmcsField::ExceptionBitmap, 0xffffffff)?;

        let field = vmcs.read_field(vmcs::VmcsField::CpuBasedVmExecControl)?;
        info!("Flags: 0x{:x}", field);
        let flags = vmcs::CpuBasedCtrlFlags::from_bits_truncate(field);
        info!("Flags: {:?}", flags);

        let field = vmcs.read_field(vmcs::VmcsField::SecondaryVmExecControl)?;
        info!("Sec Flags: 0x{:x}", field);
        let flags = vmcs::SecondaryExecFlags::from_bits_truncate(field);
        info!("Sec Flags: {:?}", flags);

        vmcs.write_field(vmcs::VmcsField::Cr3TargetCount, 0)?;
        vmcs.write_field(vmcs::VmcsField::TprThreshold, 0)?;

        Ok(())
    }

    pub fn launch(self, vmx: vmx::Vmx) -> Result<!> {
        unsafe {
            VMS.set(Some(VirtualMachineRunning {
                vmcs: self.vmcs.activate(vmx)?,
                config: self.config,
                addr_space: self.addr_space,
                stack: self.stack,
            }));
        }

        let rflags = unsafe {
            let rflags: u64;
            asm!("vmlaunch; pushfq; popq $0"
                 : "=r"(rflags)
                 :: "rflags"
                 : "volatile");
            rflags
        };

        error::check_vm_insruction(rflags, "Failed to launch vm".into())?;

        unreachable!()
    }
}

pub struct VirtualMachineRunning {
    pub vmcs: vmcs::ActiveVmcs,
    config: VirtualMachineConfig,
    addr_space: GuestAddressSpace,
    stack: PhysFrame<Size4KiB>,
}
