use nasm_rs;

fn main() {
    nasm_rs::Build::new()
        .file("src/vm.S")
        .file("src/boot.S")
        .include("asm_include/")
        .target("x86_64-unknown-none")
        .compile("vm");
    println!("cargo:rustc-link-lib=static=vm");
}
