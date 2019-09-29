use crate::error::{Error, Result};
use x86_64::registers::rflags;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::page::Size4KiB;
use x86_64::structures::paging::FrameAllocator;
use x86_64::PhysAddr;

pub const HOST_ES_SELECTOR: u32 = 0x00000C00;
pub const HOST_CS_SELECTOR: u32 = 0x00000C02;
pub const HOST_SS_SELECTOR: u32 = 0x00000C04;
pub const HOST_DS_SELECTOR: u32 = 0x00000C06;
pub const HOST_FS_SELECTOR: u32 = 0x00000C08;
pub const HOST_GS_SELECTOR: u32 = 0x00000C0A;
pub const HOST_TR_SELECTOR: u32 = 0x00000C0C;
pub const HOST_IA32_PAT_FULL: u32 = 0x00002C00;
pub const HOST_IA32_EFER_FULL: u32 = 0x00002C02;
pub const HOST_IA32_EFER_HIGH: u32 = 0x00002C03;
pub const HOST_IA32_PERF_GLOBAL_CTRL_FULL: u32 = 0x00002C04;
pub const HOST_IA32_PERF_GLOBAL_CTRL_HIGH: u32 = 0x00002C05;
pub const HOST_IA32_SYSENTER_CS: u32 = 0x00004C00;
pub const HOST_CR0: u32 = 0x00006C00;
pub const HOST_CR3: u32 = 0x00006C02;
pub const HOST_CR4: u32 = 0x00006C04;
pub const HOST_FS_BASE: u32 = 0x00006C06;
pub const HOST_GS_BASE: u32 = 0x00006C08;
pub const HOST_TR_BASE: u32 = 0x00006C0A;
pub const HOST_GDTR_BASE: u32 = 0x00006C0C;
pub const HOST_IDTR_BASE: u32 = 0x00006C0E;
pub const HOST_IA32_SYSENTER_ESP: u32 = 0x00006C10;
pub const HOST_IA32_SYSENTER_EIP: u32 = 0x00006C12;
pub const HOST_RSP: u32 = 0x00006C14;
pub const HOST_RIP: u32 = 0x00006C16;
pub const HOST_IA32_EFER: u32 = 0x00002C02;
// // Appendix B.1.2
pub const GUEST_ES_SELECTOR: u32 = 0x00000800;
pub const GUEST_CS_SELECTOR: u32 = 0x00000802;
pub const GUEST_SS_SELECTOR: u32 = 0x00000804;
pub const GUEST_DS_SELECTOR: u32 = 0x00000806;
pub const GUEST_FS_SELECTOR: u32 = 0x00000808;
pub const GUEST_GS_SELECTOR: u32 = 0x0000080A;
pub const GUEST_LDTR_SELECTOR: u32 = 0x0000080C;
pub const GUEST_TR_SELECTOR: u32 = 0x0000080E;
pub const GUEST_INTERRUPT_STATUS: u32 = 0x00000810;
pub const GUEST_PML_INDEX: u32 = 0x00000812;

// // Appendix B.4.3
pub const GUEST_CR0: u32 = 0x00006800;
pub const GUEST_CR3: u32 = 0x00006802;
pub const GUEST_CR4: u32 = 0x00006804;
pub const GUEST_ES_BASE: u32 = 0x00006806;
pub const GUEST_CS_BASE: u32 = 0x00006808;
pub const GUEST_SS_BASE: u32 = 0x0000680A;
pub const GUEST_DS_BASE: u32 = 0x0000680C;
pub const GUEST_FS_BASE: u32 = 0x0000680E;
pub const GUEST_GS_BASE: u32 = 0x00006810;
pub const GUEST_LDTR_BASE: u32 = 0x00006812;
pub const GUEST_TR_BASE: u32 = 0x00006814;
pub const GUEST_GDTR_BASE: u32 = 0x00006816;
pub const GUEST_IDTR_BASE: u32 = 0x00006818;
pub const GUEST_DR7: u32 = 0x0000681A;
pub const GUEST_RSP: u32 = 0x0000681C;
pub const GUEST_RIP: u32 = 0x0000681E;
pub const GUEST_RFLAG: u32 = 0x00006820;
pub const GUEST_PENDING_DEBUG_EXCEPTION: u32 = 0x00006822;
pub const GUEST_IA32_SYSENTER_ESP: u32 = 0x00006824;
pub const GUEST_IA32_SYSENTER_EIP: u32 = 0x00006826;
pub const GUEST_IA32_EFER: u32 = 0x00002806;

