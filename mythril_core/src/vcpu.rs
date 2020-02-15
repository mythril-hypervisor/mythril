use crate::device::{Port, PortIoValue};
use crate::error::{self, Error, Result};
use crate::memory::{self, Raw4kPage};
use crate::registers::{GdtrBase, IdtrBase};
use crate::vm::VirtualMachine;
use crate::{vmcs, vmexit, vmx};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::mem;
use core::pin::Pin;
use raw_cpuid::CpuId;
use spin::RwLock;
use x86::controlregs::{cr0, cr3, cr4};
use x86::msr;

extern "C" {
    pub fn vmlaunch_wrapper() -> u64;
}

pub fn smp_entry_point(vm_map: &'static BTreeMap<usize, Arc<RwLock<VirtualMachine>>>) -> ! {
    let cpuid = CpuId::new();
    let apicid = match cpuid.get_feature_info() {
        Some(finfo) => finfo.initial_local_apic_id() as usize,
        _ => panic!("Unable to get cpuid"),
    };
    let vm = vm_map.get(&apicid).expect("Failed to locate VM for VCPU");
    let vcpu = VCpu::new(vm.clone()).expect("Failed to create vcpu");
    vcpu.launch().expect("Failed to launch vm")
}

pub struct VCpu {
    vm: Arc<RwLock<VirtualMachine>>,
    pub vmcs: vmcs::ActiveVmcs,
    stack: Vec<u8>,
}

impl VCpu {
    pub fn new(vm: Arc<RwLock<VirtualMachine>>) -> Result<Pin<Box<Self>>> {
        let vmx = vmx::Vmx::enable()?;
        let vmcs = vmcs::Vmcs::new()?.activate(vmx)?;

        // Allocate 1MB for host stack space
        let stack = vec![0u8; 1024 * 1024];

        let mut vcpu = Box::pin(Self {
            vm: vm,
            vmcs: vmcs,
            stack: stack,
        });

        let eptp = vcpu.vm.read().guest_space.eptp();
        vcpu.vmcs.write_field(vmcs::VmcsField::EptPointer, eptp)?;

        let stack_base = vcpu.stack.as_ptr() as u64 + vcpu.stack.len() as u64
            - mem::size_of::<*const Self>() as u64;

        // 'push' the address of this VCpu to the host stack for the vmexit
        let raw_vcpu: *mut Self = (&mut *vcpu) as *mut Self;
        unsafe {
            core::ptr::write(stack_base as *mut *mut Self, raw_vcpu);
        }

        Self::initialize_host_vmcs(&mut vcpu.vmcs, stack_base)?;
        Self::initialize_guest_vmcs(&mut vcpu.vmcs)?;
        Self::initialize_ctrl_vmcs(&mut vcpu.vmcs)?;

        Ok(vcpu)
    }

    pub fn launch(self: Pin<Box<Self>>) -> Result<!> {
        let rflags = unsafe { vmlaunch_wrapper() };
        error::check_vm_insruction(rflags, "Failed to launch vm".into())?;

        unreachable!()
    }

    fn initialize_host_vmcs(vmcs: &mut vmcs::ActiveVmcs, stack: u64) -> Result<()> {
        //TODO: Check with MSR_IA32_VMX_CR0_FIXED0/1 that these bits are valid
        vmcs.write_field(vmcs::VmcsField::HostCr0, unsafe { cr0() }.bits() as u64)?;

        let current_cr3 = unsafe { cr3() };
        vmcs.write_field(vmcs::VmcsField::HostCr3, current_cr3)?;
        vmcs.write_field(vmcs::VmcsField::HostCr4, unsafe { cr4() }.bits() as u64)?;

        vmcs.write_field(vmcs::VmcsField::HostEsSelector, 0x00)?;

        //FIXME: The segment selector values are valid for OVMF specifically
        vmcs.write_field(vmcs::VmcsField::HostCsSelector, 0x38)?;
        vmcs.write_field(vmcs::VmcsField::HostSsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostDsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostEsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostFsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostGsSelector, 0x30)?;
        vmcs.write_field(vmcs::VmcsField::HostTrSelector, 0x30)?;

        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterCs, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEsp, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEip, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::HostIdtrBase, IdtrBase::read())?;
        vmcs.write_field(vmcs::VmcsField::HostGdtrBase, GdtrBase::read())?;

