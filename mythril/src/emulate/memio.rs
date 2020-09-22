use crate::error::{Error, Result};
use crate::memory;
use crate::virtdev::{
    DeviceEvent, MemReadRequest, MemWriteRequest, ResponseEventArray,
};
use crate::{vcpu, vmcs, vmexit};
use arrayvec::ArrayVec;
use iced_x86;

macro_rules! read_register {
    ($out:ident, $value:expr, $type:ty) => {{
        let data = ($value as $type).to_be_bytes();
        $out.try_extend_from_slice(&data).map_err(|_| {
            Error::InvalidValue("Invalid length with reading register".into())
        })?;
    }};
}

macro_rules! write_register {
    ($vm:ident, $vcpu:ident, $addr:expr, $responses:ident, $value:expr, $type:ty, $mask:expr) => {{
        let mut buff = <$type>::default().to_be_bytes();
        let request = MemReadRequest::new(&mut buff[..]);
        $vm.dispatch_event(
            $addr,
            DeviceEvent::MemRead(($addr, request)),
            $vcpu,
            $responses,
        )?;
        $value = ($value & $mask) | <$type>::from_be_bytes(buff) as u64;
    }};
}

fn read_register_value(
    register: iced_x86::Register,
    vmcs: &vmcs::ActiveVmcs,
    guest_cpu: &mut vmexit::GuestCpuState,
) -> Result<ArrayVec<[u8; 8]>> {
    let mut res = ArrayVec::new();

    // TODO: we should probably support the AH style registers
    match register {
        iced_x86::Register::AL => read_register!(res, guest_cpu.rax, u8),
        iced_x86::Register::AX => read_register!(res, guest_cpu.rax, u16),
        iced_x86::Register::EAX => read_register!(res, guest_cpu.rax, u32),
        iced_x86::Register::RAX => read_register!(res, guest_cpu.rax, u64),

        iced_x86::Register::BL => read_register!(res, guest_cpu.rbx, u8),
        iced_x86::Register::BX => read_register!(res, guest_cpu.rbx, u16),
        iced_x86::Register::EBX => read_register!(res, guest_cpu.rbx, u32),
        iced_x86::Register::RBX => read_register!(res, guest_cpu.rbx, u64),

        iced_x86::Register::CL => read_register!(res, guest_cpu.rcx, u8),
        iced_x86::Register::CX => read_register!(res, guest_cpu.rcx, u16),
        iced_x86::Register::ECX => read_register!(res, guest_cpu.rcx, u32),
        iced_x86::Register::RCX => read_register!(res, guest_cpu.rcx, u64),

        iced_x86::Register::DL => read_register!(res, guest_cpu.rdx, u8),
        iced_x86::Register::DX => read_register!(res, guest_cpu.rdx, u16),
        iced_x86::Register::EDX => read_register!(res, guest_cpu.rdx, u32),
        iced_x86::Register::RDX => read_register!(res, guest_cpu.rdx, u64),

        iced_x86::Register::R8L => read_register!(res, guest_cpu.r8, u8),
        iced_x86::Register::R8W => read_register!(res, guest_cpu.r8, u16),
        iced_x86::Register::R8D => read_register!(res, guest_cpu.r8, u32),
        iced_x86::Register::R8 => read_register!(res, guest_cpu.r8, u64),

        iced_x86::Register::R9L => read_register!(res, guest_cpu.r9, u8),
        iced_x86::Register::R9W => read_register!(res, guest_cpu.r9, u16),
        iced_x86::Register::R9D => read_register!(res, guest_cpu.r9, u32),
        iced_x86::Register::R9 => read_register!(res, guest_cpu.r9, u64),

        iced_x86::Register::R10L => read_register!(res, guest_cpu.r10, u8),
        iced_x86::Register::R10W => read_register!(res, guest_cpu.r10, u16),
        iced_x86::Register::R10D => read_register!(res, guest_cpu.r10, u32),
        iced_x86::Register::R10 => read_register!(res, guest_cpu.r10, u64),

        iced_x86::Register::R11L => read_register!(res, guest_cpu.r11, u8),
        iced_x86::Register::R11W => read_register!(res, guest_cpu.r11, u16),
        iced_x86::Register::R11D => read_register!(res, guest_cpu.r11, u32),
        iced_x86::Register::R11 => read_register!(res, guest_cpu.r11, u64),

        iced_x86::Register::R12L => read_register!(res, guest_cpu.r12, u8),
        iced_x86::Register::R12W => read_register!(res, guest_cpu.r12, u16),
        iced_x86::Register::R12D => read_register!(res, guest_cpu.r12, u32),
        iced_x86::Register::R12 => read_register!(res, guest_cpu.r12, u64),

        iced_x86::Register::R13L => read_register!(res, guest_cpu.r13, u8),
        iced_x86::Register::R13W => read_register!(res, guest_cpu.r13, u16),
        iced_x86::Register::R13D => read_register!(res, guest_cpu.r13, u32),
        iced_x86::Register::R13 => read_register!(res, guest_cpu.r13, u64),

        iced_x86::Register::R14L => read_register!(res, guest_cpu.r14, u8),
        iced_x86::Register::R14W => read_register!(res, guest_cpu.r14, u16),
        iced_x86::Register::R14D => read_register!(res, guest_cpu.r14, u32),
        iced_x86::Register::R14 => read_register!(res, guest_cpu.r14, u64),

        iced_x86::Register::R15L => read_register!(res, guest_cpu.r15, u8),
        iced_x86::Register::R15W => read_register!(res, guest_cpu.r15, u16),
        iced_x86::Register::R15D => read_register!(res, guest_cpu.r15, u32),
        iced_x86::Register::R15 => read_register!(res, guest_cpu.r15, u64),

        iced_x86::Register::DIL => read_register!(res, guest_cpu.rdi, u8),
        iced_x86::Register::DI => read_register!(res, guest_cpu.rdi, u16),
        iced_x86::Register::EDI => read_register!(res, guest_cpu.rdi, u32),
        iced_x86::Register::RDI => read_register!(res, guest_cpu.rdi, u64),

        iced_x86::Register::SIL => read_register!(res, guest_cpu.rsi, u8),
        iced_x86::Register::SI => read_register!(res, guest_cpu.rsi, u16),
        iced_x86::Register::ESI => read_register!(res, guest_cpu.rsi, u32),
        iced_x86::Register::RSI => read_register!(res, guest_cpu.rsi, u64),

        iced_x86::Register::SPL => {
            read_register!(res, vmcs.read_field(vmcs::VmcsField::GuestRsp)?, u8)
        }
        iced_x86::Register::SP => read_register!(
            res,
            vmcs.read_field(vmcs::VmcsField::GuestRsp)?,
            u16
        ),
        iced_x86::Register::ESP => read_register!(
            res,
            vmcs.read_field(vmcs::VmcsField::GuestRsp)?,
            u32
        ),
        iced_x86::Register::RSP => read_register!(
            res,
            vmcs.read_field(vmcs::VmcsField::GuestRsp)?,
            u64
        ),

        iced_x86::Register::BPL => read_register!(res, guest_cpu.rbp, u8),
        iced_x86::Register::BP => read_register!(res, guest_cpu.rbp, u16),
        iced_x86::Register::EBP => read_register!(res, guest_cpu.rbp, u32),
        iced_x86::Register::RBP => read_register!(res, guest_cpu.rbp, u64),

        _ => {
            return Err(Error::InvalidValue(format!(
                "Invalid register '{:?}'",
                register
            )))
        }
    }

    Ok(res)
}

