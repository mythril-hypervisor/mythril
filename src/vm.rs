use crate::error::{Error, Result};
use crate::vmcs;
use crate::vmx;
use alloc::vec::Vec;
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::FrameAllocator;
use x86_64::PhysAddr;

pub struct VirtualMachineConfig {
    images: Vec<(Vec<u8>, PhysAddr)>,
    memory: u64 // number of 4k pages
}

impl VirtualMachineConfig {
    pub fn new(memory: u64) -> VirtualMachineConfig {
        VirtualMachineConfig {
            images: vec![],
            memory: memory
        }
    }

    pub fn load_image(&mut self, image: Vec<u8>, guest_addr: PhysAddr) -> Result<()> {
        self.images.push((image, guest_addr));
        Ok(())
    }
}


pub struct VirtualMachine {
    vmcs: vmcs::Vmcs,
    config: VirtualMachineConfig
}

impl VirtualMachine {
    pub fn new(
        vmx: &mut vmx::Vmx,
        alloc: &mut impl FrameAllocator<Size4KiB>,
        config: VirtualMachineConfig,
    ) -> Result<Self> {
        let vmcs = vmcs::Vmcs::new(alloc)?.activate(vmx)?;

        //TODO: initialize the vmcs from the config

        let vmcs = vmcs.deactivate();

        Ok(Self {
            vmcs: vmcs,
            config: config
        })
    }

    pub fn launch(self, vmx: &mut vmx::Vmx) -> Result<VirtualMachineRunning> {
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
            return Err(Error::VmFailInvalid)
        } else if rflags.contains(RFlags::ZERO_FLAG) {
            return Err(Error::VmFailValid)
        }

        Ok(VirtualMachineRunning {
            vmcs: self.vmcs.activate(vmx)?,
        })
    }
}

pub struct VirtualMachineRunning<'a> {
    vmcs: vmcs::ActiveVmcs<'a>,
}