        vmcs.write_field(vmcs::VmcsField::HostFsBase, unsafe {
            msr::rdmsr(msr::IA32_FS_BASE)
        })?;
        vmcs.write_field(vmcs::VmcsField::HostFsBase, unsafe {
            msr::rdmsr(msr::IA32_GS_BASE)
        })?;

        vmcs.write_field(vmcs::VmcsField::HostRsp, stack)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32Efer, unsafe {
            msr::rdmsr(msr::IA32_EFER)
        })?;

        vmcs.write_field(
            vmcs::VmcsField::HostRip,
            vmexit::vmexit_handler_wrapper as u64,
        )?;
        Ok(())
    }

    fn initialize_guest_vmcs(vmcs: &mut vmcs::ActiveVmcs) -> Result<()> {
        vmcs.write_field(vmcs::VmcsField::GuestEsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsSelector, 0xf000)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrSelector, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestEsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsBase, 0xffff0000)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestIdtrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestGdtrBase, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestEsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestSsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestTrLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestLdtrLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestIdtrLimit, 0xffff)?;
        vmcs.write_field(vmcs::VmcsField::GuestGdtrLimit, 0xffff)?;

        vmcs.write_field(vmcs::VmcsField::GuestEsArBytes, 0x0093)?; // read/write
        vmcs.write_field(vmcs::VmcsField::GuestSsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestDsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestFsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestGsArBytes, 0x0093)?;
        vmcs.write_field(vmcs::VmcsField::GuestCsArBytes, 0x009b)?; // exec/read
        vmcs.write_field(vmcs::VmcsField::GuestLdtrArBytes, 0x0082)?; // LDT
        vmcs.write_field(vmcs::VmcsField::GuestTrArBytes, 0x008b)?; // TSS (busy)

        vmcs.write_field(vmcs::VmcsField::GuestInterruptibilityInfo, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestActivityState, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestDr7, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestRsp, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::GuestRflags, 1 << 1)?; // Reserved rflags

        vmcs.write_field(vmcs::VmcsField::VmcsLinkPointer, 0xffffffff)?;
        vmcs.write_field(vmcs::VmcsField::VmcsLinkPointerHigh, 0xffffffff)?;

        //TODO: get actual EFER (use MSR for vt-x v1)
        vmcs.write_field(vmcs::VmcsField::GuestIa32Efer, 0x00)?;

        let (guest_cr0, guest_cr4) = {
            let mut cr0_fixed0 = unsafe { msr::rdmsr(msr::IA32_VMX_CR0_FIXED0) };
            cr0_fixed0 &= !(1 << 0); // disable PE
            cr0_fixed0 &= !(1 << 31); // disable PG
            let cr4_fixed0 = unsafe { msr::rdmsr(msr::IA32_VMX_CR4_FIXED0) };

            vmcs.write_field(
                vmcs::VmcsField::Cr4GuestHostMask,
                cr4_fixed0 & 0x00000000ffffffff,
            )?;

            (cr0_fixed0, cr4_fixed0)
        };

        vmcs.write_field(vmcs::VmcsField::GuestCr0, guest_cr0)?;
        vmcs.write_field(vmcs::VmcsField::GuestCr4, guest_cr4)?;
        vmcs.write_field(vmcs::VmcsField::Cr4ReadShadow, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::GuestCr3, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::GuestRip, 0xfff0)?;

        Ok(())
    }

    fn initialize_ctrl_vmcs(vmcs: &mut vmcs::ActiveVmcs) -> Result<()> {
        vmcs.write_with_fixed(
            vmcs::VmcsField::CpuBasedVmExecControl,
            (vmcs::CpuBasedCtrlFlags::UNCOND_IO_EXITING
                | vmcs::CpuBasedCtrlFlags::ACTIVATE_MSR_BITMAP
                | vmcs::CpuBasedCtrlFlags::ACTIVATE_SECONDARY_CONTROLS)
                .bits(),
            msr::IA32_VMX_PROCBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::SecondaryVmExecControl,
            (vmcs::SecondaryExecFlags::VIRTUALIZE_APIC_ACCESSES
                | vmcs::SecondaryExecFlags::ENABLE_EPT
                | vmcs::SecondaryExecFlags::ENABLE_VPID
                | vmcs::SecondaryExecFlags::UNRESTRICTED_GUEST)
                .bits(),
            msr::IA32_VMX_PROCBASED_CTLS2,
        )?;
        vmcs.write_field(vmcs::VmcsField::VirtualProcessorId, 1)?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::PinBasedVmExecControl,
            0,
            msr::IA32_VMX_PINBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmExitControls,
            (vmcs::VmExitCtrlFlags::IA32E_MODE
                | vmcs::VmExitCtrlFlags::LOAD_HOST_EFER
                | vmcs::VmExitCtrlFlags::SAVE_GUEST_EFER)
                .bits(),
            msr::IA32_VMX_EXIT_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmEntryControls,
            vmcs::VmEntryCtrlFlags::LOAD_GUEST_EFER.bits(),
            msr::IA32_VMX_ENTRY_CTLS,
        )?;

        let msr_bitmap = Box::into_raw(Box::new(Raw4kPage::default()));
        vmcs.write_field(vmcs::VmcsField::MsrBitmap, msr_bitmap as u64)?;

        vmcs.write_field(vmcs::VmcsField::ExceptionBitmap, 0xffffffff)?;

        let field = vmcs.read_field(vmcs::VmcsField::CpuBasedVmExecControl)?;
        info!("Flags: 0x{:x}", field);
        let flags = vmcs::CpuBasedCtrlFlags::from_bits_truncate(field);
        info!("Flags: {:?}", flags);

        let field = vmcs.read_field(vmcs::VmcsField::SecondaryVmExecControl)?;
        info!("Sec Flags: 0x{:x}", field);
        let flags = vmcs::SecondaryExecFlags::from_bits_truncate(field);
        info!("Sec Flags: {:?}", flags);

        vmcs.write_field(vmcs::VmcsField::Cr3TargetCount, 0)?;
        vmcs.write_field(vmcs::VmcsField::TprThreshold, 0)?;

        Ok(())
    }

    fn skip_emulated_instruction(&mut self) -> Result<()> {
        let mut rip = self.vmcs.read_field(vmcs::VmcsField::GuestRip)?;
        rip += self
            .vmcs
            .read_field(vmcs::VmcsField::VmExitInstructionLen)?;
        self.vmcs.write_field(vmcs::VmcsField::GuestRip, rip)?;

        //TODO: clear interrupts?
        Ok(())
    }

    fn emulate_outs(
        &mut self,
        port: Port,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::IoInstructionInformation,
    ) -> Result<()> {
        let mut vm = self.vm.write();

        let linear_addr = self.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
        let guest_addr = memory::GuestVirtAddr::new(linear_addr, &self.vmcs)?;

        // FIXME: This could actually be any priv level due to IOPL, but for now
        //        assume that is requires supervisor
        let access = memory::GuestAccess::Read(memory::PrivilegeLevel(0));

        // FIXME: The direction we read is determined by the DF flag (I think)
        // FIXME: We should probably only be using some of the lower order bits
        let bytes = vm.guest_space.read_bytes(
            &self.vmcs,
            guest_addr,
            (guest_cpu.rcx * exit.size as u64) as usize,
            access,
        )?;

        let dev = vm
            .config
            .device_map()
            .device_for_mut(port)
            .ok_or(Error::MissingDevice(format!("No device for port {}", port)))?;

        // FIXME: Actually test for REP
        for chunk in bytes.chunks_exact(exit.size as usize) {
            dev.on_port_write(port, PortIoValue::try_from(chunk)?)?;
        }

        guest_cpu.rsi += bytes.len() as u64;
        guest_cpu.rcx = 0;
        Ok(())
    }

    fn emulate_ins(
        &mut self,
        port: Port,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::IoInstructionInformation,
    ) -> Result<()> {
        let mut vm = self.vm.write();

        let dev = vm
            .config
            .device_map()
            .device_for_mut(port)
            .ok_or(Error::MissingDevice(format!("No device for port {}", port)))?;

        let linear_addr = self.vmcs.read_field(vmcs::VmcsField::GuestLinearAddress)?;
        let guest_addr = memory::GuestVirtAddr::new(linear_addr, &self.vmcs)?;
        let access = memory::GuestAccess::Read(memory::PrivilegeLevel(0));

        let mut bytes = vec![0u8; guest_cpu.rcx as usize];
        for chunk in bytes.chunks_exact_mut(exit.size as usize) {
            let mut val = PortIoValue::try_from(&*chunk)?;
            dev.on_port_read(port, &mut val)?;
            chunk.copy_from_slice(val.as_slice());
        }

        vm.guest_space
            .write_bytes(&self.vmcs, guest_addr, &bytes, access)?;

        guest_cpu.rdi += bytes.len() as u64;
        guest_cpu.rcx = 0;
        Ok(())
    }

    fn handle_portio(
        &mut self,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::IoInstructionInformation,
    ) -> Result<()> {
        let (port, input, size, string) = (exit.port, exit.input, exit.size, exit.string);

        if !string {
            let mut vm = self.vm.write();

            let dev = vm
                .config
                .device_map()
                .device_for_mut(port)
                .ok_or(Error::MissingDevice(format!("No device for port {}", port)))?;

            if !input {
                let arr = (guest_cpu.rax as u32).to_be_bytes();
                dev.on_port_write(port, PortIoValue::try_from(&arr[4 - size as usize..])?)?;
            } else {
                let mut val = match size {
                    1 => PortIoValue::OneByte([0]),
                    2 => PortIoValue::TwoBytes([0, 0]),
                    4 => PortIoValue::FourBytes([0, 0, 0, 0]),
                    _ => panic!("Invalid portio read size: {}", size),
                };
                dev.on_port_read(port, &mut val)?;
                guest_cpu.rax &= (!guest_cpu.rax) << (size * 8);
                guest_cpu.rax |= val.as_u32() as u64;
            }
        } else {
            if !input {
                self.emulate_outs(port, guest_cpu, exit)?;
            } else {
                self.emulate_ins(port, guest_cpu, exit)?;
            }
        }
        Ok(())
    }

    fn handle_ept_violation(
        &mut self,
        _guest_cpu: &mut vmexit::GuestCpuState,
        _exit: vmexit::EptInformation,
    ) -> Result<()> {
        let addr = self
            .vmcs
            .read_field(vmcs::VmcsField::GuestPhysicalAddress)?;
        info!("ept violation: guest phys addr = 0x{:x}", addr);
        Ok(())
    }

    pub fn handle_vmexit(
        &mut self,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::ExitReason,
    ) -> Result<()> {
        match exit.info {
            vmexit::ExitInformation::CrAccess(info) => {
                match info.cr_num {
                    0 => match info.access_type {
                        vmexit::CrAccessType::Clts => {
                            let cr0 = self.vmcs.read_field(vmcs::VmcsField::GuestCr0)?;
                            self.vmcs
                                .write_field(vmcs::VmcsField::GuestCr0, cr0 & !0b1000)?;
                        }
                        _ => unreachable!(),
                    },
                    3 => match info.access_type {
                        vmexit::CrAccessType::MovToCr => {
                            let reg = info.register.unwrap();
                            let val = reg.read(&self.vmcs, guest_cpu)?;
                            self.vmcs.write_field(vmcs::VmcsField::GuestCr3, val)?;
                        }
                        vmexit::CrAccessType::MovFromCr => {
                            let reg = info.register.unwrap();
                            let val = self.vmcs.read_field(vmcs::VmcsField::GuestCr3)?;
                            reg.write(val, &mut self.vmcs, guest_cpu)?;
                        }
                        _ => unreachable!(),
                    },
                    _ => return Err(Error::InvalidValue(format!("Unsupported CR number access"))),
                }

                self.skip_emulated_instruction()?;
            }

            vmexit::ExitInformation::CpuId => {
                //FIXME: for now just use the actual cpuid
                let mut res = raw_cpuid::native_cpuid::cpuid_count(
                    guest_cpu.rax as u32,
                    guest_cpu.rcx as u32,
                );

                // Disable MTRR support in the features info leaf (for now)
                if guest_cpu.rax as u32 == 1 {
                    res.edx &= !(1 << 12);
                }

                guest_cpu.rax = res.eax as u64 | (guest_cpu.rax & 0xffffffff00000000);
                guest_cpu.rbx = res.ebx as u64 | (guest_cpu.rbx & 0xffffffff00000000);
                guest_cpu.rcx = res.ecx as u64 | (guest_cpu.rcx & 0xffffffff00000000);
                guest_cpu.rdx = res.edx as u64 | (guest_cpu.rdx & 0xffffffff00000000);
                self.skip_emulated_instruction()?;
            }
            vmexit::ExitInformation::IoInstruction(info) => {
                self.handle_portio(guest_cpu, info)?;
                self.skip_emulated_instruction()?;
            }
            vmexit::ExitInformation::EptViolation(info) => {
                self.handle_ept_violation(guest_cpu, info)?;
                self.skip_emulated_instruction()?;
            }
            _ => info!("No handler for exit reason: {:?}", exit),
        }

        Ok(())
    }
}
