use crate::error::Result;
use crate::{vcpu, vmexit};

//Used https://c9x.me/x86/html/file_module_x86_id_45.html as guid for implementing this.
const CPUID_NAME: u32 = 0;
const CPUID_MODEL_FAMILY_STEPPING: u32 = 1;
const CPUID_CACHE_TLB_INFO: u32 = 2;
const INTEL_CORE_CACHE_TOPOLOGY : u32 = 4;
const CPUID_BRAND_STRING_1: u32 = 0x80000002;
const CPUID_BRAND_STRING_2: u32 = 0x80000003;
const CPUID_BRAND_STRING_3: u32 = 0x80000004;
//todo //CPUID leaves above 2 and below 80000000H are visible only when
//     // IA32_MISC_ENABLE[bit 22] has its default value of 0.



pub fn emulate_cpuid(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
) -> Result<()> {
    let eax = guest_cpu.rax as u32;

    match eax {
        CPUID_NAME => {
            if vcpu.vm.read().config.override_cpu_name(){
                todo!()
            }
        },
        CPUID_MODEL_FAMILY_STEPPING => todo!(),
        INTEL_CORE_CACHE_TOPOLOGY => {
            _vcpu.vm.read().config.cpus()
        }
        CPUID_BRAND_STRING_1 => todo!(),
        CPUID_BRAND_STRING_2 => todo!(),
        _ => {
            // dbg!(eax);
            // todo!("If you are reading this then a invalid arg was passed to cpuid. In principle we should prob fault here or something, but this probably indicates a bug.")
        }
    }

    //FIXME: for now just use the actual cpuid
    let mut res = raw_cpuid::native_cpuid::cpuid_count(
        guest_cpu.rax as u32,
        guest_cpu.rcx as u32,
    );

    if guest_cpu.rax as u32 == 1 {
        // Disable MTRR
        res.edx &= !(1 << 12);

        // Disable XSAVE
        res.ecx &= !(1 << 26);

        // Hide hypervisor feature
        res.ecx &= !(1 << 31);

        // Hide TSC deadline timer
        res.ecx &= !(1 << 24);
    } else if guest_cpu.rax as u32 == 0x0b {
        res.edx = crate::percore::read_core_id().raw as u32;
    }

    guest_cpu.rax = res.eax as u64 | (guest_cpu.rax & 0xffffffff00000000);
    guest_cpu.rbx = res.ebx as u64 | (guest_cpu.rbx & 0xffffffff00000000);
    guest_cpu.rcx = res.ecx as u64 | (guest_cpu.rcx & 0xffffffff00000000);
    guest_cpu.rdx = res.edx as u64 | (guest_cpu.rdx & 0xffffffff00000000);
    Ok(())
}
