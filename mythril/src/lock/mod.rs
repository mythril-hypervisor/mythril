pub mod ro_after_init;

/// Provides a hint to the processor that it is in a spin loop
#[inline(always)]
pub fn relax_cpu() {
    unsafe {
        llvm_asm!("rep; nop" ::: "memory");
    }
}