fn do_mmio_write(
    addr: memory::GuestPhysAddr,
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    responses: &mut ResponseEventArray,
    instr: iced_x86::Instruction,
) -> Result<()> {
    let mut res = ArrayVec::<[u8; 8]>::new();
    let data = match instr.op1_kind() {
        iced_x86::OpKind::Register => {
            let reg = instr.op_register(1);
            read_register_value(reg, &vcpu.vmcs, guest_cpu)?
        }
        iced_x86::OpKind::Immediate8 => {
            let value = instr.immediate8();
            res.try_extend_from_slice(&value.to_be_bytes()).unwrap();
            res
        }
        iced_x86::OpKind::Immediate16 => {
            let value = instr.immediate16();
            res.try_extend_from_slice(&value.to_be_bytes()).unwrap();
            res
        }
        iced_x86::OpKind::Immediate32 => {
            let value = instr.immediate32();
            res.try_extend_from_slice(&value.to_be_bytes()).unwrap();
            res
        }
        iced_x86::OpKind::Immediate64 => {
            let value = instr.immediate64();
            res.try_extend_from_slice(&value.to_be_bytes()).unwrap();
            res
        }
        _ => return Err(Error::NotSupported),
    };
    let request = MemWriteRequest::new(&data[..]);

    let mut vm = vcpu.vm.write();
    vm.dispatch_event(
        addr,
        crate::virtdev::DeviceEvent::MemWrite((addr, request)),
        vcpu,
        responses,
    )
}

