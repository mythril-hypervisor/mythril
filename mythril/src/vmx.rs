use crate::error::{self, Error, Result};
use crate::memory::{GuestVirtAddr, Raw4kPage};
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
            asm!(
                "mov rax, cr0",
                "or rax, rdx",
                "mov cr0, rax",
                in("rdx") 0x20,
                lateout("rax") _,
                options(nomem, nostack)
            );

            // Enable vmx in CR4
            asm!(
                "mov rax, cr4",
                "or rax, rdx",
                "mov cr4, rax",
                in("rdx") VMX_ENABLE_FLAG,
                lateout("rax") _,
                options(nomem, nostack)
            );
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
            asm!(
                "vmxon [{}]",
                "pushf",
                "pop {}",
                in(reg) &vmxon_region_addr,
                lateout(reg) rflags,
            );
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
            asm!(
                "vmxoff",
                "pushf",
                "pop {}",
                lateout(reg) rflags,
            );
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
            asm!(
                "invept {}, [{}]",
                "pushfq",
                "pop {}",
                in(reg) t,
                in(reg) &val,
                lateout(reg) rflags
            );
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
            asm!(
                "invvpid {}, [{}]",
                "pushfq",
                "pop {}",
                in(reg) t,
                in(reg) &val,
                lateout(reg) rflags
            );
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
