use crate::apic::*;
use crate::error::{Error, Result};
use crate::memory;
use crate::percore;
use crate::vm;
use alloc::sync::Arc;
use core::convert::TryFrom;
use core::sync::atomic::AtomicU32;
use num_enum::TryFromPrimitive;

#[derive(Debug)]
enum ApicRegisterOffset {
    Simple(ApicRegisterSimpleOffset),
    InterruptRequest(u16),
    InterruptCommand(u16),
    TriggerMode(u16),
    InService(u16),
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u16)]
enum ApicRegisterSimpleOffset {
    ApicId = 0x20,
    ApicVersion = 0x30,
    TaskPriority = 0x80,
    ArbitrationPriority = 0x90,
    ProcessorPriority = 0xa0,
    EndOfInterrupt = 0xb0,
    RemoteRead = 0xc0,
    LogicalDestination = 0xd0,
    DestinationFormat = 0xe0,
    SpuriousInterruptVector = 0xf0,
    ErrorStatus = 0x280,
    LvtCorrectMachineCheckInterrupt = 0x2f0,
    LvtTimer = 0x320,
    LvtThermalSensor = 0x330,
    LvtPerformanceMonitoringCounter = 0x340,
    LvtLINT0 = 0x350,
    LvtLINT1 = 0x360,
    LvtError = 0x370,
    TimerInitialCount = 0x380,
    TimerCurrentCount = 0x390,
    TimerDivideConfig = 0x3e0,
}

/// The portion of guest local APIC state related to logical addressing
///
/// This portion of state must be 'shared' with other cores, because the
/// state of each local APIC's logical destination registers affects how
/// logically addressed IPIs are transmitted. Therefore, this state is
/// stored in the VirtualMachine instead of in each VCpu.
pub struct LogicalApicState {
    pub logical_destination: AtomicU32,
    pub destination_format: AtomicU32,
}

impl core::default::Default for LogicalApicState {
    fn default() -> Self {
        Self {
            logical_destination: AtomicU32::new(0),
            destination_format: AtomicU32::new(0),
        }
    }
}

impl TryFrom<u16> for ApicRegisterOffset {
    type Error = Error;

    fn try_from(value: u16) -> Result<ApicRegisterOffset> {
        if value & 0b1111 != 0 {
            return Err(Error::InvalidValue(format!(
                "APIC register offset not aligned: 0x{:x}",
                value
            )));
        }

        if let Ok(simple_reg) = ApicRegisterSimpleOffset::try_from(value) {
            return Ok(ApicRegisterOffset::Simple(simple_reg));
        }

        let res = match value {
            0x100..=0x170 => {
                ApicRegisterOffset::InService((value - 0x100) >> 4)
            }
            0x180..=0x1f0 => {
                ApicRegisterOffset::TriggerMode((value - 0x180) >> 4)
            }
            0x200..=0x270 => {
                ApicRegisterOffset::InterruptRequest((value - 0x200) >> 4)
            }
            0x300..=0x310 => {
                ApicRegisterOffset::InterruptCommand((value - 0x300) >> 4)
            }
            offset => {
                return Err(Error::InvalidValue(format!(
                    "Invalid APIC register offset: 0x{:x}",
                    offset
                )))
            }
        };

        Ok(res)
    }
}

#[derive(Default)]
pub struct LocalApic {
    icr_destination: Option<u32>,
}

impl LocalApic {
    pub fn new() -> Self {
        LocalApic {
            icr_destination: None,
        }
    }

    fn process_sipi_request(&self, value: u32) -> Result<()> {
        // TODO(alschwalm): check the destination and delivery modes to
        // be sure this is actually what we should be doing.
        if let Some(dest) = self.icr_destination {
            let vector = value as u64 & 0xff;
            let addr = memory::GuestPhysAddr::new(vector << 12);

            // FIXME(alschwalm): The destination is actually a virtual local
            // apic id. We should convert that to a global core id for this.
            let core_id = percore::CoreId::from(dest >> 24);

            debug!(
                "Sending startup message for address = {:?} to core {}",
                addr, core_id
            );

            vm::virtual_machines().send_msg_core(
                vm::VirtualMachineMsg::StartVcpu(addr),
                core_id,
                false,
            )?;
        }
        Ok(())
    }