fn do_mmio_read(
    addr: memory::GuestPhysAddr,
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    responses: &mut ResponseEventArray,
    instr: iced_x86::Instruction,
) -> Result<()> {
    let mut vm = vcpu.vm.write();
    match instr.op0_kind() {
        iced_x86::OpKind::Register => match instr.op_register(0) {
            iced_x86::Register::AL => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rax,
                u8,
                !0xff
            ),
            iced_x86::Register::AX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rax,
                u16,
                !0xffff
            ),
            iced_x86::Register::EAX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rax,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::RAX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rax,
                u64,
                0x00
            ),

            iced_x86::Register::BL => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rbx,
                u8,
                !0xff
            ),
            iced_x86::Register::BX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rbx,
                u16,
                !0xffff
            ),
            iced_x86::Register::EBX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rbx,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::RBX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rbx,
                u64,
                0x00
            ),

            iced_x86::Register::CL => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rcx,
                u8,
                !0xff
            ),
            iced_x86::Register::CX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rcx,
                u16,
                !0xffff
            ),
            iced_x86::Register::ECX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rcx,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::RCX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdx,
                u64,
                0x00
            ),

            iced_x86::Register::DL => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdx,
                u8,
                !0xff
            ),
            iced_x86::Register::DX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdx,
                u16,
                !0xffff
            ),
            iced_x86::Register::EDX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdx,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::RDX => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdx,
                u64,
                0x00
            ),

            iced_x86::Register::R8L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r8,
                u8,
                !0xff
            ),
            iced_x86::Register::R8W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r8,
                u16,
                !0xffff
            ),
            iced_x86::Register::R8D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r8,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R8 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r8,
                u64,
                0x00
            ),

            iced_x86::Register::R9L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r9,
                u8,
                !0xff
            ),
            iced_x86::Register::R9W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r9,
                u16,
                !0xffff
            ),
            iced_x86::Register::R9D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r9,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R9 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r9,
                u64,
                0x00
            ),

            iced_x86::Register::R10L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r10,
                u8,
                !0xff
            ),
            iced_x86::Register::R10W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r10,
                u16,
                !0xffff
            ),
            iced_x86::Register::R10D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r10,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R10 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r10,
                u64,
                0x00
            ),

            iced_x86::Register::R11L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r11,
                u8,
                !0xff
            ),
            iced_x86::Register::R11W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r11,
                u16,
                !0xffff
            ),
            iced_x86::Register::R11D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r11,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R11 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r11,
                u64,
                0x00
            ),

            iced_x86::Register::R12L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r12,
                u8,
                !0xff
            ),
            iced_x86::Register::R12W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r12,
                u16,
                !0xffff
            ),
            iced_x86::Register::R12D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r12,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R12 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r12,
                u64,
                0x00
            ),

            iced_x86::Register::R13L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r13,
                u8,
                !0xff
            ),
            iced_x86::Register::R13W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r13,
                u16,
                !0xffff
            ),
            iced_x86::Register::R13D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r13,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R13 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r13,
                u64,
                0x00
            ),

            iced_x86::Register::R14L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r14,
                u8,
                !0xff
            ),
            iced_x86::Register::R14W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r14,
                u16,
                !0xffff
            ),
            iced_x86::Register::R14D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r14,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R14 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r14,
                u64,
                0x00
            ),

            iced_x86::Register::R15L => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r15,
                u8,
                !0xff
            ),
            iced_x86::Register::R15W => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r15,
                u16,
                !0xffff
            ),
            iced_x86::Register::R15D => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r15,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::R15 => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.r15,
                u64,
                0x00
            ),

            iced_x86::Register::DIL => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdi,
                u8,
                !0xff
            ),
            iced_x86::Register::DI => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdi,
                u16,
                !0xffff
            ),
            iced_x86::Register::EDI => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdi,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::RDI => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rdi,
                u64,
                0x00
            ),

            iced_x86::Register::SIL => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rsi,
                u8,
                !0xff
            ),
            iced_x86::Register::SI => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rsi,
                u16,
                !0xffff
            ),
            iced_x86::Register::ESI => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rsi,
                u32,
                !0xffffffff
            ),
            iced_x86::Register::RSI => write_register!(
                vm,
                vcpu,
                addr,
                responses,
                guest_cpu.rsi,
                u64,
                0x00
            ),

            register => {
                return Err(Error::InvalidValue(format!(
                    "mmio read into invalid register '{:?}'",
                    register
                )))
            }
        },
        _ => return Err(Error::NotSupported),
    };

    Ok(())
}

