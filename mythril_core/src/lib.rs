#![cfg_attr(not(std), no_std)]
#![feature(asm)]
#![feature(never_type)]
#![feature(const_fn)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

pub mod allocator;
pub mod device;
pub mod error;
pub mod memory;
pub mod pci;
mod percpu;
mod registers;
pub mod vm;
pub mod vmcs;
mod vmexit;
pub mod vmx;
