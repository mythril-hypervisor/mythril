use crate::error::Result;
use crate::vcpu::VCpu;
use crate::{vcpu, vmexit};
use arrayvec::ArrayVec;
use bitfield::bitfield;
<<<<<<< HEAD
use core::convert::TryInto;
use raw_cpuid::CpuIdResult;
=======
use bitflags::_core::num::flt2dec::to_shortest_exp_str;
use crate::apic::get_local_apic;
>>>>>>> handle CPUID_MODEL_FAMILY_STEPPING

const CPUID_NAME: u32 = 0;
const CPUID_MODEL_FAMILY_STEPPING: u32 = 1;
const CPUID_CACHE_TLB_INFO: u32 = 2;
const INTEL_CORE_CACHE_TOPOLOGY: u32 = 4;
const THERMAL_AND_POWER_MANAGEMENT: u32 = 6;
const STRUCTURED_EXTENDED_FEATURE_FLAGS: u32 = 7;
const ARCHITECTURAL_PERFORMANCE: u32 = 0xA;
const EXTENDED_TOPOLOGY_ENUMERATION: u32 = 0xB;
const PROCESSOR_EXTENDED_STATE_ENUMERATION: u32 = 0xD;
const V2_EXTENDED_TOPOLOGY_ENUMERATION: u32 = 0x1F;
const EXTENDED_FUNCTION_CPUID_INFORMATION: u32 = 0x80000000;
const CPUID_BRAND_STRING_1: u32 = 0x80000002;
const CPUID_BRAND_STRING_2: u32 = 0x80000003;
const CPUID_BRAND_STRING_3: u32 = 0x80000004;
const MAX_CPUID_INPUT: u32 = 0x80000008;
//todo //CPUID leaves above 2 and below 80000000H are visible only when
//     // IA32_MISC_ENABLE[bit 22] has its default value of 0.

const NAME_CREATION_ERROR_MESSAGE: &'static str =
    "Somehow bytes was not actually a 12 element array";

fn get_cpu_id_result(
    vcpu: &vcpu::VCpu,
    eax_in: u32,
    ecx_in: u32,
) -> CpuIdResult {
    let mut actual = raw_cpuid::native_cpuid::cpuid_count(eax_in, ecx_in);

    match eax_in {
        CPUID_NAME => cpuid_name(vcpu, actual),
        CPUID_MODEL_FAMILY_STEPPING => cpuid_model_family_stepping(actual),
        INTEL_CORE_CACHE_TOPOLOGY => intel_cache_topo(actual),
        THERMAL_AND_POWER_MANAGEMENT => {
            todo!("Portions of this output are per core, but presumably we don't support this. Additionally there is stuff about APIC timers here, also unsure if supported.")
        }
        STRUCTURED_EXTENDED_FEATURE_FLAGS => {
            // nothing here seems to suspicious so just return actual:
            actual
        }
        ARCHITECTURAL_PERFORMANCE => {
            // For now I assume performance counters are unsupported, but if one wanted
            // to support performance counters this would need to be handled here, and other places
            actual
        }
        EXTENDED_TOPOLOGY_ENUMERATION => {
            todo!("This basically requires APIC stuff to be done.")
        }
        PROCESSOR_EXTENDED_STATE_ENUMERATION => actual,
        // There are bunch more leaves after PROCESSOR_EXTENDED_STATE_ENUMERATION, however most of them seem unlikely to be used/ not relevant
        V2_EXTENDED_TOPOLOGY_ENUMERATION => {
            todo!("Requires APIC")
        }
        0x40000000..=0x4FFFFFFF => {
            // these are software reserved.
            actual
        }
        EXTENDED_FUNCTION_CPUID_INFORMATION => CpuIdResult {
            eax: MAX_CPUID_INPUT,
            ebx: 0,
            ecx: 0,
            edx: 0,
        },
        CPUID_BRAND_STRING_1..=CPUID_BRAND_STRING_3 => {
            if vcpu.vm.config.override_cpu_name() {
                todo!("CPU Brand string not implemented yet")
            }
            actual
        }
        _ => {
            //TODO for code review. Idk how I feel about silently fallingback on real cpuid here.
            // I would perhaps prefer to put a todo!() and explicitly implement stuff.
            actual
        }
    }
}

