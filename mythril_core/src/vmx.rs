use crate::allocator::FrameAllocator;
use crate::error::{self, Error, Result};
use crate::memory::HostPhysFrame;
use raw_cpuid::CpuId;
use x86::msr;

pub struct Vmx {
    vmxon_region: HostPhysFrame,
}

impl Vmx {
    pub fn enable(alloc: &mut impl FrameAllocator) -> Result<Self> {
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

        let vmxon_region = alloc.allocate_frame()?;
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

        match error::check_vm_insruction(rflags, "Failed to enable vmx".into()) {
            Ok(_) => Ok(Vmx {
                vmxon_region: vmxon_region,
            }),
            Err(e) => {
                alloc.deallocate_frame(vmxon_region).unwrap_or_else(|_| {
                    info!("Failed to deallocate vmxon region");
                });
                Err(e)
            }
        }
    }

    pub fn disable(self, alloc: &mut impl FrameAllocator) -> Result<()> {
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
        alloc.deallocate_frame(self.vmxon_region)
    }

    pub fn revision() -> u32 {
        unsafe { msr::rdmsr(msr::IA32_VMX_BASIC) as u32 }
    }
}
