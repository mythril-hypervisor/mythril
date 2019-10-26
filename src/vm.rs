use crate::device::{EmulatedDevice, PortIoDevice};
use crate::error::{self, Error, Result};
use crate::memory::{self, GuestAddressSpace, GuestPhysAddr};
use crate::percpu;
use crate::registers::{self, Cr4, GdtrBase, IdtrBase};
use crate::{vmcs, vmexit, vmx};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use uefi::{self, table::boot::BootServices};
use x86_64::registers::control::{Cr0, Cr3};
use x86_64::registers::model_specific::{Efer, FsBase, GsBase, Msr};
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::{PageSize, Size4KiB};
use x86_64::structures::paging::FrameAllocator;
use x86_64::PhysAddr;

extern "C" {
    pub fn vmlaunch_wrapper() -> u64;
}

pub static mut VMS: percpu::PerCpu<Option<VirtualMachineRunning>> =
    percpu::PerCpu::<Option<VirtualMachineRunning>>::new();

pub struct VirtualMachineConfig {
    images: Vec<(String, GuestPhysAddr)>,
    port_devices: Vec<Box<dyn PortIoDevice>>,
    memory: u64, // number of 4k pages
}

impl VirtualMachineConfig {
    pub fn new(memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            images: vec![],
            port_devices: vec![],
            memory: memory,
        }
    }

    pub fn load_image(&mut self, image: String, addr: GuestPhysAddr) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }

    pub fn register_device(&mut self, device: EmulatedDevice) -> Result<()> {
        match device {
            EmulatedDevice::Port(device) => self.port_devices.push(device),
            _ => (),
        }
        Ok(())
    }
}

pub struct VirtualMachine {
    vmcs: vmcs::Vmcs,
    config: VirtualMachineConfig,
    addr_space: GuestAddressSpace,
    stack: Vec<u8>,
}

impl VirtualMachine {
    pub fn new(
        vmx: &mut vmx::Vmx,
        alloc: &mut impl FrameAllocator<Size4KiB>,
        config: VirtualMachineConfig,
        services: &BootServices,
    ) -> Result<Self> {
        let mut vmcs = vmcs::Vmcs::new(alloc)?;

        // Allocate 1MB for host stack space
        let stack = vec![0u8; 1024 * 1024];

        let addr_space = vmcs.with_active_vmcs(vmx, |mut vmcs| {
            let addr_space = Self::setup_ept(&mut vmcs, alloc, &config, services)?;
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

    //FIXME this whole function is rough
    fn read_image(services: &BootServices, path: &str) -> Result<Vec<u8>> {
        use core::mem::MaybeUninit;
        use uefi::data_types::Handle;
        use uefi::prelude::ResultExt;
        use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
        use uefi::proto::media::fs::SimpleFileSystem;

        let fs = uefi::table::boot::SearchType::from_proto::<SimpleFileSystem>();
        let num_handles = services
            .locate_handle(fs, None)
            .log_warning()
            .map_err(|_| Error::Uefi("Failed to get number of FS handles".into()))?;

        let mut volumes: Vec<Handle> =
            vec![unsafe { MaybeUninit::uninit().assume_init() }; num_handles];
        let _ = services
            .locate_handle(fs, Some(&mut volumes))
            .log_warning()
            .map_err(|_| Error::Uefi("Failed to read FS handles".into()))?;

        for volume in volumes.into_iter() {
            let proto = services
                .handle_protocol::<SimpleFileSystem>(volume)
                .log_warning()
                .map_err(|_| Error::Uefi("Failed to protocol for FS handle".into()))?;
            let fs = unsafe { proto.get().as_mut() }
                .ok_or(Error::NullPtr("FS Protocol ptr was NULL".into()))?;

            let mut root = fs
                .open_volume()
                .log_warning()
                .map_err(|_| Error::Uefi("Failed to open volume".into()))?;

            //FIXME: we should just continue on error here
            let handle = match root
                .open(path, FileMode::Read, FileAttribute::READ_ONLY)
                .log_warning()
            {
                Ok(f) => f,
                Err(_) => continue,
            };
            let file = handle
                .into_type()
                .log_warning()
                .map_err(|_| Error::Uefi(format!("Failed to convert file")))?;

            match file {
                FileType::Regular(mut f) => {
                    info!("Reading file: {}", path);
                    let mut contents = vec![];
                    let mut buff = [0u8; 1024];
                    while f
                        .read(&mut buff)
                        .log_warning()
                        .map_err(|_| Error::Uefi(format!("Failed to read file: {}", path)))?
                        > 0
                    {
                        contents.extend_from_slice(&buff);
                    }
                    return Ok(contents);
                }
                _ => return Err(Error::Uefi(format!("Image file {} was a directory", path))),
            }
        }

        Err(Error::MissingFile(format!(
            "Unable to find image file {}",
            path
        )))
    }

    fn map_image(
        image: &str,
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
        alloc: &mut impl FrameAllocator<Size4KiB>,
        services: &BootServices,
    ) -> Result<()> {
        let image = Self::read_image(services, image)?;
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
        services: &BootServices,
    ) -> Result<GuestAddressSpace> {
        let mut guest_space = GuestAddressSpace::new(alloc)?;
        for image in config.images.iter() {
            Self::map_image(&image.0, &image.1, &mut guest_space, alloc, services)?;
        }

        vmcs.write_field(vmcs::VmcsField::EptPointer, guest_space.eptp())?;
        Ok(guest_space)
    }

    fn initialize_host_vmcs(vmcs: &mut vmcs::TemporaryActiveVmcs, stack: &[u8]) -> Result<()> {
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
        vmcs.write_field(vmcs::VmcsField::HostTrSelector, 0x30)?;

        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterCs, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEsp, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEip, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::HostIdtrBase, IdtrBase::read().as_u64())?;
        vmcs.write_field(vmcs::VmcsField::HostGdtrBase, GdtrBase::read().as_u64())?;

        vmcs.write_field(vmcs::VmcsField::HostFsBase, FsBase::read().as_u64())?;
        vmcs.write_field(vmcs::VmcsField::HostGsBase, GsBase::read().as_u64())?;

        vmcs.write_field(vmcs::VmcsField::HostRsp, stack.as_ptr() as u64)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32Efer, Efer::read().bits())?;

        vmcs.write_field(
            vmcs::VmcsField::HostRip,
            vmexit::vmexit_handler_wrapper as u64,
        )?;

        Ok(())
    }

    fn initialize_guest_vmcs(vmcs: &mut vmcs::TemporaryActiveVmcs) -> Result<()> {
        vmcs.write_field(vmcs::VmcsField::GuestEsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsSelector, 0xf000)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestEsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsBase, 0xffff0000)?;
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

        vmcs.write_field(vmcs::VmcsField::GuestEsArBytes, 0x0093)?; // read/write
        vmcs.write_field(vmcs::VmcsField::GuestSsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsArBytes, 0x009b)?; // exec/read
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

            vmcs.write_field(
                vmcs::VmcsField::Cr4GuestHostMask,
                cr4_fixed0 & 0x00000000ffffffff,
            )?;

            (cr0_fixed0, cr4_fixed0)
        };

