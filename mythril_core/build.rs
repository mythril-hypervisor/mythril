use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let target = env::var_os("TARGET")
        .expect("Missing target")
        .into_string()
        .expect("Target is invalid UTF-8 string");
    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .expect("Missing output directory");

    let (gcc, ar, libname) = if target.contains("uefi") {
        (
            "x86_64-w64-mingw32-gcc",
            "x86_64-w64-mingw32-ar",
            "vmexit.lib",
        )
    } else {
        ("gcc", "ar", "libvmexit.a")
    };

    Command::new(gcc)
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

    Command::new(ar)
        .args(&["crus", libname, "vmexit.o"])
        .current_dir(&Path::new(&out_dir))
        .status()
        .unwrap();

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=vmexit");
}
