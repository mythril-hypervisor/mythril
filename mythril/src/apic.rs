#![deny(missing_docs)]

use crate::error::{Error, Result};
use crate::time;
use crate::{declare_per_core, get_per_core, get_per_core_mut};
use num_enum::TryFromPrimitive;
use raw_cpuid::CpuId;
use x86::msr;

use core::fmt;

/// APIC base physical address mask.
const IA32_APIC_BASE_MASK: u64 = 0xffff_f000;
/// xAPIC global enable mask
const IA32_APIC_BASE_EN: u64 = 1 << 11;
/// x2APIC enable mask
const IA32_APIC_BASE_EXD: u64 = 1 << 10;
/// BSP mask
const IA32_APIC_BASE_BSP: u64 = 1 << 8;

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
/// ICR destination shorthand values
pub enum DstShorthand {
    /// No shorthand used
    NoShorthand = 0x00,
    /// Send only to myself
    MySelf = 0x01,
    /// Broadcast including myself
    AllIncludingSelf = 0x02,
    /// Broadcast excluding myself
    AllExcludingSelf = 0x03,
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
/// INIT IPI Level
pub enum Level {
    /// INIT IPI Level De-Assert
    DeAssert = 0x00,
    /// INIT IPI Level Assert
    Assert = 0x01,
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
/// ICR trigger modes
pub enum TriggerMode {
    /// Edge sensitive
    Edge = 0x00,
    /// Level sensitive
    Level = 0x01,
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
/// ICR mode of the Destination field
pub enum DstMode {
    /// Physical ID
    Physical = 0x00,
    /// Logical ID
    Logical = 0x01,
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
/// ICR delivery mode
pub enum DeliveryMode {
    /// Send interrupt vector to target
    Fixed = 0x00,
    #[doc(hidden)]
    _Reserved0 = 0x01,
    /// Send SMI interrupt to target
    SMI = 0x02,
    #[doc(hidden)]
    _Reserved1 = 0x03,
    /// Send NMI interrupt to target
    NMI = 0x04,
    /// Send INIT interrupt to target
    Init = 0x05,
    /// Send Start Up to target
    StartUp = 0x06,
    #[doc(hidden)]
    _Reserved2 = 0x07,
}

declare_per_core! {
    static mut LOCAL_APIC: Option<LocalApic> = None;
}

/// Obtain a reference to the current core's LocalApic
pub fn get_local_apic() -> &'static LocalApic {
    get_per_core!(LOCAL_APIC)
        .as_ref()
        .expect("Attempt to get local APIC before initialization")
}

/// Obtain a mutable reference to the current core's LocalApic
///
/// The caller must ensure that calling this function does not
/// cause soundness violations such as holding two mutable
/// references or a mutable and immutable reference.
pub unsafe fn get_local_apic_mut() -> &'static mut LocalApic {
    get_per_core_mut!(LOCAL_APIC)
        .as_mut()
        .expect("Attempt to get local APIC before initialization")
}

/// A representation of a APIC ID
#[derive(Copy, Clone, Debug, Ord, PartialEq, PartialOrd, Eq)]
pub struct ApicId {
    /// The raw ID as an integer
    pub raw: u32,
}

impl ApicId {
    /// Returns whether this is the BSP core
    pub fn is_bsp(&self) -> bool {
        //TODO(alschwalm): This is not correct for multi socket systems
        self.raw == 0
    }
}

impl From<u32> for ApicId {
    fn from(value: u32) -> Self {
        ApicId { raw: value }
    }
}

impl fmt::Display for ApicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self.raw)
    }
}

/// Structure defining the interface for a local x2APIC
#[derive(Debug)]
pub struct LocalApic {
    /// The raw value of the `IA32_APIC_BASE_MSR`
    base_reg: u64,

    ticks_per_ms: u64,
}

impl LocalApic {
    /// Create a new local x2APIC
    ///
    /// # Panics
    ///
    ///  - The CPU does not support X2APIC
    ///  - Unable to get the `cpuid`
    pub fn init() -> Result<&'static mut Self> {
        // Ensure the CPU supports x2apic
        let cpuid = CpuId::new();
        match cpuid.get_feature_info() {
            Some(finfo) if !finfo.has_x2apic() => {
                return Err(Error::NotSupported);
            }
            Some(_) => (),
            None => {
                return Err(Error::NotSupported);
            }
        };

        // Fetch the raw APIC BASE MSR
        let raw_base = unsafe { msr::rdmsr(msr::IA32_APIC_BASE) };

        // If the EXD bit is set, the EN bit must be set as well and
        // there is no need to flip it again.
        if raw_base & IA32_APIC_BASE_EXD == 0 {
            unsafe {
                msr::wrmsr(
                    msr::IA32_APIC_BASE,
                    raw_base | IA32_APIC_BASE_EN | IA32_APIC_BASE_EXD,
                );
            }
        }

        // Fetch the new value of the APIC BASE MSR
        let base_reg = unsafe { msr::rdmsr(msr::IA32_APIC_BASE) };

        let mut apic = LocalApic {
            base_reg,
            ticks_per_ms: 0,
        };

        // Enable the APIC in the Spurious Interrupt Vector Register
        unsafe {
            msr::wrmsr(msr::IA32_X2APIC_SIVR, 1 << 8);
        }

