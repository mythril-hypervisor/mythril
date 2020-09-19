use crate::apic;
use crate::emulate;
use crate::error::{self, Error, Result};
use crate::memory::Raw4kPage;
use crate::percore;
use crate::registers::{GdtrBase, IdtrBase};
use crate::time;
use crate::vm::VirtualMachine;
use crate::{virtdev, vm, vmcs, vmexit, vmx};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem;
use core::pin::Pin;
use spin::RwLock;
use x86::controlregs::{cr0, cr3, cr4};
use x86::msr;

extern "C" {
    pub fn vmlaunch_wrapper() -> u64;
    static GDT64_CODE: u64;
    static GDT64_DATA: u64;
}

/// The post-startup point where a core begins executing its statically
/// assigned VCPU. Past this point, there is no distinction between BSP
/// and AP.
pub fn mp_entry_point() -> ! {
    unsafe {
        time::init_timer_wheel()
            .expect("Failed to initialize per-core timer wheel");
    }

    let vm = unsafe {
        vm::VM_MAP
            .as_ref()
            .unwrap()
            .get(&apic::get_local_apic().id())
            .expect("Failed to find VM for core")
            .clone()
    };
    let vcpu = VCpu::new(vm.clone()).expect("Failed to create vcpu");
    vcpu.launch().expect("Failed to launch vm")
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum InjectedInterruptType {
    ExternalInterrupt = 0,
    NonMaskableInterrupt = 2,
    HardwareException = 3,
    SoftwareInterrupt = 4,
    PrivilegedSoftwareException = 5,
    SoftwareException = 6,
    OtherEvent = 7,
}

/// A virtual CPU.
///
/// Each `VCpu` will be executed on a particular physical core, and is
/// associated with a particular `VirtualMachine`. The `VCpu` is responsible
/// for at least the initial handling of any VMEXIT (though in may cases the
/// ultimate handling will occur within an emulated device in the `VirtualMachine`'s
/// `DeviceMap`)
pub struct VCpu {
    pub vm: Arc<RwLock<VirtualMachine>>,
    pub vmcs: vmcs::ActiveVmcs,
    pending_interrupts: BTreeMap<u8, InjectedInterruptType>,
    stack: Vec<u8>,
}

impl VCpu {
    /// Create a new `VCpu` assocaited with the given `VirtualMachine`
    ///
    /// Note that the result must be `Pin`, as the `VCpu` pushes its own
    /// address on to the per-core host stack so it can be retrieved on
    /// VMEXIT.
    pub fn new(vm: Arc<RwLock<VirtualMachine>>) -> Result<Pin<Box<Self>>> {
        let vmx = vmx::Vmx::enable()?;
        let vmcs = vmcs::Vmcs::new()?.activate(vmx)?;

        // Allocate 1MB for host stack space
        let stack = vec![0u8; 1024 * 1024];

        let mut vcpu = Box::pin(Self {
            vm: vm,
            vmcs: vmcs,
            stack: stack,
            pending_interrupts: BTreeMap::new(),
        });

        // All VCpus in a VM must share the same address space (except for the
        // local apic)
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

    pub fn inject_interrupt(
        &mut self,
        vector: u8,
        kind: InjectedInterruptType,
    ) {
        self.pending_interrupts.insert(vector, kind);
    }

    /// Begin execution in the guest context for this core
    pub fn launch(self: Pin<Box<Self>>) -> Result<!> {
        let rflags = unsafe { vmlaunch_wrapper() };
        error::check_vm_insruction(rflags, "Failed to launch vm".into())?;

        unreachable!()
    }

    fn initialize_host_vmcs(
        vmcs: &mut vmcs::ActiveVmcs,
        stack: u64,
    ) -> Result<()> {
        //TODO: Check with MSR_IA32_VMX_CR0_FIXED0/1 that these bits are valid
        vmcs.write_field(
            vmcs::VmcsField::HostCr0,
            unsafe { cr0() }.bits() as u64,
        )?;

        let current_cr3 = unsafe { cr3() };
        vmcs.write_field(vmcs::VmcsField::HostCr3, current_cr3)?;
        vmcs.write_field(
            vmcs::VmcsField::HostCr4,
            unsafe { cr4() }.bits() as u64,
        )?;

        // Unsafe is required here due to reading an extern static
        unsafe {
            vmcs.write_field(vmcs::VmcsField::HostCsSelector, GDT64_CODE)?;
            vmcs.write_field(vmcs::VmcsField::HostSsSelector, GDT64_DATA)?;
            vmcs.write_field(vmcs::VmcsField::HostDsSelector, GDT64_DATA)?;
            vmcs.write_field(vmcs::VmcsField::HostEsSelector, GDT64_DATA)?;
            vmcs.write_field(vmcs::VmcsField::HostGsSelector, GDT64_DATA)?;
            vmcs.write_field(vmcs::VmcsField::HostTrSelector, GDT64_DATA)?;
        }

        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterCs, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEsp, 0x00)?;
        vmcs.write_field(vmcs::VmcsField::HostIa32SysenterEip, 0x00)?;

        vmcs.write_field(vmcs::VmcsField::HostIdtrBase, IdtrBase::read())?;
        vmcs.write_field(vmcs::VmcsField::HostGdtrBase, GdtrBase::read())?;

        vmcs.write_field(
            vmcs::VmcsField::HostFsSelector,
            percore::read_core_idx() << 3, // Skip the RPL and TI flags
        )?;

        vmcs.write_field(vmcs::VmcsField::HostFsBase, unsafe {
            msr::rdmsr(msr::IA32_FS_BASE)
        })?;

        vmcs.write_field(vmcs::VmcsField::HostGsBase, unsafe {
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
            let mut cr0_fixed0 =
                unsafe { msr::rdmsr(msr::IA32_VMX_CR0_FIXED0) };
            cr0_fixed0 &= !(1 << 0); // disable PE
            cr0_fixed0 &= !(1 << 31); // disable PG
            let cr4_fixed0 = unsafe { msr::rdmsr(msr::IA32_VMX_CR4_FIXED0) };

            vmcs.write_field(
                vmcs::VmcsField::Cr0GuestHostMask,
                cr0_fixed0 & 0x00000000ffffffff,
            )?;

            vmcs.write_field(
                vmcs::VmcsField::Cr4GuestHostMask,
                cr4_fixed0 & 0x00000000ffffffff,
            )?;

            (cr0_fixed0, cr4_fixed0)
        };

        vmcs.write_field(vmcs::VmcsField::GuestCr0, guest_cr0)?;
        vmcs.write_field(vmcs::VmcsField::GuestCr4, guest_cr4)?;
        vmcs.write_field(vmcs::VmcsField::Cr0ReadShadow, 0x00)?;
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
                | vmcs::SecondaryExecFlags::ENABLE_INVPCID
                | vmcs::SecondaryExecFlags::UNRESTRICTED_GUEST)
                .bits(),
            msr::IA32_VMX_PROCBASED_CTLS2,
        )?;

        // The value 0 is forbidden for the VPID, so we use the sequential
        // processor id plus 1.
        //
        //   26.2.1.1 VM-Execution Control Fields
        //   If the “enable VPID” VM-execution control is 1, the value of the VPID
        //   VM-execution control field must not be 0000H.
        vmcs.write_field(
            vmcs::VmcsField::VirtualProcessorId,
            percore::read_core_idx() + 1,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::PinBasedVmExecControl,
            vmcs::PinBasedCtrlFlags::EXT_INTR_EXIT.bits(),
            msr::IA32_VMX_PINBASED_CTLS,
        )?;

        vmcs.write_with_fixed(
            vmcs::VmcsField::VmExitControls,
            (vmcs::VmExitCtrlFlags::IA32E_MODE
                | vmcs::VmExitCtrlFlags::LOAD_HOST_EFER
                | vmcs::VmExitCtrlFlags::SAVE_GUEST_EFER
                | vmcs::VmExitCtrlFlags::ACK_INTR_ON_EXIT)
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

        // Do not VMEXIT on any exceptions
        vmcs.write_field(vmcs::VmcsField::ExceptionBitmap, 0x00000000)?;

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

        Ok(())
    }

    /// Handle an arbitrary guest VMEXIT.
    ///
    /// This is the rust 'entry' point when a guest exists.
    ///
    /// # Arguments
    ///
    /// * `guest_cpu` - A structure containing the current register values of the guest
    /// * `exit` - A representation of the VMEXIT reason
    pub fn handle_vmexit(
        &mut self,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::ExitReason,
    ) -> Result<()> {
        // Process the exit reason
        self.handle_vmexit_impl(guest_cpu, exit.clone())?;

        // Always check for expired timers
        unsafe {
            for (vec, kind) in
                time::get_timer_wheel_mut().expire_elapsed_timers()?
            {
                self.inject_interrupt(vec, kind);
            }
        }

        // If there are no pending interrupts, we're done
        if self.pending_interrupts.is_empty() {
            return Ok(());
        }

        let interruptibility = vmcs::InterruptibilityState::from_bits(
            self.vmcs
                .read_field(vmcs::VmcsField::GuestInterruptibilityInfo)?,
        )
        .ok_or_else(|| {
            Error::InvalidValue("Invalid interruptibility state".into())
        })?;

        let rflags = self.vmcs.read_field(vmcs::VmcsField::GuestRflags)?;

        // If the guest is not currently interruptible, set the interrupt window exiting
        // and exit. Otherwise, ensure that it is disabled.
        let field = self
            .vmcs
            .read_field(vmcs::VmcsField::CpuBasedVmExecControl)?;
        if !interruptibility.is_empty() || rflags & 0b1000000000 == 0 {
            self.vmcs.write_field(
                vmcs::VmcsField::CpuBasedVmExecControl,
                field
                    | vmcs::CpuBasedCtrlFlags::INTERRUPT_WINDOW_EXITING.bits(),
            )?;
            return Ok(());
        } else {
            self.vmcs.write_field(
                vmcs::VmcsField::CpuBasedVmExecControl,
                field
                    & !vmcs::CpuBasedCtrlFlags::INTERRUPT_WINDOW_EXITING.bits(),
            )?;
        }

        // At this point, we must have at least one pending interrupt, and the guest
        // can accept interrupts, so do the injection.
        if let Some(pending) = self.pending_interrupts.pop_first() {
            self.vmcs.write_field(
                vmcs::VmcsField::VmEntryIntrInfoField,
                0x80000000 | pending.0 as u64 | ((pending.1 as u64) << 8),
            )?;
        }

        // If there are still pending interrupts, set the interrupt window so
        // we should get a chance to do the injection once the guest is finished
        // handling the one we just injected.
        if !self.pending_interrupts.is_empty() {
            self.vmcs.write_field(
                vmcs::VmcsField::CpuBasedVmExecControl,
                field
                    | vmcs::CpuBasedCtrlFlags::INTERRUPT_WINDOW_EXITING.bits(),
            )?;
        } else {
            self.vmcs.write_field(
                vmcs::VmcsField::CpuBasedVmExecControl,
                field
                    & !vmcs::CpuBasedCtrlFlags::INTERRUPT_WINDOW_EXITING.bits(),
            )?;
        }

        Ok(())
    }

    fn handle_uart_keypress(
        &mut self,
        interrupts: &mut virtdev::InterruptArray,
    ) -> Result<()> {
        let vm = self.vm.read();

        let serial_info = vm
            .config
            .physical_devices()
            .serial
            .as_ref()
            .map(|serial| (serial.read(), serial.base_port()));
        drop(vm);

        let mut vm = self.vm.write();
        if let Some((key, port)) = serial_info {
            vm.on_event(
                self,
                &virtdev::DeviceMessage::UartKeyPressed(key),
                port,
                interrupts,
            )
        } else {
            Ok(())
        }
    }

    fn handle_vmexit_impl(
        &mut self,
        guest_cpu: &mut vmexit::GuestCpuState,
        exit: vmexit::ExitReason,
    ) -> Result<()> {
        // An array of outstanding interrupts/exceptions that should be injected
        // into the guest as a result of this vmexit
        let mut interrupts = virtdev::InterruptArray::default();

        match exit.info {
            vmexit::ExitInformation::CrAccess(info) => {
                match info.cr_num {
                    0 => match info.access_type {
                        vmexit::CrAccessType::Clts => {
                            let cr0 = self
                                .vmcs
                                .read_field(vmcs::VmcsField::GuestCr0)?;
                            self.vmcs.write_field(
                                vmcs::VmcsField::GuestCr0,
                                cr0 & !0b1000,
                            )?;
                        }
                        vmexit::CrAccessType::MovToCr => {
                            let reg = info.register.unwrap();
                            let val = reg.read(&self.vmcs, guest_cpu)?;
                            self.vmcs
                                .write_field(vmcs::VmcsField::GuestCr0, val)?;
                        }
                        op => panic!(
                            "Unsupported MovToCr cr0 operation: {:?}",
                            op
                        ),
                    },
                    3 => match info.access_type {
                        vmexit::CrAccessType::MovToCr => {
                            let reg = info.register.unwrap();
                            let val = reg.read(&self.vmcs, guest_cpu)?;
                            self.vmcs
                                .write_field(vmcs::VmcsField::GuestCr3, val)?;
                        }
                        vmexit::CrAccessType::MovFromCr => {
                            let reg = info.register.unwrap();
                            let val = self
                                .vmcs
                                .read_field(vmcs::VmcsField::GuestCr3)?;
                            reg.write(val, &mut self.vmcs, guest_cpu)?;
                        }
                        op => panic!(
                            "Unsupported MovFromCr cr0 operation: {:?}",
                            op
                        ),
                    },
                    _ => {
                        return Err(Error::InvalidValue(format!(
                            "Unsupported CR number access"
                        )))
                    }
                }

                self.skip_emulated_instruction()?;
            }

            vmexit::ExitInformation::CpuId => {
                emulate::cpuid::emulate_cpuid(self, guest_cpu)?;
                self.skip_emulated_instruction()?;
            }
            vmexit::ExitInformation::IoInstruction(info) => {
                emulate::portio::emulate_portio(
                    self,
                    guest_cpu,
                    info,
                    &mut interrupts,
                )?;
                self.skip_emulated_instruction()?;
            }
            vmexit::ExitInformation::EptViolation(info) => {
                emulate::memio::handle_ept_violation(
                    self,
                    guest_cpu,
                    info,
                    &mut interrupts,
                )?;
                self.skip_emulated_instruction()?;
            }
            vmexit::ExitInformation::WrMsr => {
                info!(
                    "wrmsr: {:x}:{:x} to register 0x{:x}",
                    guest_cpu.rdx as u32,
                    guest_cpu.rax as u32,
                    guest_cpu.rcx as u32
                );
            }
            vmexit::ExitInformation::InterruptWindow => {}
            vmexit::ExitInformation::ExternalInterrupt(info) => unsafe {
                match info.vector {
                    0x24 => self.handle_uart_keypress(&mut interrupts)?,
                    _ => (),
                }

                // Ack the interrupt via PIC or APIC based on the vector
                if info.vector < 48 {
                    //TODO: this should be a call to the physical PIC
                    x86::io::outb(0x20, 0x20);
                } else {
                    apic::get_local_apic_mut().eoi();
                }
            },
            _ => {
                info!("{}", self.vmcs);
                panic!("No handler for exit reason: {:?}", exit);
            }
        }

        for interrupt in interrupts {
            self.inject_interrupt(
                interrupt,
                InjectedInterruptType::ExternalInterrupt,
            );
        }

        Ok(())
    }
}
