use crate::error::Result;
use crate::{vcpu, vmexit};
use raw_cpuid::CpuIdResult;
use arrayvec::ArrayVec;
use core::convert::TryInto;
use bitfield::bitfield;
use bitflags::_core::num::flt2dec::to_shortest_exp_str;
use crate::apic::get_local_apic;

//Used https://c9x.me/x86/html/file_module_x86_id_45.html as guid for implementing this.
const CPUID_NAME: u32 = 0;
const CPUID_MODEL_FAMILY_STEPPING: u32 = 1;
const CPUID_CACHE_TLB_INFO: u32 = 2;
const INTEL_CORE_CACHE_TOPOLOGY: u32 = 4;
const CPUID_BRAND_STRING_1: u32 = 0x80000002;
const CPUID_BRAND_STRING_2: u32 = 0x80000003;
const CPUID_BRAND_STRING_3: u32 = 0x80000004;
const MAX_CPUID_INPUT: u32 = 0x80000004;
//todo //CPUID leaves above 2 and below 80000000H are visible only when
//     // IA32_MISC_ENABLE[bit 22] has its default value of 0.


//
// bitfield! {
//     pub struct IntelCoreCacheTopologyEaxRes(u32)
//     impl Debug;
//     impl Copy;
//
// }

bitfield! {
    pub struct IntelTypeFamilyModelSteppingIDEaxRes(u32);
    impl Debug;
    stepping_id, _: 3,0;
    model,_:7,4;
    family_id,_:11,8;
    processor_type,_:13,12;
    extended_model_id,_:19,16;
    extended_family_id,_:27,20;
}

bitfield! {
    pub struct BrandCFlushMaxIDsInitialAPIC(u32);
    impl Debug;
    brand_idx, _: 7,0;
    cflush,_:15,8;
    max_processor_ids,_:23,16;
    apic_id,_:31,24;
}


fn get_cpu_id_result(vcpu: &vcpu::VCpu, eax_in: u32, ecx_in: u32) -> CpuIdResult {
    const NAME_CREATION_ERROR_MESSAGE: &'static str = "Somehow bytes was not actually a 12 element array";


    let mut actual = raw_cpuid::native_cpuid::cpuid_count(
        guest_cpu.rax as u32,
        guest_cpu.rcx as u32,
    );

    match eax_in {
        CPUID_NAME => cpuid_name(vcpu, &mut actual),
        CPUID_MODEL_FAMILY_STEPPING => {
            let family_model_stepping = IntelTypeFamilyModelSteppingIDEaxRes(actual.eax);
            //we can change family_model_stepping, but for now just use actual.
            let eax = family_model_stepping.0;
            let mut brand_cflush_max_initial = BrandCFlushMaxIDsInitialAPIC(actual.ebx);
            brand_cflush_max_initial.set_apic_id(get_local_apic().id());//in principle this is redundant
            let ebx = brand_cflush_max_initial.0;
            let mut ecx = actual.ecx;
            let mut edx = actual.edx;
            // I would have made type safe bindings for this but then I saw how many fields there where...

            // Disable MTRR
            edx &= !(1 << 12);

            // Disable XSAVE
            ecx &= !(1 << 26);

            // Hide hypervisor feature
            ecx &= !(1 << 31);
            CpuIdResult{
                eax,
                ebx,
                ecx,
                edx
            }
        }
        INTEL_CORE_CACHE_TOPOLOGY => {
            let core_cpus = vcpu.vm.read().config.cpus();

            todo!()
        }
        CPUID_BRAND_STRING_1..=CPUID_BRAND_STRING_2 => {
            if vcpu.vm.read().config.override_cpu_name() { todo!("CPU Brand string not implemented yet") }
            actual
        }
        _ => {
            //TODO for code review. Idk how I feel about silently fallingback on real cpuid here.
            // I would perhaps prefer to put a todo!() and explicitly implement stuff.
            actual
        }
    }
}

fn cpuid_name(vcpu: &VCpu, actual: &mut CpuIdResult) -> CpuIdResult {
    if vcpu.vm.read().config.override_cpu_name() {
        let cpu_name = "MythrilCPU__";
        let bytes = cpu_name.chars().map(|char| char as u8).collect::<ArrayVec<[u8; 12]>>();
        let first_bytes: [u8; 4] = bytes[0..4].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
        let second_bytes: [u8; 4] = bytes[4..8].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
        let third_bytes: [u8; 4] = bytes[8..12].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
        return CpuIdResult {
            eax: MAX_CPUID_INPUT,
            ebx: u32::from_le_bytes(first_bytes),
            ecx: u32::from_le_bytes(second_bytes),
            edx: u32::from_le_bytes(third_bytes),
        };
    }
    actual
}

pub fn emulate_cpuid(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
) -> Result<()> {
    let eax = guest_cpu.rax as u32;

    let ecx = guest_cpu.rcx as u32;
    let mut res = get_cpu_id_result(vcpu, eax, ecx);

    //todo move this into get_cpu_id_result
    if guest_cpu.rax as u32 == 1 {

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
