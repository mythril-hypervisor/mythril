use crate::error::Result;
use crate::{vcpu, vmexit};
use raw_cpuid::CpuIdResult;
use arrayvec::ArrayVec;
use core::convert::TryInto;
use bitfield::bitfield;

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

fn get_cpu_id_result(vcpu: &vcpu::VCpu, eax: u32, ecx: u32) -> Option<CpuIdResult> {
    const NAME_CREATION_ERROR_MESSAGE: &'static str = "Somehow bytes was not actually a 12 element array";

    match eax {
        CPUID_NAME => {
            if vcpu.vm.read().config.override_cpu_name() {
                let cpu_name = "MythrilCPU__";
                let bytes = cpu_name.chars().map(|char| char as u8).collect::<ArrayVec<[u8; 12]>>();
                let first_bytes: [u8; 4] = bytes[0..4].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
                let second_bytes: [u8; 4] = bytes[4..8].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
                let third_bytes: [u8; 4] = bytes[8..12].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
                return Some(CpuIdResult {
                    eax: MAX_CPUID_INPUT,
                    ebx: u32::from_le_bytes(first_bytes),
                    ecx: u32::from_le_bytes(second_bytes),
                    edx: u32::from_le_bytes(third_bytes),
                });
            }
        }
        CPUID_MODEL_FAMILY_STEPPING => {

        }
        INTEL_CORE_CACHE_TOPOLOGY => {
            let core_cpus = vcpu.vm.read().config.cpus();

            todo!()
        }
        CPUID_BRAND_STRING_1..=CPUID_BRAND_STRING_2 => {
            if vcpu.vm.read().config.override_cpu_name() { todo!("CPU Brand string not implemented yet") }
            return None;
        }
        _ => {
            //TODO for code review. Idk how I feel about silently fallingback on real cpuid here.
            // I would perhaps prefer to put a todo!() and explicitly implement stuff.
            return None;
        }
    };
    panic!()
}

pub fn emulate_cpuid(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
) -> Result<()> {
    let eax = guest_cpu.rax as u32;


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
