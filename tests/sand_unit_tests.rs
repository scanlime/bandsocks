use std::{fs::copy, process::Command, os::unix::fs::symlink, path::Path};

// This should use the same configuration as ../build.rs
#[test]
fn cargo_test_sand() {
    copy("sand/sand-Cargo.toml", "sand/Cargo.toml").unwrap();
    copy("sand/sand-Cargo.lock", "sand/Cargo.lock").unwrap();

    let protocol_link = Path::new("sand/protocol");
    if !protocol_link.exists() {
        symlink("../protocol", &protocol_link).unwrap();
    }

    assert!(Command::new("cargo")
        .current_dir("sand")
        .arg("+nightly")
        .arg("test")
        .arg("--release")
        .status()
        .unwrap()
        .success());
}
