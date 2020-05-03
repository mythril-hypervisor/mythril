use crate::device::pit;
use crate::error::{Error, Result};
use crate::time::{Instant, TimeSource};

use x86::io::{inb, outb};

const CALIBRATE_COUNT: u16 = 0x800; // Approx 1.7ms
const PIT_HZ: u64 = 1193182;

const PPCB_T2GATE: u8 = 1 << 0;
const PPCB_SPKR: u8 = 1 << 1;
const PPCB_T2OUT: u8 = 1 << 5;

struct TscTimeSource {
    frequency: u64,
}

unsafe fn read_tsc() -> u64 {
    x86::time::rdtsc()
}

impl TimeSource for TscTimeSource {
    fn now(&self) -> Instant {
        Instant(unsafe { read_tsc() })
    }

    fn frequency(&self) -> u64 {
        self.frequency
    }
}

static mut TSC: Option<TscTimeSource> = None;

pub unsafe fn calibrate_tsc() -> Result<&'static mut dyn TimeSource> {
    if TSC.is_some() {
        return Err(Error::InvalidValue("TSC is already calibrated".into()));
    }

    let orig: u8 = inb(pit::Pit8254::PIT_PS2_CTRL_B);
    outb(
        pit::Pit8254::PIT_PS2_CTRL_B,
        (orig & !PPCB_SPKR) | PPCB_T2GATE,
    );

    outb(
        pit::Pit8254::PIT_MODE_CONTROL,
        ((pit::Channel::Channel2 as u8) << 6)
            | ((pit::AccessMode::Word as u8) << 4)
            | ((pit::OperatingMode::Mode0 as u8) << 1)
            | pit::BinaryMode::Binary as u8,
    );

    /* LSB of ticks */
    outb(pit::Pit8254::PIT_COUNTER_2, (CALIBRATE_COUNT & 0xFF) as u8);
    /* MSB of ticks */
    outb(pit::Pit8254::PIT_COUNTER_2, (CALIBRATE_COUNT >> 8) as u8);

    let start = read_tsc();
    while (inb(pit::Pit8254::PIT_PS2_CTRL_B) & PPCB_T2OUT) == 0 {}
    let end = read_tsc();

    outb(pit::Pit8254::PIT_PS2_CTRL_B, orig);

    let diff = end - start;
    let tsc_khz = (diff * PIT_HZ) / (CALIBRATE_COUNT as u64 * 1000);

    info!("tsc calibrate diff={} (khz={})", diff, tsc_khz);

    let source = TscTimeSource {
        frequency: tsc_khz * 1000,
    };

    TSC = Some(source);
    Ok(TSC.as_mut().unwrap())
}
