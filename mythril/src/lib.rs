#![cfg_attr(not(std), no_std)]
#![feature(asm)]
#![feature(never_type)]
#![feature(const_fn)]
#![feature(get_mut_unchecked)]
#![feature(fixed_size_array)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]
#![feature(lang_items)]
#![feature(stmt_expr_attributes)]
#![feature(negative_impls)]
#![feature(map_first_last)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

// Needed as 'the build-std feature currently provides no
// way to enable the mem feature of compiler_builtins. You
// need to add a dependency on the rlibc crate.'
extern crate rlibc;

/// Support for ACPI.
pub mod acpi;
pub mod ap;
/// Support for the local APIC.
pub mod apic;
pub mod boot_info;
/// User configuration format
pub mod config;
pub mod emulate;
pub mod error;
pub mod global_alloc;
pub mod interrupt;
pub mod ioapic;
pub mod kmain;
pub mod linux;
pub mod lock;
pub mod logger;
pub mod memory;
pub mod multiboot;
pub mod multiboot2;
pub mod percore;
pub mod physdev;
pub mod registers;
pub mod time;
pub mod tsc;
pub mod vcpu;
pub mod virtdev;
/// Top level virtual machine definition
pub mod vm;
pub mod vmcs;
pub mod vmexit;
pub mod vmx;
