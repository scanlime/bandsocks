use build_deps::rerun_if_changed_paths;
use std::{env::var, path::Path, process::Command};

fn main() {
    let out_dir = var("OUT_DIR").unwrap();
    let sand_target = Path::new(&out_dir).join("sand-target");

    rerun_if_changed_paths("sand/Cargo.toml").unwrap();
    rerun_if_changed_paths("sand/src/**/*.rs").unwrap();

    // skip building sand when running in rust-language-server
    let skip_sand_build = var("RUSTC_WORKSPACE_WRAPPER").is_ok();

    if !skip_sand_build {
        assert!(Command::new("cargo")
            .current_dir("sand")
            .arg("+nightly")
            .arg("build")
            .arg("--release")
            .arg("--target-dir")
            .arg(sand_target)
            .status()
            .unwrap()
            .success());
    }
}
