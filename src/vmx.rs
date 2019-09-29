use crate::error::{Error, Result};
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};
use x86_64::structures::paging::frame::PhysFrame;

pub struct Vmx {
    vmxon_region: PhysFrame<Size4KiB>
}

impl Vmx {
    pub fn enable(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<Self> {
        const VMX_ENABLE_FLAG: u32 = 1 << 13;

        unsafe {
            // Enable NE in CR0, This is fixed bit in VMX CR0
            asm!("movq %cr0, %rax; orq %rdx, %rax; movq %rax, %cr0;"
                 :
                 :"{rdx}"(0x20)
                 :"rax");

            // Enable vmx in CR4
            asm!("movq %cr4, %rax; orq %rdx, %rax; movq %rax, %cr4;"
                 :
                 :"{rdx}"(VMX_ENABLE_FLAG)
                 :"rax");
        }

        let revision_id = Self::revision();

        let vmxon_region = alloc.allocate_frame()
            .ok_or(Error::AllocError("Failed to allocate vmxon frame"))?;
        let vmxon_region_addr = vmxon_region.start_address().as_u64();

        // Set the revision in the vmx page
        let region_revision = vmxon_region_addr as *mut u32;
        unsafe {
            *region_revision = revision_id;
        }

        let rflags = unsafe {
            let rflags: u64;
            asm!("vmxon $1; pushfq; popq $0"
                 : "=r"(rflags)
                 : "m"(vmxon_region_addr)
                 : "rflags");
            rflags::RFlags::from_bits_truncate(rflags)
        };

        // FIXME: this leaks the page on error
        if rflags.contains(RFlags::CARRY_FLAG) {
            Err(Error::VmFailInvalid)
        } else if rflags.contains(RFlags::ZERO_FLAG) {
            Err(Error::VmFailValid)
        } else {
            Ok(Vmx{
                vmxon_region: vmxon_region
            })
        }
    }

    pub fn disable(self, alloc: &mut impl FrameDeallocator<Size4KiB>) -> Result<()> {
        let rflags = unsafe {
            let rflags: u64;
            asm!("vmxoff; pushfq; popq $0"
                 : "=r"(rflags)
                 :
                 : "rflags");
            rflags::RFlags::from_bits_truncate(rflags)
        };

        if rflags.contains(RFlags::CARRY_FLAG) {
            Err(Error::VmFailInvalid)
        } else if rflags.contains(RFlags::ZERO_FLAG) {
            Err(Error::VmFailValid)
        } else {
            alloc.deallocate_frame(self.vmxon_region);
            Ok(())
        }
    }

    pub fn revision() -> u32 {
        //FIXME: this is currently returning very strange values
        // see https://software.intel.com/en-us/forums/virtualization-software-development/topic/293175
        use x86_64::registers::model_specific::Msr;
        const IA32_VMX_BASIC_MSR: u32 = 0x480;
        let vmx_basic = Msr::new(IA32_VMX_BASIC_MSR);
        unsafe { vmx_basic.read() as u32 }
    }
}
