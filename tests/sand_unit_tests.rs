use std::process::Command;

// This should use the same configuration as ../build.rs
#[test]
fn cargo_test_sand() {
    assert!(Command::new("cargo")
        .current_dir("sand")
        .arg("+nightly")
        .arg("test")
        .arg("--release")
        .status()
        .unwrap()
        .success());
}
