use alloc::string::String;
use serde::export::Vec;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct VmConfig {
    pub memory: u64,
    pub kernel: String,
    pub initramfs: String,
    pub cmdline: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub version: u64,
    pub vms: Vec<VmConfig>,
}
