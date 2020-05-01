use crate::error::Result;
use crate::{vcpu, vmexit};

pub fn emulate_cpuid(
    _vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
) -> Result<()> {
    //FIXME: for now just use the actual cpuid
    let mut res = raw_cpuid::native_cpuid::cpuid_count(
        guest_cpu.rax as u32,
        guest_cpu.rcx as u32,
    );

    if guest_cpu.rax as u32 == 1 {
        // Disable MTRR
        res.edx &= !(1 << 12);

        // Disable TSC
        res.edx &= !(1 << 4);

        // Disable TSC deadline
        res.ecx &= !(1 << 24);

        // Disable XSAVE
        res.ecx &= !(1 << 26);

        // Hide hypervisor feature
        res.ecx &= !(1 << 31);
    }

    guest_cpu.rax = res.eax as u64 | (guest_cpu.rax & 0xffffffff00000000);
    guest_cpu.rbx = res.ebx as u64 | (guest_cpu.rbx & 0xffffffff00000000);
    guest_cpu.rcx = res.ecx as u64 | (guest_cpu.rcx & 0xffffffff00000000);
    guest_cpu.rdx = res.edx as u64 | (guest_cpu.rdx & 0xffffffff00000000);
    Ok(())
}