// // Appendix B.3.3
pub const GUEST_ES_LIMIT: u32 = 0x00004800;
pub const GUEST_CS_LIMIT: u32 = 0x00004802;
pub const GUEST_SS_LIMIT: u32 = 0x00004804;
pub const GUEST_DS_LIMIT: u32 = 0x00004806;
pub const GUEST_FS_LIMIT: u32 = 0x00004808;
pub const GUEST_GS_LIMIT: u32 = 0x0000480A;
pub const GUEST_LDTR_LIMIT: u32 = 0x0000480C;
pub const GUEST_TR_LIMIT: u32 = 0x0000480E;
pub const GUEST_GDTR_LIMIT: u32 = 0x00004810;
pub const GUEST_IDTR_LIMIT: u32 = 0x00004812;
pub const GUEST_ES_ACCESS_RIGHT: u32 = 0x00004814;
pub const GUEST_CS_ACCESS_RIGHT: u32 = 0x00004816;
pub const GUEST_SS_ACCESS_RIGHT: u32 = 0x00004818;
pub const GUEST_DS_ACCESS_RIGHT: u32 = 0x0000481A;
pub const GUEST_FS_ACCESS_RIGHT: u32 = 0x0000481C;
pub const GUEST_GS_ACCESS_RIGHT: u32 = 0x0000481E;
pub const GUEST_LDTR_ACCESS_RIGHT: u32 = 0x00004820;
pub const GUEST_TR_ACCESS_RIGHT: u32 = 0x00004822;
pub const GUEST_INTERRUPTIBILITY_STATE: u32 = 0x00004824;
pub const GUEST_ACTIVITY_STATE: u32 = 0x00004826; // See 24.4.2
pub const GUEST_SMBASE: u32 = 0x00004828;
pub const GUEST_IA32_SYSENTER_CS: u32 = 0x0000482A;
pub const GUEST_VMX_PREEMPTION_TIMER: u32 = 0x0000482E;
// // Appendix b.2.3
pub const GUEST_VMCS_LINK_POINTER_LOW: u32 = 0x00002800;
pub const GUEST_VMCS_LINK_POINTER_HIGH: u32 = 0x00002801;

// //Appendix B.3.1
pub const CTLS_PIN_BASED_VM_EXECUTION: u32 = 0x00004000;
pub const CTLS_PRI_PROC_BASED_VM_EXECUTION: u32 = 0x00004002;
pub const CTLS_SEC_PROC_BASED_VM_EXECUTION: u32 = 0x0000401E;
pub const CTLS_EXCEPTION_BITMAP: u32 = 0x00004004;
pub const CTLS_IO_BITMAP_A: u32 = 0x00002000;
pub const CTLS_IO_BITMAP_B: u32 = 0x00002002;
pub const CTLS_VM_EXIT: u32 = 0x0000400C;
pub const CTLS_VM_ENTRY: u32 = 0x00004012;
pub const CTLS_VM_EXIT_MSR_STORE: u32 = 0x00002006;
pub const CTLS_VM_EXIT_MSR_STORE_COUNT: u32 = 0x0000400E;
pub const CTLS_VM_EXIT_MSR_LOAD: u32 = 0x00002008;
pub const CTLS_VM_EXIT_MSR_LOAD_COUNT: u32 = 0x00004010;
pub const CTLS_VM_ENTRY_MSR_LOAD: u32 = 0x0000200A;
pub const CTLS_VM_ENTRY_MSR_LOAD_COUNT: u32 = 0x00004014;
pub const CTLS_VM_ENTRY_INTERRUPT_INFORMATION_FIELD: u32 = 0x00004016;
pub const CTLS_EPTP: u32 = 0x0000201A;
pub const CTLS_VPID: u32 = 0x00000000;
pub const CTLS_CR3_TARGET_COUNT: u32 = 0x0000400A;
pub const RDONLY_VM_INSTRUCTION_ERROR: u32 = 0x00004400;

pub const VMEXIT_REASON: u32 = 0x00004402;
pub const VMEXIT_QUALIFICATION: u32 = 0x00006400;
pub const VMEXIT_GUEST_LINEAR_ADDR: u32 = 0x0000640A;
pub const VMEXIT_GUEST_PHYSICAL_ADDR: u32 = 0x00002400;
pub const VMEXIT_INSTRUCTION_LENGTH: u32 = 0x0000440C;
pub const VMEXIT_INSTRUCTION_INFO: u32 = 0x0000440E;
pub const VMEXIT_INTERRUPT_INFORMATION: u32 = 0x00004404;
pub const VMEXIT_INTERRUPT_ERROR_CODE: u32 = 0x00004406;

