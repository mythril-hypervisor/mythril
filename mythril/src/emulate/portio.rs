use crate::error::Result;
use crate::memory;
use crate::virtdev::{
    DeviceEvent, Port, PortReadRequest, PortWriteRequest, ResponseEventArray,
};
use crate::{vcpu, vmcs, vmexit};
use core::convert::TryFrom;

fn emulate_outs(
    vcpu: &mut vcpu::VCpu,
    port: Port,
    guest_cpu: &mut vmexit::GuestCpuState,
    exit: vmexit::IoInstructionInformation,
    responses: &mut ResponseEventArray,
) -> Result<()> {
    let linear_addr =
        vcpu.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
    let guest_addr = memory::GuestVirtAddr::new(linear_addr, &vcpu.vmcs)?;

    // FIXME: This could actually be any priv level due to IOPL, but for now
    //        assume that is requires supervisor
    let access = memory::GuestAccess::Read(memory::PrivilegeLevel(0));

    let view = memory::GuestAddressSpaceView::from_vmcs(
        &vcpu.vmcs,
        &vcpu.vm.guest_space,
    )?;

    // FIXME: The direction we read is determined by the DF flag (I think)
    // FIXME: We should probably only be using some of the lower order bits
    let bytes = view.read_bytes(
        guest_addr,
        (guest_cpu.rcx * exit.size as u64) as usize,
        access,
    )?;

    // FIXME: Actually test for REP
    for chunk in bytes.chunks_exact(exit.size as usize) {
        let request = PortWriteRequest::try_from(chunk)?;
        vcpu.vm.dispatch_event(
            port,
            DeviceEvent::PortWrite(port, request),
            vcpu,
            responses,
        )?;
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
    responses: &mut ResponseEventArray,
) -> Result<()> {
    let linear_addr =
        vcpu.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
    let guest_addr = memory::GuestVirtAddr::new(linear_addr, &vcpu.vmcs)?;
    let access = memory::GuestAccess::Read(memory::PrivilegeLevel(0));

    let mut bytes = vec![0u8; guest_cpu.rcx as usize];
    for chunk in bytes.chunks_exact_mut(exit.size as usize) {
        let request = PortReadRequest::try_from(chunk)?;
        vcpu.vm.dispatch_event(
            port,
            DeviceEvent::PortRead(port, request),
            vcpu,
            responses,
        )?;
    }

    let view = memory::GuestAddressSpaceView::from_vmcs(
        &vcpu.vmcs,
        &vcpu.vm.guest_space,
    )?;
    view.write_bytes(guest_addr, &bytes, access)?;

    guest_cpu.rdi += bytes.len() as u64;
    guest_cpu.rcx = 0;
    Ok(())
}

pub fn emulate_portio(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    exit: vmexit::IoInstructionInformation,
    responses: &mut ResponseEventArray,
) -> Result<()> {
    let (port, input, size, string) =
        (exit.port, exit.input, exit.size, exit.string);

    if !string {
        if !input {
            let arr = (guest_cpu.rax as u32).to_be_bytes();
            let request =
                PortWriteRequest::try_from(&arr[4 - size as usize..])?;
            vcpu.vm.dispatch_event(
                port,
                DeviceEvent::PortWrite(port, request),
                vcpu,
                responses,
            )?;
        } else {
            let mut arr = [0u8; 4];
            let request =
                PortReadRequest::try_from(&mut arr[4 - size as usize..])?;
            vcpu.vm.dispatch_event(
                port,
                DeviceEvent::PortRead(port, request),
                vcpu,
                responses,
            )?;
            guest_cpu.rax &= (!guest_cpu.rax) << (size * 8);
            guest_cpu.rax |= u32::from_be_bytes(arr) as u64;
        };
    } else {
        if !input {
            emulate_outs(vcpu, port, guest_cpu, exit, responses)?;
        } else {
            emulate_ins(vcpu, port, guest_cpu, exit, responses)?;
        }
    }

    Ok(())
}