bitfield! {
    pub struct IntelCoreCacheTopologyEaxRes(u32);
    impl Debug;
    cache_type,set_cache_type:4,0;
    cache_level,set_cache_level:7,5;
    self_init_cache_level,set_self_init_cache_level:8;
    fully_associative,set_fully_associative:9;
    max_addressable_ids_logical,set_max_addressable_ids_logical:14,25;
    max_addressable_ids_physical,set_max_addressable_ids_physical:26,31;
}

fn intel_cache_topo(actual: CpuIdResult) -> CpuIdResult {
    let mut cache_topo_eax = IntelCoreCacheTopologyEaxRes(actual.eax);
    cache_topo_eax.set_max_addressable_ids_logical(todo!("waiting on apics"));
    cache_topo_eax.set_max_addressable_ids_physical(todo!("waiting on apics"));
    let eax = cache_topo_eax.0;
    CpuIdResult {
        eax,
        //no changes should be required for these:
        ebx: actual.ebx,
        ecx: actual.ecx,
        edx: actual.edx,
    }
}

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
    max_processor_ids,set_max_processor_ids:23,16;
    apic_id, set_apic_id:31,24;
}

bitfield! {
    pub struct FeatureInformationECX(u32);
    impl Debug;
    //there are a lot of features here, so only add the ones we care about for now.
    xsave, set_xsave: 26;
    hypervisor, set_hypervisor: 31;
}

bitfield! {
    pub struct FeatureInformationEDX(u32);
    impl Debug;
    //there are a lot of features here, so only add the ones we care about for now.
    mtrr, set_mtrr: 12;
}

fn cpuid_model_family_stepping(actual: CpuIdResult) -> CpuIdResult {
    let family_model_stepping =
        IntelTypeFamilyModelSteppingIDEaxRes(actual.eax);
    //we can change family_model_stepping, but for now just use actual.
    let eax = family_model_stepping.0;
    let mut brand_cflush_max_initial = BrandCFlushMaxIDsInitialAPIC(actual.ebx);
    brand_cflush_max_initial.set_apic_id(todo!("Waiting on virtual APICs"));
    brand_cflush_max_initial
        .set_max_processor_ids(todo!("Waiting on virtual APICs"));
    let ebx = brand_cflush_max_initial.0;
    let mut features_ecx = FeatureInformationECX(actual.ecx);
    let mut features_edx = FeatureInformationEDX(actual.edx);
    // I would have made type safe bindings for this but then I saw how many fields there where...

    // Disable MTRR
    features_edx.set_mtrr(false);

    // Disable XSAVE
    // ecx &= !(1 << 26);
    features_ecx.set_xsave(false);

    // Hide hypervisor feature
    features_ecx.set_hypervisor(false);
    let ecx = features_ecx.0;
    let edx = features_edx.0;
    CpuIdResult { eax, ebx, ecx, edx }
}

fn cpuid_name(vcpu: &VCpu, actual: CpuIdResult) -> CpuIdResult {
    if vcpu.vm.config.override_cpu_name() {
        let cpu_name = "MythrilCPU__";
        let bytes = cpu_name
            .chars()
            .map(|char| char as u8)
            .collect::<ArrayVec<[u8; 12]>>();
        let first_bytes: [u8; 4] =
            bytes[0..4].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
        let second_bytes: [u8; 4] =
            bytes[4..8].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
        let third_bytes: [u8; 4] =
            bytes[8..12].try_into().expect(NAME_CREATION_ERROR_MESSAGE);
        return CpuIdResult {
            eax: MAX_CPUID_INPUT,
            ebx: u32::from_le_bytes(first_bytes),
            ecx: u32::from_le_bytes(second_bytes),
            edx: u32::from_le_bytes(third_bytes),
        };
    }
    actual
}
