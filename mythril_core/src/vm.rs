use crate::allocator::FrameAllocator;
use crate::device::EmulatedDevice;
use crate::error::{self, Error, Result};
use crate::memory::{self, GuestAddressSpace, GuestPhysAddr};
use crate::percpu;
use crate::registers::{GdtrBase, IdtrBase};
use crate::{vmcs, vmexit, vmx};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;
use x86::bits64::segmentation::{rdfsbase, rdgsbase};
use x86::controlregs::{cr0, cr3, cr4};
use x86::msr;

pub trait VmServices {
    type Allocator: FrameAllocator;
    fn allocator(&mut self) -> &mut Self::Allocator;
    fn read_file(&self, path: &str) -> Result<Vec<u8>>;
}

extern "C" {
    pub fn vmlaunch_wrapper() -> u64;
}

pub static mut VMS: percpu::PerCpu<Option<VirtualMachineRunning>> =
    percpu::PerCpu::<Option<VirtualMachineRunning>>::new();

pub struct VirtualMachineConfig {
    images: Vec<(String, GuestPhysAddr)>,
    devices: Vec<Box<dyn EmulatedDevice>>,
    memory: u64, // number of 4k pages
}

impl VirtualMachineConfig {
    pub fn new(memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            images: vec![],
            devices: vec![],
            memory: memory,
        }
    }

    pub fn load_image(&mut self, image: String, addr: GuestPhysAddr) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }

    pub fn register_device(&mut self, device: Box<dyn EmulatedDevice>) {
        self.devices.push(device);
    }
}

pub struct VirtualMachine<S>
where
    S: VmServices,
{
    vmcs: vmcs::Vmcs,
    config: VirtualMachineConfig,
    addr_space: GuestAddressSpace,
    stack: Vec<u8>,
    _services: PhantomData<S>,
}

