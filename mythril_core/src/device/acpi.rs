use crate::device::{DeviceRegion, EmulatedDevice, Port, PortIoValue};
use crate::error::Result;
use crate::tsc;
use alloc::boxed::Box;
use alloc::vec::Vec;

const PMTIMER_HZ: u64 = 3579545;

pub struct AcpiRuntime {
    pm_base: Port,
}

impl AcpiRuntime {
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
        ]
    }

    fn on_port_read(&mut self, port: Port, val: &mut PortIoValue) -> Result<()> {
        if port == self.pmtimer() {
            let pm_time = tsc::read_tsc() * PMTIMER_HZ / (tsc::tsc_khz() * 1000);
            info!("pm_time={}", pm_time);
            val.copy_from_u32(pm_time as u32);
        }
        Ok(())
    }

    fn on_port_write(&mut self, port: Port, val: PortIoValue) -> Result<()> {
        info!(
            "Attempt to write to AcpiRuntime port=0x{:x}, val={:?}. Ignoring",
            port, val
        );
        Ok(())
    }
}
