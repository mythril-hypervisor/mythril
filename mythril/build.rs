use nasm_rs;

fn main() {
    // We must _not_ do this build under the test setup, because that
    // will produce a `_start` symbol that will conflict with the one
    // provided by the unittest harness.
    if cfg!(feature = "test") {
        return;
    }

    let mut build = nasm_rs::Build::new();
    build
        .file("src/vm.S")
        .file("src/boot.S")
        .file("src/multiboot_header.S")
        .file("src/multiboot2_header.S")
        .file("src/ap_startup.S")
        .include("asm_include/")
        .target("x86_64-unknown-none")
        .compile("vm");
    println!("cargo:rustc-link-lib=static=vm");
}
