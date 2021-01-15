use crate::error::{Error, Result};
use crate::memory;
use crate::virtdev::{
    DeviceEvent, MemReadRequest, MemWriteRequest, ResponseEventArray,
};
use crate::{vcpu, vm, vmcs, vmexit};
use arrayvec::ArrayVec;
use byteorder::ByteOrder;
use core::mem::size_of;
use iced_x86;
use x86::bits64::paging::BASE_PAGE_SIZE;

trait MemIoCallback:
    Fn(
    &mut vcpu::VCpu,
    memory::GuestPhysAddr,
    DeviceEvent,
    &mut ResponseEventArray,
) -> Result<()>
{
}

impl<T> MemIoCallback for T where
    T: Fn(
        &mut vcpu::VCpu,
        memory::GuestPhysAddr,
        DeviceEvent,
        &mut ResponseEventArray,
    ) -> Result<()>
{
}

macro_rules! read_register {
    ($out:ident, $value:expr, $type:ty) => {{
        let data = ($value as $type).to_be_bytes();
        $out.try_extend_from_slice(&data).map_err(|_| {
            Error::InvalidValue("Invalid length with reading register".into())
        })?;
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
    on_write: impl MemIoCallback,
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

    (on_write)(
        vcpu,
        addr,
        crate::virtdev::DeviceEvent::MemWrite(addr, request),
        responses,
    )
}

fn do_mmio_read(
    addr: memory::GuestPhysAddr,
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    responses: &mut ResponseEventArray,
    instr: iced_x86::Instruction,
    on_read: impl MemIoCallback,
) -> Result<()> {
    let (reg, size, offset) = match instr.op0_kind() {
        iced_x86::OpKind::Register => match instr.op_register(0) {
            iced_x86::Register::AL => (&mut guest_cpu.rax, size_of::<u8>(), 0),
            iced_x86::Register::AX => (&mut guest_cpu.rax, size_of::<u16>(), 0),
            iced_x86::Register::EAX => {
                (&mut guest_cpu.rax, size_of::<u32>(), 0)
            }
            iced_x86::Register::RAX => {
                (&mut guest_cpu.rax, size_of::<u64>(), 0)
            }

            iced_x86::Register::BL => (&mut guest_cpu.rbx, size_of::<u8>(), 0),
            iced_x86::Register::BX => (&mut guest_cpu.rbx, size_of::<u16>(), 0),
            iced_x86::Register::EBX => {
                (&mut guest_cpu.rbx, size_of::<u32>(), 0)
            }
            iced_x86::Register::RBX => {
                (&mut guest_cpu.rbx, size_of::<u64>(), 0)
            }

            iced_x86::Register::CL => (&mut guest_cpu.rcx, size_of::<u8>(), 0),
            iced_x86::Register::CX => (&mut guest_cpu.rcx, size_of::<u16>(), 0),
            iced_x86::Register::ECX => {
                (&mut guest_cpu.rcx, size_of::<u32>(), 0)
            }
            iced_x86::Register::RCX => {
                (&mut guest_cpu.rcx, size_of::<u64>(), 0)
            }

            iced_x86::Register::DL => (&mut guest_cpu.rdx, size_of::<u8>(), 0),
            iced_x86::Register::DX => (&mut guest_cpu.rdx, size_of::<u16>(), 0),
            iced_x86::Register::EDX => {
                (&mut guest_cpu.rdx, size_of::<u32>(), 0)
            }
            iced_x86::Register::RDX => {
                (&mut guest_cpu.rdx, size_of::<u64>(), 0)
            }

            iced_x86::Register::R8L => (&mut guest_cpu.r8, size_of::<u8>(), 0),
            iced_x86::Register::R8W => (&mut guest_cpu.r8, size_of::<u16>(), 0),
            iced_x86::Register::R8D => (&mut guest_cpu.r8, size_of::<u32>(), 0),
            iced_x86::Register::R8 => (&mut guest_cpu.r8, size_of::<u64>(), 0),

            iced_x86::Register::R9L => (&mut guest_cpu.r9, size_of::<u8>(), 0),
            iced_x86::Register::R9W => (&mut guest_cpu.r9, size_of::<u16>(), 0),
            iced_x86::Register::R9D => (&mut guest_cpu.r9, size_of::<u32>(), 0),
            iced_x86::Register::R9 => (&mut guest_cpu.r9, size_of::<u64>(), 0),

            iced_x86::Register::R10L => {
                (&mut guest_cpu.r10, size_of::<u8>(), 0)
            }
            iced_x86::Register::R10W => {
                (&mut guest_cpu.r10, size_of::<u16>(), 0)
            }
            iced_x86::Register::R10D => {
                (&mut guest_cpu.r10, size_of::<u32>(), 0)
            }
            iced_x86::Register::R10 => {
                (&mut guest_cpu.r10, size_of::<u64>(), 0)
            }

            iced_x86::Register::R11L => {
                (&mut guest_cpu.r11, size_of::<u8>(), 0)
            }
            iced_x86::Register::R11W => {
                (&mut guest_cpu.r11, size_of::<u16>(), 0)
            }
            iced_x86::Register::R11D => {
                (&mut guest_cpu.r11, size_of::<u32>(), 0)
            }
            iced_x86::Register::R11 => {
                (&mut guest_cpu.r11, size_of::<u64>(), 0)
            }

            iced_x86::Register::R12L => {
                (&mut guest_cpu.r12, size_of::<u8>(), 0)
            }
            iced_x86::Register::R12W => {
                (&mut guest_cpu.r12, size_of::<u16>(), 0)
            }
            iced_x86::Register::R12D => {
                (&mut guest_cpu.r12, size_of::<u32>(), 0)
            }
            iced_x86::Register::R12 => {
                (&mut guest_cpu.r12, size_of::<u64>(), 0)
            }

            iced_x86::Register::R13L => {
                (&mut guest_cpu.r13, size_of::<u8>(), 0)
            }
            iced_x86::Register::R13W => {
                (&mut guest_cpu.r13, size_of::<u16>(), 0)
            }
            iced_x86::Register::R13D => {
                (&mut guest_cpu.r13, size_of::<u32>(), 0)
            }
            iced_x86::Register::R13 => {
                (&mut guest_cpu.r13, size_of::<u64>(), 0)
            }

            iced_x86::Register::R14L => {
                (&mut guest_cpu.r14, size_of::<u8>(), 0)
            }
            iced_x86::Register::R14W => {
                (&mut guest_cpu.r14, size_of::<u16>(), 0)
            }
            iced_x86::Register::R14D => {
                (&mut guest_cpu.r14, size_of::<u32>(), 0)
            }
            iced_x86::Register::R14 => {
                (&mut guest_cpu.r14, size_of::<u64>(), 0)
            }

            iced_x86::Register::R15L => {
                (&mut guest_cpu.r15, size_of::<u8>(), 0)
            }
            iced_x86::Register::R15W => {
                (&mut guest_cpu.r15, size_of::<u16>(), 0)
            }
            iced_x86::Register::R15D => {
                (&mut guest_cpu.r15, size_of::<u32>(), 0)
            }
            iced_x86::Register::R15 => {
                (&mut guest_cpu.r15, size_of::<u64>(), 0)
            }

            iced_x86::Register::DIL => (&mut guest_cpu.rdi, size_of::<u8>(), 0),
            iced_x86::Register::DI => (&mut guest_cpu.rdi, size_of::<u16>(), 0),
            iced_x86::Register::EDI => {
                (&mut guest_cpu.rdi, size_of::<u32>(), 0)
            }
            iced_x86::Register::RDI => {
                (&mut guest_cpu.rdi, size_of::<u64>(), 0)
            }

            iced_x86::Register::SIL => (&mut guest_cpu.rsi, size_of::<u8>(), 0),
            iced_x86::Register::SI => (&mut guest_cpu.rsi, size_of::<u16>(), 0),
            iced_x86::Register::ESI => {
                (&mut guest_cpu.rsi, size_of::<u32>(), 0)
            }
            iced_x86::Register::RSI => {
                (&mut guest_cpu.rsi, size_of::<u64>(), 0)
            }

            register => {
                return Err(Error::InvalidValue(format!(
                    "mmio read into invalid register '{:?}'",
                    register
                )))
            }
        },
        _ => return Err(Error::NotSupported),
    };

    let mut arr = [0u8; size_of::<u64>()];
    let request = MemReadRequest::new(&mut arr[..size]);
    (on_read)(vcpu, addr, DeviceEvent::MemRead(addr, request), responses)?;

    let (value, mask) = match size {
        1 => (arr[0] as u64, ((u8::MAX as u64) << (offset * 8))),
        2 => (
            byteorder::LittleEndian::read_u16(&arr[..size]) as u64,
            (u16::MAX as u64) << (offset * 8),
        ),
        4 => (
            byteorder::LittleEndian::read_u32(&arr[..size]) as u64,
            (u32::MAX as u64) << (offset * 8),
        ),
        8 => (
            byteorder::LittleEndian::read_u64(&arr[..size]) as u64,
            (u64::MAX as u64) << (offset * 8),
        ),
        _ => unreachable!(),
    };

    *reg &= !mask;
    *reg |= value << (offset * 8);

    Ok(())
}

fn process_memio_op(
    addr: memory::GuestPhysAddr,
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    responses: &mut ResponseEventArray,
    on_read: impl MemIoCallback,
    on_write: impl MemIoCallback,
) -> Result<()> {
    let instruction_len = vcpu
        .vmcs
        .read_field(vmcs::VmcsField::VmExitInstructionLen)?;
    let ip = vcpu.vmcs.read_field(vmcs::VmcsField::GuestRip)?;

    let ip_addr = memory::GuestVirtAddr::new(ip, &vcpu.vmcs)?;
    let view = memory::GuestAddressSpaceView::from_vmcs(
        &vcpu.vmcs,
        &vcpu.vm.guest_space,
    )?;

    let bytes = view.read_bytes(
        ip_addr,
        instruction_len as usize,
        memory::GuestAccess::Read(memory::PrivilegeLevel(0)),
    )?;

    let efer = vcpu.vmcs.read_field(vmcs::VmcsField::GuestIa32Efer)?;
    // TODO: 16bit support
    let mode = if efer & 0x00000100 != 0 { 64 } else { 32 };

    let mut decoder =
        iced_x86::Decoder::new(mode, &bytes, iced_x86::DecoderOptions::NONE);
    decoder.set_ip(ip);
    let instr = decoder.decode();

    // For now, just assume everything is like MOV. This is obviously very
    // incomplete.
    if instr.op0_kind() == iced_x86::OpKind::Memory
        || instr.op0_kind() == iced_x86::OpKind::Memory64
    {
        do_mmio_write(addr, vcpu, guest_cpu, responses, instr, on_write)?;
    } else if instr.op1_kind() == iced_x86::OpKind::Memory
        || instr.op1_kind() == iced_x86::OpKind::Memory64
    {
        do_mmio_read(addr, vcpu, guest_cpu, responses, instr, on_read)?;
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

pub fn handle_ept_violation(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    _exit: vmexit::EptInformation,
    responses: &mut ResponseEventArray,
) -> Result<()> {
    fn on_ept_violation(
        vcpu: &mut vcpu::VCpu,
        addr: memory::GuestPhysAddr,
        event: DeviceEvent,
        responses: &mut ResponseEventArray,
    ) -> Result<()> {
        vcpu.vm.dispatch_event(addr, event, vcpu, responses)
    }

    let addr = memory::GuestPhysAddr::new(
        vcpu.vmcs
            .read_field(vmcs::VmcsField::GuestPhysicalAddress)?,
    );

    process_memio_op(
        addr,
        vcpu,
        guest_cpu,
        responses,
        on_ept_violation,
        on_ept_violation,
    )
}

pub fn handle_apic_access(
    vcpu: &mut vcpu::VCpu,
    guest_cpu: &mut vmexit::GuestCpuState,
    exit: vmexit::ApicAccessInformation,
    responses: &mut ResponseEventArray,
) -> Result<()> {
    fn address_to_apic_offset(addr: memory::GuestPhysAddr) -> u16 {
        let addr = addr.as_u64();
        let apic_base = vm::GUEST_LOCAL_APIC_ADDR.as_u64();
        assert!(
            addr >= apic_base && addr < (apic_base + BASE_PAGE_SIZE as u64)
        );
        ((addr - apic_base) / size_of::<u32>() as u64) as u16
    }

    fn on_apic_read(
        vcpu: &mut vcpu::VCpu,
        addr: memory::GuestPhysAddr,
        event: DeviceEvent,
        _responses: &mut ResponseEventArray,
    ) -> Result<()> {
        let offset = address_to_apic_offset(addr);
        let res = vcpu.local_apic.register_read(offset)?;
        let mut bytes = res.to_be_bytes();

        match event {
            DeviceEvent::MemRead(_, mut req) => {
                req.as_mut_slice().copy_from_slice(&mut bytes[..]);
                Ok(())
            }
            _ => return Err(Error::NotSupported),
        }
    }

    fn on_apic_write(
        vcpu: &mut vcpu::VCpu,
        addr: memory::GuestPhysAddr,
        event: DeviceEvent,
        _responses: &mut ResponseEventArray,
    ) -> Result<()> {
        let offset = address_to_apic_offset(addr);
        let mut bytes = [0u8; 4];
        let value = match event {
            DeviceEvent::MemWrite(_, req) => {
                bytes[..].copy_from_slice(req.as_slice());
                u32::from_be_bytes(bytes)
            }
            _ => return Err(Error::NotSupported),
        };

        vcpu.local_apic
            .register_write(vcpu.vm.clone(), offset, value)
    }

    let addr = vm::GUEST_LOCAL_APIC_ADDR
        + (exit.offset.expect("Apic access with no offset") as usize
            * size_of::<u32>());

    process_memio_op(
        addr,
        vcpu,
        guest_cpu,
        responses,
        on_apic_read,
        on_apic_write,
    )
}
