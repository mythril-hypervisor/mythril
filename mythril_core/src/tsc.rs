use crate::device::pit;
use crate::error::Result;
use x86::io::{inb, outb};

static mut TSC_KHZ: Option<u64> = None;

const CALIBRATE_COUNT: u16 = 0x800; // Approx 1.7ms
const PIT_HZ: u64 = 1193182;

const PPCB_T2GATE: u8 = 1 << 0;
const PPCB_SPKR: u8 = 1 << 1;
const PPCB_T2OUT: u8 = 1 << 5;

pub unsafe fn calibrate() -> Result<()> {
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
    TSC_KHZ = Some(tsc_khz);

    Ok(())
}

pub fn tsc_khz() -> u64 {
    unsafe { TSC_KHZ.unwrap() }
}

pub fn read_tsc() -> u64 {
    unsafe { x86::time::rdtsc() }
}
