use build_deps::rerun_if_changed_paths;
use std::{fs::copy, env::var, path::Path, process::Command};

fn main() {
    let cargo = var("CARGO").unwrap();
    let out_dir = var("OUT_DIR").unwrap();
    let sand_target = Path::new(&out_dir).join("sand-target");
    let sand_dir = Path::new("sand");

    rerun_if_changed_paths("sand/sand-Cargo.toml").unwrap();
    rerun_if_changed_paths("sand/sand-Cargo.lock").unwrap();
    rerun_if_changed_paths("sand/src/**/*.rs").unwrap();

    // skip building sand when running in rust-language-server
    if var("RUSTC_WORKSPACE_WRAPPER").is_ok() {
        return;
    }

    // the inner cargo files stay masked to keep "cargo publish" from insisting on
    // skipping the directory because it thinks this is a properly separate
    // crate.
    copy(sand_dir.join("sand-Cargo.toml"), sand_dir.join("Cargo.toml")).unwrap();
    copy(sand_dir.join("sand-Cargo.lock"), sand_dir.join("Cargo.lock")).unwrap();

    let args = &[
        "build",
        "--release",
        "--target-dir",
        sand_target.to_str().unwrap(),
    ];

    // prefer to run rustup's cargo wrapper and explicitly ask for nightly.
    // sand needs nightly rust but we'd like to not require it for the outer app.
    let result = Command::new("cargo")
        .current_dir(sand_dir)
        .arg("+nightly")
        .args(args)
        .status();
    if result.is_ok() {
        assert!(result.unwrap().success());
        return;
    }

    assert!(Command::new(cargo)
        .current_dir(sand_dir)
        .args(args)
        .status()
        .unwrap()
        .success());
}
