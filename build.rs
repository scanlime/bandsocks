use build_deps::rerun_if_changed_paths;
use fs_extra::{copy_items, dir::CopyOptions};
use std::{
    env::var,
    fs::{copy, create_dir_all},
    path::Path,
    process::Command,
};

fn main() {
    // skip building sand when running in rust-language-server
    if var("RUSTC_WORKSPACE_WRAPPER").is_ok() {
        return;
    }

    let cargo = var("CARGO").unwrap();
    let out_dir = var("OUT_DIR").unwrap();
    let build_dir = Path::new(&out_dir).join("sand");
    create_dir_all(&build_dir).unwrap();

    rerun_if_changed_paths("sand/sand-Cargo.toml").unwrap();
    rerun_if_changed_paths("sand/sand-Cargo.lock").unwrap();
    rerun_if_changed_paths("sand/src/**/*.rs").unwrap();

    let mut opts = CopyOptions::new();
    opts.copy_inside = true;
    opts.skip_exist = true;
    copy("sand/sand-Cargo.toml", build_dir.join("Cargo.toml")).unwrap();
    copy("sand/sand-Cargo.lock", build_dir.join("Cargo.lock")).unwrap();
    copy_items(&["sand/src"], &build_dir, &opts).unwrap();

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
