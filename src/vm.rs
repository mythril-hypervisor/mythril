use crate::error::{Error, Result};
use crate::memory::GuestPhysAddr;
use crate::registers::{self, Cr4, GdtrBase, IdtrBase};
use crate::vmcs;
use crate::vmx;
use alloc::vec::Vec;
use x86_64::registers::control::Cr0;
use x86_64::registers::model_specific::{Efer, FsBase, GsBase, Msr};
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::FrameAllocator;
use x86_64::PhysAddr;

pub struct VirtualMachineConfig {
    images: Vec<(Vec<u8>, GuestPhysAddr)>,
    memory: u64, // number of 4k pages
}

impl VirtualMachineConfig {
    pub fn new(start_addr: GuestPhysAddr, memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            images: vec![],
            memory: memory,
        }
    }

    pub fn load_image(&mut self, image: Vec<u8>, addr: GuestPhysAddr) -> Result<()> {
        self.images.push((image, addr));
        Ok(())
    }
}

pub struct VirtualMachine {
    vmcs: vmcs::Vmcs,
    config: VirtualMachineConfig,
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

        vmcs.with_active_vmcs(vmx, |mut vmcs| {
            Self::initialize_host_vmcs(&mut vmcs, &stack)?;
            Self::initialize_guest_vmcs(&mut vmcs)?;
            Self::initialize_ctrl_vmcs(&mut vmcs)?;
            Ok(())
        })?;

        Ok(Self {
            vmcs: vmcs,
            config: config,
            stack: stack,
        })
    }

    fn initialize_host_vmcs(
        vmcs: &mut vmcs::TemporaryActiveVmcs,
        stack: &PhysFrame<Size4KiB>,
    ) -> Result<()> {
        let cr0_fixed = Msr::new(registers::IA32_VMX_CR0_FIXED0_MSR);
        let cr4_fixed = Msr::new(registers::IA32_VMX_CR4_FIXED0_MSR);

        let (host_cr0, host_cr4) = unsafe {
            (
                cr0_fixed.read() | Cr0::read().bits(),
                cr4_fixed.read() | Cr4::read(),
            )
        };

        vmcs.write_field(vmcs::VmcsField::HostCr0, host_cr0)?;
        vmcs.write_field(vmcs::VmcsField::HostCr4, host_cr4)?;

        vmcs.write_field(vmcs::VmcsField::HostEsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostCsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostSsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostDsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostFsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostGsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostTrSelector, 0x00)?;

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

        vmcs.write_field(vmcs::VmcsField::VmcsLinkPointer, 0xffffffff)?;
        vmcs.write_field(vmcs::VmcsField::VmcsLinkPointerHigh, 0xffffffff)?;

        //TODO: get actual EFER (use MSR for vt-x v1)
        vmcs.write_field(vmcs::VmcsField::GuestIa32Efer, 0x00)?;

        let (cr0_fixed, cr4_fixed) = unsafe {
            (
                Msr::new(registers::IA32_VMX_CR0_FIXED0_MSR).read(),
                Msr::new(registers::IA32_VMX_CR4_FIXED0_MSR).read(),
            )
        };

        vmcs.write_field(vmcs::VmcsField::GuestCr4, cr4_fixed)?;

        //TODO: start in real mode? (so clear PE bit)
        vmcs.write_field(vmcs::VmcsField::GuestCr0, cr0_fixed)?;
        vmcs.write_field(vmcs::VmcsField::GuestCr3, 0x00)?;

        //TODO: set to a value from the config
        vmcs.write_field(vmcs::VmcsField::GuestRip, 0x00)?;

        Ok(())
    }

    fn initialize_ctrl_vmcs(vmcs: &mut vmcs::TemporaryActiveVmcs) -> Result<()> {
        Ok(())
    }

    pub fn launch(self, vmx: vmx::Vmx) -> Result<!> {
        // TODO: make this and store it in a per-cpu variable
        // Ok(VirtualMachineRunning {
        //     vmcs: self.vmcs.activate(vmx)?,
        // })

        let rflags = unsafe {
            let rflags: u64;
            asm!("vmlaunch; pushfq; popq $0"
                 : "=r"(rflags)
                 :: "rflags"
                 : "volatile");
            rflags
        };

        let rflags = rflags::RFlags::from_bits_truncate(rflags);

        if rflags.contains(RFlags::CARRY_FLAG) {
            return Err(Error::VmFailInvalid);
        } else if rflags.contains(RFlags::ZERO_FLAG) {
            return Err(Error::VmFailValid);
        }

        panic!("Failed to launch the vm!")
    }
}

pub struct VirtualMachineRunning {
    vmcs: vmcs::ActiveVmcs,
}
