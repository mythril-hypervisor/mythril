use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = env::var_os("OUT_DIR").map(PathBuf::from)
        .expect("Missing output directory");

    Command::new("x86_64-w64-mingw32-gcc")
        .args(&[
            "src/vmexit.S",
            "-c",
            "-fPIC",
            "-ffreestanding",
            "-mno-red-zone",
            "-nostdlib",
            "-fno-stack-protector",
            "-fno-builtin",
            "-fno-exceptions",
            "-o",
        ])
        .arg(&format!("{}/vmexit.o", out_dir.display()))
        .status()
        .unwrap();
    Command::new("x86_64-w64-mingw32-ar")
        .args(&["crus", "vmexit.lib", "vmexit.o"])
        .current_dir(&Path::new(&out_dir))
        .status()
        .unwrap();

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=vmexit");
}
