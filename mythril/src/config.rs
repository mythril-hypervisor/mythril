#![deny(missing_docs)]

use crate::percore;

use alloc::string::String;
use core::fmt;
use serde::de::{self, Visitor};
use serde::export::Vec;
use serde::{Deserialize, Deserializer};

/// A description of a single virtual machine configuration
#[derive(Deserialize, Debug)]
pub struct UserVmConfig {
    /// Memory in MB available to the virtual machine
    pub memory: u64,

    /// The multiboot identifier for the kernel this virtual machine will use
    pub kernel: String,

    /// The multiboot identifier for the initramfs this virtual machine will use
    pub initramfs: String,

    /// The kernel commandline for this virtual machine
    pub cmdline: String,

    /// A list of core ID's (starting from 0) used by this machine
    pub cpus: Vec<percore::CoreId>,
}

/// The top level Mythril configuration
#[derive(Deserialize, Debug)]
pub struct UserConfig {
    /// Version number for this configuration
    pub version: u64,

    /// A list of virtual machine configurations
    pub vms: Vec<UserVmConfig>,
}

struct CoreIdVisitor;

impl<'de> Visitor<'de> for CoreIdVisitor {
    type Value = percore::CoreId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a core id")
    }

    fn visit_u64<E>(self, value: u64) -> core::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok((value as u32).into())
    }
}

impl<'de> Deserialize<'de> for percore::CoreId {
    fn deserialize<D>(
        deserializer: D,
    ) -> core::result::Result<percore::CoreId, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_u64(CoreIdVisitor)
    }
}
