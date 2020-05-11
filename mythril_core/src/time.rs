#![deny(missing_docs)]

//! # Abstract time support
//!
//! This module contains types and traits related to time keeping in
//! Mythril. Note that this does not include _date_ information, only
//! abstract system clock, counter, and timer information.

use crate::error::Result;
use crate::tsc;

use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

static mut TIME_SRC: Option<&'static mut dyn TimeSource> = None;
static mut START_TIME: Option<Instant> = None;

/// Determine the best available global system `TimeSource` and calibrate it.
pub unsafe fn init_global_time() -> Result<()> {
    // Currently we only support using the TSC
    TIME_SRC = Some(tsc::calibrate_tsc()?);
    START_TIME = Some(now());
    Ok(())
}

/// Get the current instant from the global system `TimeSource`.
pub fn now() -> Instant {
    unsafe {
        TIME_SRC
            .as_ref()
            .expect("Global time source is not calibrated")
            .now()
    }
}

/// Get the instant the system was started (approximately) in terms
/// of the global system `TimeSource`.
pub fn system_start_time() -> Instant {
    unsafe { START_TIME.expect("Global time source is not started") }
}

/// Returns whether the global system `TimeSource` has be initialized.
pub fn is_global_time_ready() -> bool {
    unsafe { TIME_SRC.is_some() }
}

fn frequency() -> u64 {
    unsafe {
        TIME_SRC
            .as_ref()
            .expect("Global time source is not calibrated")
            .frequency()
    }
}

/// A point in time on the system in terms of the global system `TimeSource`
///
/// An `Instant` can be added/subtracted with a `Duration` to produce an
/// `Instant` in the future or past. However, this requires that the global
/// system time source be initialized, otherwise it will panic.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct Instant(pub u64);

impl Add<Duration> for Instant {
    type Output = Self;

    fn add(self, other: Duration) -> Self {
        let ticks = (frequency() as u128 * other.as_nanos()) / 1_000_000_000;
        Instant(self.0 + ticks as u64)
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, other: Duration) {
        *self = *self + other;
    }
}

impl Sub<Duration> for Instant {
    type Output = Self;

    fn sub(self, other: Duration) -> Self {
        let ticks = (frequency() as u128 * other.as_nanos()) / 1_000_000_000;
        Instant(self.0 - ticks as u64)
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, other: Duration) {
        *self = *self - other;
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, other: Self) -> Duration {
        let ticks = (self.0 as i128 - other.0 as i128).abs() as u64;
        let ns = (ticks as u128 * 1_000_000_000) / frequency() as u128;
        Duration::from_nanos(ns as u64)
    }
}

/// A trait representing a counter on the system with a consistent frequency.
pub trait TimeSource {
    /// The current value of the counter.
    fn now(&self) -> Instant;

    /// The frequency this counter increments in ticks per second.
    fn frequency(&self) -> u64;
}

enum TimerMode {
    OneShot,
    Periodic,
}

/// A one-shot or periodic timer.
pub struct Timer {
    duration: Duration,
    mode: TimerMode,
    started: Option<Instant>,
}

impl Timer {
    /// Create a new one-shot timer.
    pub fn one_shot(duration: Duration) -> Self {
        Self {
            duration: duration,
            mode: TimerMode::OneShot,
            started: None,
        }
    }

    /// Create a new periodic timer.
    pub fn periodic(period: Duration) -> Self {
        Self {
            duration: period,
            mode: TimerMode::Periodic,
            started: None,
        }
    }

    /// Start the timer.
    pub fn start(&mut self) {
        self.started = Some(now());
    }

    /// Stop the timer.
    pub fn stop(&mut self) {
        self.started = None;
    }

    /// Returns whether the timer has been started.
    pub fn started(&self) -> bool {
        self.started.is_some()
    }

    /// Returns whether the timer is periodic
    pub fn is_periodic(&self) -> bool {
        match self.mode {
            TimerMode::Periodic => true,
            TimerMode::OneShot => false,
        }
    }

    /// Returns whether the timer has elapsed
    ///
    /// Note that for a periodic timer, `reset` must still be called
    /// before `elapsed` will return false after having elapsed once.
    pub fn elapsed(&self) -> bool {
        if !self.started() {
            return false;
        }

        now() - self.started.unwrap() > self.duration
    }

    /// Reset this timer
    ///
    /// Set the effective start time for this timer to 'now' (as determined
    /// bye the global system timer). This does nothing if the timer has not
    /// been started.
    pub fn reset(&mut self) {
        if !self.started() {
            return;
        }
        self.started = Some(now());
    }
}
