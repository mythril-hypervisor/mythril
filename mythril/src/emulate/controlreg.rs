use crate::error::{Error, Result};
use crate::{vcpu, vmcs, vmexit, vmx};

pub fn emulate_access(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    info: vmexit::CrInformation,
) -> Result<()> {
    match info.cr_num {
        0 => match info.access_type {
            vmexit::CrAccessType::Clts => {
                let cr0 = vcpu.vmcs.read_field(vmcs::VmcsField::GuestCr0)?;
                vcpu.vmcs
                    .write_field(vmcs::VmcsField::GuestCr0, cr0 & !0b1000)?;
            }
            vmexit::CrAccessType::MovToCr => {
                let reg = info.register.unwrap();
                let val = reg.read(&vcpu.vmcs, guest_cpu)?;
                vcpu.vmcs.write_field(vmcs::VmcsField::GuestCr0, val)?;
            }
            op => panic!("Unsupported MovToCr cr0 operation: {:?}", op),
        },
        3 => match info.access_type {
            vmexit::CrAccessType::MovToCr => {
                let reg = info.register.unwrap();
                let mut val = reg.read(&vcpu.vmcs, guest_cpu)?;

                // If CR4.PCIDE = 1, bit 63 of the source operand to MOV to
                // CR3 determines whether the instruction invalidates entries
                // in the TLBs and the paging-structure caches. The instruction
                // does not modify bit 63 of CR3, which is reserved and always 0
                if val & (1 << 63) == 0 {
                    // Some instructions invalidate all entries in the TLBs and
                    // paging-structure caches—except for global translations.
                    // An example is the MOV to CR3 instruction. Emulation of such
                    // an instruction may require execution of the INVVPID instruction
                    // as follows:
                    // — The INVVPID type is single-context-retaining-globals (3).
                    // — The VPID in the INVVPID descriptor is the one assigned to the
                    //   virtual processor whose execution is being emulated.
                    let vpid = vcpu
                        .vmcs
                        .read_field(vmcs::VmcsField::VirtualProcessorId)?;
                    vcpu.vmcs.vmx.invvpid(
                        vmx::InvVpidMode::SingleContextRetainGlobal(
                            vpid as u16,
                        ),
                    )?;
                } else {
                    val &= !(1 << 63);
                }

                vcpu.vmcs.write_field(vmcs::VmcsField::GuestCr3, val)?;
            }
            vmexit::CrAccessType::MovFromCr => {
                let reg = info.register.unwrap();
                let val = vcpu.vmcs.read_field(vmcs::VmcsField::GuestCr3)?;
                reg.write(val, &mut vcpu.vmcs, guest_cpu)?;
            }
            op => panic!("Unsupported MovFromCr cr0 operation: {:?}", op),
        },
        _ => {
            error!("Unsupported CR number access");
            return Err(Error::InvalidValue)
        }
    }
    Ok(())
}