pub fn handle_ept_violation(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    _exit: vmexit::EptInformation,
    responses: &mut ResponseEventArray,
) -> Result<()> {
    let instruction_len = vcpu
        .vmcs
        .read_field(vmcs::VmcsField::VmExitInstructionLen)?;
    let ip = vcpu.vmcs.read_field(vmcs::VmcsField::GuestRip)?;

    let mut vm = vcpu.vm.write();
    let ip_addr = memory::GuestVirtAddr::new(ip, &vcpu.vmcs)?;
    let view = memory::GuestAddressSpaceViewMut::from_vmcs(
        &vcpu.vmcs,
        &mut vm.guest_space,
    )?;

    let bytes = view.read_bytes(
        ip_addr,
        instruction_len as usize,
        memory::GuestAccess::Read(memory::PrivilegeLevel(0)),
    )?;
    drop(vm);

    let efer = vcpu.vmcs.read_field(vmcs::VmcsField::GuestIa32Efer)?;
    // TODO: 16bit support
    let mode = if efer & 0x00000100 != 0 { 64 } else { 32 };

    let mut decoder =
        iced_x86::Decoder::new(mode, &bytes, iced_x86::DecoderOptions::NONE);
    decoder.set_ip(ip);
    let instr = decoder.decode();

    let addr = memory::GuestPhysAddr::new(
        vcpu.vmcs
            .read_field(vmcs::VmcsField::GuestPhysicalAddress)?,
    );

    // For now, just assume everything is like MOV. This is obviously very
    // incomplete.
    if instr.op0_kind() == iced_x86::OpKind::Memory
        || instr.op0_kind() == iced_x86::OpKind::Memory64
    {
        do_mmio_write(addr, vcpu, guest_cpu, responses, instr)?;
    } else if instr.op1_kind() == iced_x86::OpKind::Memory
        || instr.op1_kind() == iced_x86::OpKind::Memory64
    {
        do_mmio_read(addr, vcpu, guest_cpu, responses, instr)?;
    } else {
        return Err(Error::InvalidValue(format!(
            "Unsupported mmio instruction: {:?} (rip=0x{:x}, bytes={:?})",
            instr.code(),
            ip,
            bytes,
        )));
    }

    Ok(())
}
