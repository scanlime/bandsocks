use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn example_simple() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--example")
        .arg("simple")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("BusyBox v1.32.0"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn example_hello_world() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--example")
        .arg("hello-world")
        .assert()
        .success()
        .stdout(predicate::eq("hello world!\n"))
        .stderr(predicate::str::is_empty());
}