        // Clear the Error Status Register and read from it.
        //
        // TODO(dlrobertson):
        //
        // Many implementations seem to "Pound the ESR really hard over
        // the head with a big hammer" here, but there is nothing in the
        // spec that indicates why this is necessary. See
        // arch/x86/kernel/apic/apic.c for details.
        //
        // The spec states the following in § 2.3.5.4
        //
        // > A write (of any value) to the ESR must be done to update the
        // > register before attempting to read it.
        //
        // It makes no mention of reading the ESR after clearing it, but
        // a few implementations were found that read and discarded the
        // initial value of the MSR. For now we'll just stick to what the
        // spec says, but this should be investigated a bit further.
        apic.clear_esr();

        apic.calibrate_timer()
            .expect("Failed to calibrate APIC timer");

        let lapic = get_per_core_mut!(LOCAL_APIC);
        *lapic = Some(apic);
        Ok(lapic.as_mut().unwrap())
    }

    /// The APIC ID
    pub fn id(&self) -> ApicId {
        ApicId {
            raw: unsafe { msr::rdmsr(msr::IA32_X2APIC_APICID) as u32 },
        }
    }

    /// The Logical APIC ID
    ///
    /// From the x2apic spec § 2.4.4:
    ///
    /// > Logical x2APIC ID = \[(x2APIC ID\[31:4\] << 16) | (1 << x2APIC ID\[3:0\])\]
    pub fn logical_id(&self) -> u32 {
        let id = unsafe { msr::rdmsr(msr::IA32_X2APIC_APICID) as u32 };
        ((id & 0xffff_fff0) << 16) | (1 << (id & 0xf))
    }

    /// Processor is Bootstrap Processor
    pub fn bsp(&self) -> bool {
        (self.base_reg & IA32_APIC_BASE_BSP) != 0
    }

    /// Clear the Error Status Register
    pub fn clear_esr(&self) {
        unsafe {
            msr::wrmsr(msr::IA32_X2APIC_ESR, 0x00);
        }
    }

    /// Read the Error Status Register
    pub fn esr(&self) -> u64 {
        unsafe { msr::rdmsr(msr::IA32_X2APIC_ESR) }
    }

    /// The APIC Base Physical Address
    pub fn base(&self) -> u64 {
        self.base_reg & IA32_APIC_BASE_MASK
    }

    /// The raw APIC Base register
    pub fn raw_base(&self) -> u64 {
        self.base_reg
    }

    /// The local APIC Version
    pub fn version(&self) -> u32 {
        unsafe { msr::rdmsr(msr::IA32_X2APIC_VERSION) as u32 }
    }

    /// Send a End Of Interrupt
    pub fn eoi(&mut self) {
        unsafe {
            msr::wrmsr(msr::IA32_X2APIC_EOI, 0x00);
        }
    }

    /// Read the Interrupt Command Register
    pub fn icr(&self) -> u64 {
        unsafe { msr::rdmsr(msr::IA32_X2APIC_ICR) }
    }

    /// Set the Interrupt Command Register
    pub fn send_ipi(
        &mut self,
        dst: ApicId,
        dst_short: DstShorthand,
        trigger: TriggerMode,
        level: Level,
        dst_mode: DstMode,
        delivery_mode: DeliveryMode,
        vector: u8,
    ) {
        let mut icr: u64 = (dst.raw as u64) << 32;
        icr |= (dst_short as u64) << 18;
        icr |= (trigger as u64) << 15;
        icr |= (level as u64) << 14;
        icr |= (dst_mode as u64) << 11;
        icr |= (delivery_mode as u64) << 8;
        icr |= vector as u64;
        // TODO(dlrobertson): Should we check for illegal vectors?
        unsafe {
            msr::wrmsr(msr::IA32_X2APIC_ICR, icr);
        }
    }

    /// Send a IPI to yourself
    pub fn self_ipi(&mut self, vector: u8) {
        // TODO(dlrobertson): Should we check for illegal vectors?
        unsafe {
            msr::wrmsr(msr::IA32_X2APIC_SELF_IPI, vector as u64);
        }
    }

    fn calibrate_timer(&mut self) -> Result<()> {
        unsafe {
            let start_tick = 0xFFFFFFFF;
            msr::wrmsr(msr::IA32_X2APIC_DIV_CONF, 0x3); // timer divisor = 16
            msr::wrmsr(msr::IA32_X2APIC_INIT_COUNT, start_tick);
            time::busy_wait(core::time::Duration::from_millis(1));
            msr::wrmsr(msr::IA32_X2APIC_LVT_TIMER, 1 << 16); // Disable the timer
            let curr_tick = msr::rdmsr(msr::IA32_X2APIC_CUR_COUNT);
            self.ticks_per_ms = start_tick - curr_tick;
        }
        Ok(())
    }

    /// Configure the timer for this local apic to generate an interrupt with
    /// the requested vector at the requested time. This will clear any outstanding
    /// apic interrupt.
    pub fn schedule_interrupt(&mut self, when: time::Instant, vector: u8) {
        //TODO: always round _up_ here to avoid the timer not actually being
        // expired when we receive the interrupt
        let micros = (when - time::now()).as_micros();
        let ticks = micros * self.ticks_per_ms as u128 / 1000;
        unsafe {
            msr::wrmsr(msr::IA32_X2APIC_DIV_CONF, 0x3); // timer divisor = 16
            msr::wrmsr(msr::IA32_X2APIC_LVT_TIMER, vector as u64);
            msr::wrmsr(msr::IA32_X2APIC_INIT_COUNT, ticks as u64);
        }
    }
}