        vmcs.write_field(vmcs::VmcsField::GuestCr0, guest_cr0)?;
        vmcs.write_field(vmcs::VmcsField::GuestCr4, guest_cr4)?;
        vmcs.write_field(vmcs::VmcsField::Cr4ReadShadow, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::GuestCr3, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::GuestRip, 0xfff0)?;

        Ok(())
    }

    fn initialize_ctrl_vmcs(
        vmcs: &mut vmcs::TemporaryActiveVmcs,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<()> {
        vmcs.write_with_fixed(
            vmcs::VmcsField::CpuBasedVmExecControl,
            (vmcs::CpuBasedCtrlFlags::UNCOND_IO_EXITING
                | vmcs::CpuBasedCtrlFlags::ACTIVATE_SECONDARY_CONTROLS)
                .bits(),
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
            (vmcs::VmExitCtrlFlags::IA32E_MODE
                | vmcs::VmExitCtrlFlags::LOAD_HOST_EFER
                | vmcs::VmExitCtrlFlags::SAVE_GUEST_EFER)
                .bits(),
            registers::MSR_IA32_VMX_EXIT_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmEntryControls,
            vmcs::VmEntryCtrlFlags::LOAD_GUEST_EFER.bits(),
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

        let rflags = unsafe { vmlaunch_wrapper() };
        error::check_vm_insruction(rflags, "Failed to launch vm".into())?;

        unreachable!()
    }
}

pub struct VirtualMachineRunning {
    pub vmcs: vmcs::ActiveVmcs,
    config: VirtualMachineConfig,
    addr_space: GuestAddressSpace,
    stack: Vec<u8>,
}

impl VirtualMachineRunning {
    fn find_matching_port_dev(&mut self, port: u16) -> Option<&mut Box<dyn PortIoDevice>> {
        self.config
            .port_devices
            .iter_mut()
            .find(|dev| dev.port() == port)
    }

    fn skip_emulated_instruction(&mut self) -> Result<()> {
        let mut rip = self.vmcs.read_field(vmcs::VmcsField::GuestRip)?;
        rip += self
            .vmcs
            .read_field(vmcs::VmcsField::VmExitInstructionLen)?;
        self.vmcs.write_field(vmcs::VmcsField::GuestRip, rip)?;

        //TODO: clear interrupts?
        Ok(())
    }

    pub fn handle_vmexit(
        &mut self,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::ExitReason,
    ) -> Result<()> {
        match exit.reason {
            vmexit::BasicExitReason::CpuId => {
                //FIXME: for now just use the actual cpuid
                let res = raw_cpuid::native_cpuid::cpuid_count(
                    guest_cpu.rax as u32,
                    guest_cpu.rcx as u32,
                );
                guest_cpu.rax = res.eax as u64 | (guest_cpu.rax & 0xffffffff00000000);
                guest_cpu.rbx = res.ebx as u64 | (guest_cpu.rbx & 0xffffffff00000000);
                guest_cpu.rcx = res.ecx as u64 | (guest_cpu.rcx & 0xffffffff00000000);
                guest_cpu.rdx = res.edx as u64 | (guest_cpu.rdx & 0xffffffff00000000);
                self.skip_emulated_instruction();
            }
            vmexit::BasicExitReason::IoInstruction => {
                let (port, input, size) = match exit.information {
                    Some(vmexit::ExitInformation::IoInstruction(qual)) => {
                        (qual.port, qual.input, qual.size)
                    }
                    _ => unreachable!(),
                };

                let dev = self
                    .find_matching_port_dev(port)
                    .ok_or(Error::MissingDevice(format!("No device for port {}", port)))?;

                if !input {
                    let arr = (guest_cpu.rax as u32).to_be_bytes();
                    dev.on_write(&arr[..size as usize])?;
                } else {
                    //TODO: read
                }
                self.skip_emulated_instruction();
            }
            _ => info!("No handler for exit reason: {:?}", exit),
        }

        Ok(())
    }
}
