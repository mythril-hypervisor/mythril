use crate::device::{DeviceRegion, EmulatedDevice, Port, PortReadRequest, PortWriteRequest};
use crate::error::Result;
use crate::tsc;
use alloc::boxed::Box;
use alloc::vec::Vec;

const PMTIMER_HZ: u64 = 3579545;

pub struct AcpiRuntime {
    pm_base: Port,
}

impl AcpiRuntime {
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

    pub fn new(pm_base: Port) -> Result<Box<Self>> {
        Ok(Box::new(AcpiRuntime { pm_base }))
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

    fn on_port_read(
        &mut self,
        port: Port,
        mut val: PortReadRequest,
    ) -> Result<()> {
        if port == self.pmtimer() {
            let pm_time =
                tsc::read_tsc() * PMTIMER_HZ / (tsc::tsc_khz() * 1000);
            info!("pm_time={}", pm_time);
            val.copy_from_u32(pm_time as u32);
        }
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortWriteRequest) -> Result<()> {
        info!(
            "Attempt to write to AcpiRuntime port=0x{:x}, val={:?}. Ignoring",
            port, val
        );
        Ok(())
    }
}
