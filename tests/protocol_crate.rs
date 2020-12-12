use std::process::Command;

#[test]
fn cargo_test_protocol() {
    assert!(Command::new("cargo")
        .current_dir("protocol")
        .arg("test")
        .status()
        .unwrap()
        .success());
}
