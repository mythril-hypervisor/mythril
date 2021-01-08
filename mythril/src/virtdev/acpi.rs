use crate::error::Result;
use crate::time;
use crate::virtdev::{DeviceEvent, DeviceRegion, EmulatedDevice, Event, Port};
use alloc::vec::Vec;

const PMTIMER_HZ: u64 = 3579545;

pub struct AcpiRuntime {
    pm_base: Port,
}

impl AcpiRuntime {
    // This should actually be determind by the ACPI tables we're passing to the guest
    const FADT_SMI_COMMAND: Port = 0xb2;

    // Seabios expects us to pass PCI hotplug info via ACPI like QEMU.
    // See https://github.com/qemu/qemu/blob/master/docs/specs/acpi_pci_hotplug.txt
    const GPE_BLOCK_START: Port = 0xafe0;
    const GPE_BLOCK_END: Port = 0xafe3;
    const PCI_SLOT_INJECTION_START: Port = 0xae00;
    const PCI_SLOT_INJECTION_END: Port = 0xae03;
    const PCI_SLOT_REMOVAL_NOTIFY_START: Port = 0xae04;
    const PCI_SLOT_REMOVAL_NOTIFY_END: Port = 0xae07;
    const PCI_DEVICE_EJECT_START: Port = 0xae08;
    const PCI_DEVICE_EJECT_END: Port = 0xae0b;
    const PCI_REMOVABILITY_STATUS_START: Port = 0xae0c;
    const PCI_REMOVABILITY_STATUS_END: Port = 0xae0f;

    pub fn new(pm_base: Port) -> Result<Self> {
        Ok(AcpiRuntime { pm_base })
    }

    fn pm1a_cnt(&self) -> Port {
        self.pm_base + 0x04
    }

    fn pmtimer(&self) -> Port {
        self.pm_base + 0x08
    }
}

impl EmulatedDevice for AcpiRuntime {
    fn services(&self) -> Vec<DeviceRegion> {
        vec![
            DeviceRegion::PortIo(
                Self::FADT_SMI_COMMAND..=Self::FADT_SMI_COMMAND,
            ),
            DeviceRegion::PortIo(self.pm1a_cnt()..=self.pm1a_cnt()),
            DeviceRegion::PortIo(self.pmtimer()..=self.pmtimer()),
            DeviceRegion::PortIo(Self::GPE_BLOCK_START..=Self::GPE_BLOCK_END),
            DeviceRegion::PortIo(
                Self::PCI_SLOT_INJECTION_START..=Self::PCI_SLOT_INJECTION_END,
            ),
            DeviceRegion::PortIo(
                Self::PCI_SLOT_REMOVAL_NOTIFY_START
                    ..=Self::PCI_SLOT_REMOVAL_NOTIFY_END,
            ),
            DeviceRegion::PortIo(
                Self::PCI_DEVICE_EJECT_START..=Self::PCI_DEVICE_EJECT_END,
            ),
            DeviceRegion::PortIo(
                Self::PCI_REMOVABILITY_STATUS_START
                    ..=Self::PCI_REMOVABILITY_STATUS_END,
            ),
        ]
    }

    fn on_event(&mut self, event: Event) -> Result<()> {
        match event.kind {
            DeviceEvent::PortRead(port, mut val) => {
                if port == self.pmtimer() {
                    let on_duration = time::now() - time::system_start_time();
                    let pm_time = (on_duration.as_nanos() * PMTIMER_HZ as u128)
                        / 1_000_000_000;
                    val.copy_from_u32(pm_time as u32);
                }
            }
            DeviceEvent::PortWrite(port, val) => {
                info!(
                    "Attempt to write to AcpiRuntime port=0x{:x}, val={}. Ignoring",
                    port, val
                );
            }
            _ => (),
        }

        Ok(())
    }
}