impl<S> VirtualMachine<S>
where
    S: VmServices,
{
    pub fn new(vmx: &mut vmx::Vmx, config: VirtualMachineConfig, services: &mut S) -> Result<Self> {
        let mut vmcs = {
            let alloc = services.allocator();
            vmcs::Vmcs::new(alloc)?
        };

        // Allocate 1MB for host stack space
        let stack = vec![0u8; 1024 * 1024];

        let addr_space = vmcs.with_active_vmcs(vmx, |mut vmcs| {
            let addr_space = Self::setup_ept(&mut vmcs, &config, services)?;
            Self::initialize_host_vmcs(&mut vmcs, &stack)?;
            Self::initialize_guest_vmcs(&mut vmcs)?;
            Self::initialize_ctrl_vmcs(&mut vmcs, services)?;
            Ok(addr_space)
        })?;

        Ok(Self {
            vmcs: vmcs,
            config: config,
            stack: stack,
            addr_space: addr_space,
            _services: PhantomData,
        })
    }

    fn map_image(
        image: &str,
        addr: &GuestPhysAddr,
        space: &mut GuestAddressSpace,
        services: &mut S,
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
        vmcs: &mut vmcs::TemporaryActiveVmcs,
        config: &VirtualMachineConfig,
        services: &mut S,
    ) -> Result<GuestAddressSpace> {
        let alloc = services.allocator();
        let mut guest_space = GuestAddressSpace::new(alloc)?;

        // FIXME: For now, just map 32MB of RAM
        for i in 0..8192 {
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

        vmcs.write_field(vmcs::VmcsField::EptPointer, guest_space.eptp())?;
        Ok(guest_space)
    }

    fn initialize_host_vmcs(vmcs: &mut vmcs::TemporaryActiveVmcs, stack: &[u8]) -> Result<()> {
        //TODO: Check with MSR_IA32_VMX_CR0_FIXED0/1 that these bits are valid
        vmcs.write_field(vmcs::VmcsField::HostCr0, unsafe { cr0() }.bits() as u64)?;

        let current_cr3 = unsafe { cr3() };
        vmcs.write_field(vmcs::VmcsField::HostCr3, current_cr3)?;
        vmcs.write_field(vmcs::VmcsField::HostCr4, unsafe { cr4() }.bits() as u64)?;

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

        vmcs.write_field(vmcs::VmcsField::HostIdtrBase, IdtrBase::read())?;
        vmcs.write_field(vmcs::VmcsField::HostGdtrBase, GdtrBase::read())?;

        vmcs.write_field(vmcs::VmcsField::HostFsBase, unsafe {
            msr::rdmsr(msr::IA32_FS_BASE)
        })?;
        vmcs.write_field(vmcs::VmcsField::HostFsBase, unsafe {
            msr::rdmsr(msr::IA32_GS_BASE)
        })?;

        vmcs.write_field(vmcs::VmcsField::HostRsp, stack.as_ptr() as u64)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32Efer, unsafe {
            msr::rdmsr(msr::IA32_EFER)
        })?;

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

        let (guest_cr0, guest_cr4) = {
            let mut cr0_fixed0 = unsafe { msr::rdmsr(msr::IA32_VMX_CR0_FIXED0) };
            cr0_fixed0 &= !(1 << 0); // disable PE
            cr0_fixed0 &= !(1 << 31); // disable PG
            let mut cr4_fixed0 = unsafe { msr::rdmsr(msr::IA32_VMX_CR4_FIXED0) };

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

    fn initialize_ctrl_vmcs(vmcs: &mut vmcs::TemporaryActiveVmcs, services: &mut S) -> Result<()> {
        let alloc = services.allocator();
        vmcs.write_with_fixed(
            vmcs::VmcsField::CpuBasedVmExecControl,
            (vmcs::CpuBasedCtrlFlags::UNCOND_IO_EXITING
                | vmcs::CpuBasedCtrlFlags::ACTIVATE_MSR_BITMAP
                | vmcs::CpuBasedCtrlFlags::ACTIVATE_SECONDARY_CONTROLS)
                .bits(),
            msr::IA32_VMX_PROCBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::SecondaryVmExecControl,
            (vmcs::SecondaryExecFlags::ENABLE_EPT
                | vmcs::SecondaryExecFlags::ENABLE_VPID
                | vmcs::SecondaryExecFlags::UNRESTRICTED_GUEST)
                .bits(),
            msr::IA32_VMX_PROCBASED_CTLS2,
        )?;
        vmcs.write_field(vmcs::VmcsField::VirtualProcessorId, 1)?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::PinBasedVmExecControl,
            0,
            msr::IA32_VMX_PINBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmExitControls,
            (vmcs::VmExitCtrlFlags::IA32E_MODE
                | vmcs::VmExitCtrlFlags::LOAD_HOST_EFER
                | vmcs::VmExitCtrlFlags::SAVE_GUEST_EFER)
                .bits(),
            msr::IA32_VMX_EXIT_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmEntryControls,
            vmcs::VmEntryCtrlFlags::LOAD_GUEST_EFER.bits(),
            msr::IA32_VMX_ENTRY_CTLS,
        )?;

        let msr_bitmap = alloc.allocate_frame()?;
        vmcs.write_field(
            vmcs::VmcsField::MsrBitmap,
            msr_bitmap.start_address().as_u64(),
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
    fn find_matching_port_dev(&mut self, port: u16) -> Option<&mut Box<dyn EmulatedDevice>> {
        self.config
            .devices
            .iter_mut()
            .find(|dev| dev.services_port(port))
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
            vmexit::BasicExitReason::CrAccess => {
                let info = match exit.information {
                    Some(vmexit::ExitInformation::CrAccess(info)) => info,
                    _ => unreachable!(),
                };

                match info.cr_num {
                    3 => match info.access_type {
                        vmexit::CrAccessType::MovToCr => {
                            let reg = info.register.unwrap();
                            let val = reg.read(&self.vmcs, guest_cpu)?;
                            self.vmcs.write_field(vmcs::VmcsField::GuestCr3, val)?;
                        }
                        vmexit::CrAccessType::MovFromCr => {
                            let reg = info.register.unwrap();
                            let val = self.vmcs.read_field(vmcs::VmcsField::GuestCr3)?;
                            reg.write(val, &mut self.vmcs, guest_cpu)?;
                        }
                        _ => unreachable!(),
                    },
                    _ => return Err(Error::InvalidValue(format!("Unsupported CR number access"))),
                }

                self.skip_emulated_instruction()?;
            }

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
                self.skip_emulated_instruction()?;
            }
            vmexit::BasicExitReason::IoInstruction => {
                let (port, input, size, string) = match exit.information {
                    Some(vmexit::ExitInformation::IoInstruction(qual)) => {
                        (qual.port, qual.input, qual.size, qual.string)
                    }
                    _ => unreachable!(),
                };

                let dev = self
                    .find_matching_port_dev(port)
                    .ok_or(Error::MissingDevice(format!("No device for port {}", port)))?;

                if !string {
                    if !input {
                        let arr = (guest_cpu.rax as u32).to_be_bytes();
                        dev.on_port_write(port, &arr[..size as usize])?;
                    } else {
                        let mut out = [0u8; 4];
                        dev.on_port_read(port, &mut out[4 - size as usize..])?;
                        guest_cpu.rax &= (!guest_cpu.rax) << (size * 8);
                        guest_cpu.rax |= u32::from_be_bytes(out) as u64;
                    }
                } else {
                    if !input {
                        let linear_addr =
                            self.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
                        let guest_addr = memory::GuestVirtAddr::Paging4Level(
                            memory::Guest4LevelPagingAddr::new(linear_addr),
                        );
                        let guest_cr3 = self.vmcs.read_field(vmcs::VmcsField::GuestCr3)?;
                        let guest_cr3 = memory::GuestPhysAddr::new(guest_cr3);
                        let translated = self
                            .addr_space
                            .translate_linear_address(guest_addr, guest_cr3)?;
                        info!("vaddr = {:?}, translated = {:?}", guest_addr, translated);

                        //TODO: for now just print this so I feel accomplished
                        let frame = self.addr_space.find_host_frame(translated)?;
                        unsafe {
                            let start = u16::from(translated.offset()) as usize;
                            let data = frame.as_array()[start..]
                                .iter()
                                .cloned()
                                .take_while(|b| *b != 0)
                                .collect::<Vec<u8>>();
                            info!("GUEST0: {:?}", String::from_utf8(data).unwrap());
                        }
                    } else {
                        //TODO: INS
                    }
                }
                self.skip_emulated_instruction()?;
            }
            _ => info!("No handler for exit reason: {:?}", exit),
        }

        Ok(())
    }
}
