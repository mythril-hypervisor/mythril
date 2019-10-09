use crate::error::{self, Error, Result};
use raw_cpuid::CpuId;
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

extern "C" {
    pub fn vmexit_handler_wrapper();
}

#[no_mangle]
pub extern "C" fn vmexit_handler() {
    info!("reached vmexit handler");
    loop {}
}

pub struct Vmx {
    vmxon_region: PhysFrame<Size4KiB>,
}

impl Vmx {
    pub fn enable(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<Self> {
        const VMX_ENABLE_FLAG: u32 = 1 << 13;

        let cpuid = CpuId::new();
        match cpuid.get_feature_info() {
            Some(finfo) if finfo.has_vmx() => Ok(()),
            _ => Err(Error::NotSupported),
        }?;

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

        let vmxon_region = alloc
            .allocate_frame()
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
            rflags
        };

        // FIXME: this leaks the page on error
        error::check_vm_insruction(rflags, "Failed to enable vmx".into())?;
        Ok(Vmx {
            vmxon_region: vmxon_region,
        })
    }

    pub fn disable(self, alloc: &mut impl FrameDeallocator<Size4KiB>) -> Result<()> {
        //TODO: this should panic when done from a different core than it
        //      was originally activated from
        let rflags = unsafe {
            let rflags: u64;
            asm!("vmxoff; pushfq; popq $0"
                 : "=r"(rflags)
                 :
                 : "rflags");
            rflags
        };

        error::check_vm_insruction(rflags, "Failed to disable vmx".into())?;
        alloc.deallocate_frame(self.vmxon_region);
        Ok(())
    }

    pub fn revision() -> u32 {
        //FIXME: this is currently returning very strange values
        // see https://software.intel.com/en-us/forums/virtualization-software-development/topic/293175
        use crate::registers::MSR_IA32_VMX_BASIC;
        use x86_64::registers::model_specific::Msr;
        let vmx_basic = Msr::new(MSR_IA32_VMX_BASIC);
        unsafe { vmx_basic.read() as u32 }
    }
}
