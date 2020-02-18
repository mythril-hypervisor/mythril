use nasm_rs;

fn main() {
    nasm_rs::Build::new()
        .file("src/header.S")
        .file("src/boot.S")
        .include("../mythril_core/asm_include/")
        .target("x86_64-unknown-none")
        .compile("header");
    println!("cargo:rustc-link-lib=static=header");
}
