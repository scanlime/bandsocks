use std::{fs::copy, process::Command};

// This should use the same configuration as ../build.rs
#[test]
fn cargo_test_sand() {
    copy("sand/sand-Cargo.toml", "sand/Cargo.toml").unwrap();
    copy("sand/sand-Cargo.lock", "sand/Cargo.lock").unwrap();
    assert!(Command::new("cargo")
        .current_dir("sand")
        .arg("+nightly")
        .arg("test")
        .arg("--release")
        .status()
        .unwrap()
        .success());
}
