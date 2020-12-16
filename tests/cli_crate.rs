use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn cli_no_args() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .assert()
        .failure()
        .stderr(predicate::str::contains("For more information try --help"))
        .stdout(predicate::str::is_empty());
}

#[test]
fn cli_help() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OPTIONS:"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_ephemeral_offline() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-0")
        .arg("--offline")
        .arg("busybox:musl")
        .assert()
        .failure()
        .stderr(predicate::str::contains("DownloadInOfflineMode"))
        .stdout(predicate::str::is_empty());
}

#[test]
fn cli_busybox_echo() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .arg("world!")
        .assert()
        .success()
        .stdout(predicate::eq("hello world!\n"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_busybox_sh_c_echo() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("echo hello world\n echo and so on; echo $?")
        .assert()
        .success()
        .stdout(predicate::eq("hello world\nand so on\n0\n"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_busybox_sh_c_for() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("for i in apple banana banana phone; do echo $i extreme; done")
        .assert()
        .success()
        .stdout(predicate::eq(
            "apple extreme\nbanana extreme\nbanana extreme\nphone extreme\n",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_busybox_env_0() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::eq(
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\n",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_busybox_env_1() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("-e")
        .arg("foo")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::eq(
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\nfoo=\n",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_busybox_env_2() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("-e")
        .arg("foo")
        .arg("-e")
        .arg("blah=ok")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::eq(
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\nfoo=\nblah=ok\n",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn cli_busybox_env_3() {
    Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("bandsocks-cli")
        .arg("--")
        .arg("-l")
        .arg("error")
        .arg("-e")
        .arg("foo")
        .arg("-e")
        .arg("blah=ok")
        .arg("-e")
        .arg("YEP=cool")
        .arg("-e")
        .arg("blah=whynot")
        .arg("busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114")
        .arg("--")
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::eq(concat!(
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\nfoo=\n",
            "blah=whynot\nYEP=cool\n",
        )))
        .stderr(predicate::str::is_empty());
}