pub const VMENTRY_INTRRUPTION_INFO: u32 = 0x00004016;
pub const VMENTRY_EXCEPTION_ERRORCODE: u32 = 0x00004018;
pub const VMENTRY_INSTRUCTION_LENGTH: u32 = 0x0000401A;

pub fn vmx_write(field: u64, value: u64) -> Result<()> {
    let rflags = unsafe {
        let rflags: u64;
        asm!("vmwrite %rdx, %rax; pushfq; popq $0"
             : "=r"(rflags)
             :"{rdx}"(value), "{rax}"(field)
             :"rflags"
             : "volatile");
        rflags
    };

    let rflags = rflags::RFlags::from_bits_truncate(rflags);

    //TODO: these are actually two different errors. See
    // VMfailInvalid and VMfailValid:
    if rflags.contains(RFlags::CARRY_FLAG) {
        info!("Carry flag");
        Err(Error::VmWriteError)
    } else if rflags.contains(RFlags::ZERO_FLAG) {
        info!("zero flag");
        Err(Error::VmWriteError)
    } else {
        Ok(())
    }
}

pub fn vmx_read(field: u64) -> Result<u64> {

    let value = unsafe {
        let value: u64;
        asm!("vmread %rax, %rdx;"
             :"={rdx}"(value)
             :"{rax}"(field)
             :"rflags"
             : "volatile");
        value
    };

    Ok(value)
}

fn get_vmx_revision_id() -> u32 {
    use x86_64::registers::model_specific::Msr;
    const IA32_VMX_BASIC_MSR: u32 = 0x480;
    let vmx_basic = Msr::new(IA32_VMX_BASIC_MSR);
    unsafe { vmx_basic.read() as u32 }
}

pub fn vmx_load(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<()> {
    let vmcs_region = alloc.allocate_frame()
        .ok_or(Error::AllocError("Failed to allocate vmcs frame"))?;
    let vmcs_region_addr = vmcs_region.start_address().as_u64();

    let revision_id = get_vmx_revision_id();
    let region_revision = vmcs_region_addr as *mut u32;
    unsafe {
        *region_revision = revision_id;
    }

    let rflags = unsafe {
        let rflags: u64;
        asm!("vmptrld $1; pushfq; popq $0"
             : "=r"(rflags)
             : "m"(vmcs_region_addr)
             : "rflags");
        rflags::RFlags::from_bits_truncate(rflags)
    };

    if rflags.contains(RFlags::CARRY_FLAG) || rflags.contains(RFlags::ZERO_FLAG) {
        info!("Failed to load vmcs ptr :(")
    } else {
        info!("Vmcs ptr load!")
    }
    Ok(())
}

pub fn vmx_on(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<()> {
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

    let revision_id = get_vmx_revision_id();
    let vmxon_region = alloc.allocate_frame()
        .ok_or(Error::AllocError("Failed to allocate vmxon frame"))?;
    let vmxon_region_addr = vmxon_region.start_address().as_u64();

    //FIXME: this is currently returning very strange values
    // see https://software.intel.com/en-us/forums/virtualization-software-development/topic/293175
    info!("{}", revision_id);

    let region_revision = vmxon_region_addr as *mut u32;
    unsafe {
        *region_revision = revision_id;
    }

    // Do the actual vmxon
    let rflags = unsafe {
        let rflags: u64;
        asm!("vmxon $1; pushfq; popq $0"
             : "=r"(rflags)
             : "m"(vmxon_region_addr)
             : "rflags");
        rflags::RFlags::from_bits_truncate(rflags)
    };

    //TODO: these are actually two different errors. See
    // VMfailInvalid and VMfailValid:
    if rflags.contains(RFlags::CARRY_FLAG) || rflags.contains(RFlags::ZERO_FLAG) {
        info!("Failed to enter vmxon :(")
    } else {
        info!("Enter vmxon!")
    }

    vmx_load(alloc)?;

    Ok(())
}

struct VmcsHost {
    stack: PhysAddr,
}

struct VmcsGuest {}

struct VmcsInfo {
    host: VmcsHost,
    guest: VmcsGuest,
    vpid: u64,
}
