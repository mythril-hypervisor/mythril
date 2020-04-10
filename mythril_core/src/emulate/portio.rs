use crate::device::{Port, PortReadRequest, PortWriteRequest};
use crate::error::Result;
use crate::memory;
use crate::{vcpu, vmcs, vmexit};
use core::convert::TryFrom;

fn emulate_outs(
    vcpu: &mut vcpu::VCpu,
    port: Port,
    guest_cpu: &mut vmexit::GuestCpuState,
    exit: vmexit::IoInstructionInformation,
) -> Result<()> {
    let mut vm = vcpu.vm.write();

    let linear_addr =
        vcpu.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
    let guest_addr = memory::GuestVirtAddr::new(linear_addr, &vcpu.vmcs)?;

    // FIXME: This could actually be any priv level due to IOPL, but for now
    //        assume that is requires supervisor
    let access = memory::GuestAccess::Read(memory::PrivilegeLevel(0));

    // FIXME: The direction we read is determined by the DF flag (I think)
    // FIXME: We should probably only be using some of the lower order bits
    let bytes = vm.guest_space.read_bytes(
        &vcpu.vmcs,
        guest_addr,
        (guest_cpu.rcx * exit.size as u64) as usize,
        access,
    )?;

    // FIXME: Actually test for REP
    for chunk in bytes.chunks_exact(exit.size as usize) {
        let request = PortWriteRequest::try_from(chunk)?;
        vm.on_port_write(vcpu, port, request)?;
    }

    guest_cpu.rsi += bytes.len() as u64;
    guest_cpu.rcx = 0;
    Ok(())
}

fn emulate_ins(
    vcpu: &mut vcpu::VCpu,
    port: Port,
    guest_cpu: &mut vmexit::GuestCpuState,
    exit: vmexit::IoInstructionInformation,
) -> Result<()> {
    let mut vm = vcpu.vm.write();

    let linear_addr =
        vcpu.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
    let guest_addr = memory::GuestVirtAddr::new(linear_addr, &vcpu.vmcs)?;
    let access = memory::GuestAccess::Read(memory::PrivilegeLevel(0));

    let mut bytes = vec![0u8; guest_cpu.rcx as usize];
    for chunk in bytes.chunks_exact_mut(exit.size as usize) {
        let request = PortReadRequest::try_from(chunk)?;
        vm.on_port_read(vcpu, port, request)?;
    }

    vm.guest_space
        .write_bytes(&vcpu.vmcs, guest_addr, &bytes, access)?;

    guest_cpu.rdi += bytes.len() as u64;
    guest_cpu.rcx = 0;
    Ok(())
}

pub fn emulate_portio(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    exit: vmexit::IoInstructionInformation,
) -> Result<()> {
    let (port, input, size, string) =
        (exit.port, exit.input, exit.size, exit.string);

    if !string {
        let mut vm = vcpu.vm.write();

        if !input {
            let arr = (guest_cpu.rax as u32).to_be_bytes();
            vm.on_port_write(
                vcpu,
                port,
                PortWriteRequest::try_from(&arr[4 - size as usize..])?,
            )?;
        } else {
            let mut arr = [0u8; 4];
            let request = PortReadRequest::try_from(&mut arr[4 - size as usize..])?;
            vm.on_port_read(vcpu, port, request)?;
            guest_cpu.rax &= (!guest_cpu.rax) << (size * 8);
            guest_cpu.rax |= u32::from_be_bytes(arr) as u64;
        }
    } else {
        if !input {
            emulate_outs(vcpu, port, guest_cpu, exit)?;
        } else {
            emulate_ins(vcpu, port, guest_cpu, exit)?;
        }
    }
    Ok(())
}
