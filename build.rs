use build_deps::rerun_if_changed_paths;
use fs_extra::{copy_items, dir::CopyOptions};
use std::{env::var, path::Path, process::Command};

fn main() {
    // skip building sand when running in rust-language-server
    if var("RUSTC_WORKSPACE_WRAPPER").is_ok() {
        return;
    }

    let cargo = var("CARGO").unwrap();
    let out_dir = var("OUT_DIR").unwrap();
    let build_dir = Path::new(&out_dir).join("sand");

    rerun_if_changed_paths("sand/sand-Cargo.toml").unwrap();
    rerun_if_changed_paths("sand/sand-Cargo.lock").unwrap();
    rerun_if_changed_paths("sand/src/**/*.rs").unwrap();
    copy_items(&["sand"], out_dir, &CopyOptions::new()).unwrap();

    let args = &["build", "--release"];

    // prefer to run rustup's cargo wrapper and explicitly ask for nightly.
    // sand needs nightly rust but we'd like to not require it for the outer app.
    let result = Command::new("cargo")
        .current_dir(&build_dir)
        .arg("+nightly")
        .args(args)
        .status();
    if result.is_ok() {
        assert!(result.unwrap().success());
        return;
    }

    assert!(Command::new(cargo)
        .current_dir(&build_dir)
        .args(args)
        .status()
        .unwrap()
        .success());
}
