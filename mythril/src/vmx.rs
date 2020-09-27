use crate::error::{self, Error, Result};
use crate::memory::Raw4kPage;
use alloc::boxed::Box;
use raw_cpuid::CpuId;
use x86::msr;

pub struct Vmx {
    _vmxon_region: *mut Raw4kPage,
}

impl Vmx {
    pub fn enable() -> Result<Self> {
        const VMX_ENABLE_FLAG: u32 = 1 << 13;

        let cpuid = CpuId::new();
        match cpuid.get_feature_info() {
            Some(finfo) if finfo.has_vmx() => Ok(()),
            _ => Err(Error::NotSupported),
        }?;

        unsafe {
            // Enable NE in CR0, This is fixed bit in VMX CR0
            llvm_asm!("movq %cr0, %rax; orq %rdx, %rax; movq %rax, %cr0;"
                      :
                      : "{rdx}"(0x20)
                      : "rax");

            // Enable vmx in CR4
            llvm_asm!("movq %cr4, %rax; orq %rdx, %rax; movq %rax, %cr4;"
                      :
                      : "{rdx}"(VMX_ENABLE_FLAG)
                      : "rax");
        }

        let revision_id = Self::revision();

        let vmxon_region = Box::into_raw(Box::new(Raw4kPage::default()));
        let vmxon_region_addr = vmxon_region as u64;

        // Set the revision in the vmx page
        let region_revision = vmxon_region_addr as *mut u32;
        unsafe {
            *region_revision = revision_id;
        }

        let rflags = unsafe {
            let rflags: u64;
            llvm_asm!("vmxon $1; pushfq; popq $0"
                      : "=r"(rflags)
                      : "m"(vmxon_region_addr)
                      : "rflags");
            rflags
        };

        error::check_vm_insruction(rflags, "Failed to enable vmx".into())?;
        Ok(Vmx {
            _vmxon_region: vmxon_region,
        })
    }

    pub fn disable(self) -> Result<()> {
        //TODO: this should panic when done from a different core than it
        //      was originally activated from
        let rflags = unsafe {
            let rflags: u64;
            llvm_asm!("vmxoff; pushfq; popq $0"
                      : "=r"(rflags)
                      :
                      : "rflags");
            rflags
        };

        error::check_vm_insruction(rflags, "Failed to disable vmx".into())
    }

    pub fn revision() -> u32 {
        unsafe { msr::rdmsr(msr::IA32_VMX_BASIC) as u32 }
    }

    pub fn invept(&self, mode: InvEptMode) -> Result<()> {
        let (t, val) = match mode {
            InvEptMode::SingleContext(eptp) => (1u64, eptp as u128),
            InvEptMode::GlobalContext => (2u64, 0 as u128),
        };

        let rflags = unsafe {
            let rflags: u64;
            llvm_asm!("invept $1, $2; pushfq; popq $0"
                      : "=r"(rflags)
                      : "m"(val), "r"(t));
            rflags
        };
        error::check_vm_insruction(rflags, "Failed to execute invept".into())
    }

    pub fn invvpid(&self, mode: InvVpidMode) -> Result<()> {
        let (t, val) = match mode {
            InvVpidMode::IndividualAddress(vpid, addr) => {
                (0u64, vpid as u128 | ((addr.as_u64() as u128) << 64))
            }
            InvVpidMode::SingleContext(vpid) => (1u64, vpid as u128),
            InvVpidMode::AllContext => (2u64, 0u128),
            InvVpidMode::SingleContextRetainGlobal(vpid) => {
                (3u64, vpid as u128)
            }
        };

        let rflags = unsafe {
            let rflags: u64;
            llvm_asm!("invvpid $1, $2; pushfq; popq $0"
                      : "=r"(rflags)
                      : "m"(val), "r"(t));
            rflags
        };
        error::check_vm_insruction(rflags, "Failed to execute invvpid".into())
    }
}

pub enum InvEptMode {
    SingleContext(u64),
    GlobalContext,
}

pub enum InvVpidMode {
    IndividualAddress(u16, GuestVirtAddr),
    SingleContext(u16),
    AllContext,
    SingleContextRetainGlobal(u16),
}