    fn process_interrupt_command(
        &mut self,
        vm: Arc<vm::VirtualMachine>,
        value: u32,
    ) -> Result<()> {
        let mode = DeliveryMode::try_from((value >> 8) as u8 & 0b111)?;
        match mode {
            // TODO: for now, just ignore the INIT signal
            DeliveryMode::Init => return Ok(()),
            DeliveryMode::StartUp => return self.process_sipi_request(value),
            _ => (),
        }

        let vector = value as u64 & 0xff;
        let dst_mode = DstMode::try_from((value >> 11 & 0b1) as u8)?;
        let shorthand = DstShorthand::try_from((value >> 18 & 0b11) as u8)?;

        match shorthand {
            DstShorthand::AllIncludingSelf => {
                warn!("Unsupported local apic command register shorthand AllIncludingSelf");
                return Ok(());
            }
            DstShorthand::AllExcludingSelf => {
                for core in vm.config.cpus() {
                    if *core == percore::read_core_id() {
                        continue;
                    }
                    vm::virtual_machines().send_msg_core(vm::VirtualMachineMsg::GuestInterrupt{
                        kind: crate::vcpu::InjectedInterruptType::ExternalInterrupt,
                        vector: vector as u8
                    }, *core, true)?
                }
                return Ok(());
            }
            DstShorthand::MySelf => {
                warn!("Unsupported local apic command register shorthand Self");
                return Ok(());
            }
            DstShorthand::NoShorthand => (),
        }

        // No shorthand was used, so we should have an ICR destination of some sort
        if let Some(dest) = self.icr_destination {
            match dst_mode {
                DstMode::Logical => {
                    for core in vm.logical_apic_destination(dest)? {
                        // FIXME(alschwalm): we need to support sending to ourselves (I think)
                        if *core == percore::read_core_id() {
                            continue;
                        }
                        vm::virtual_machines().send_msg_core(vm::VirtualMachineMsg::GuestInterrupt{
                            kind: crate::vcpu::InjectedInterruptType::ExternalInterrupt,
                            vector: vector as u8
                        }, *core, true)?
                    }
                }
                DstMode::Physical => {
                    warn!("Unsupported Physical address for IPI vector=0x{:x} (dest=0x{:x}/short={:?}/mode={:?})",
                          vector, dest, shorthand, mode);
                }
            }
        } else {
            warn!("IPI with no icr_destination vector=0x{:x} (dest={:?}/short={:?})",
                  vector, dst_mode, shorthand);
        }

        Ok(())
    }

    pub fn register_read(&mut self, offset: u16) -> Result<u32> {
        let offset = ApicRegisterOffset::try_from(offset)?;
        match offset {
            ApicRegisterOffset::Simple(ApicRegisterSimpleOffset::ApicId) => {
                // FIXME(alschwalm): we shouldn't really use the core id for this
                Ok(percore::read_core_id().raw)
            }
            _ => Ok(0),
        }
    }

    pub fn register_write(
        &mut self,
        vm: Arc<vm::VirtualMachine>,
        offset: u16,
        value: u32,
    ) -> Result<()> {
        let offset = ApicRegisterOffset::try_from(offset)?;
        match offset {
            ApicRegisterOffset::Simple(ref simple) => match simple {
                ApicRegisterSimpleOffset::EndOfInterrupt => (),
                ApicRegisterSimpleOffset::LogicalDestination => {
                    vm.update_core_logical_destination(value);
                }
                _ => info!(
                    "Write to virtual local apic: {:?}, value=0x{:x}",
                    offset, value
                ),
            },
            ApicRegisterOffset::InterruptCommand(offset) => {
                match offset {
                    0 => {
                        self.process_interrupt_command(vm, value)?;

                        // TODO(alschwalm): What is the expected behavior here?
                        self.icr_destination = None;
                    }
                    1 => {
                        self.icr_destination = Some(value);
                    }
                    _ => unreachable!(),
                }
            }
            _ => info!(
                "Write to virtual local apic: {:?}, value=0x{:x}",
                offset, value
            ),
        }
        Ok(())
    }
}
