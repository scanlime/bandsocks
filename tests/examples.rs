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

#[test]
fn example_pull_progress() {
    let output = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--example")
        .arg("pull-progress")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines: Vec<&str> = stdout.split("\n").collect();
    lines.sort();

    let mut event_progress_count = 0;
    let mut lines_to_compare: Vec<&str> = Vec::new();
    for line in lines {
        if line.contains("event: Progress(") {
            event_progress_count += 1;
        } else if !line.is_empty() {
            lines_to_compare.push(line);
        }
    }

    println!("{} {:?}", event_progress_count, lines_to_compare);
    assert!(event_progress_count > 500);
    assert_eq!(lines_to_compare, vec![
            "done: Image(ubuntu@sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f)",
            "done: Image(ubuntu@sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f)",
            "update: ProgressUpdate { resource: Blob(sha256:75ac8019c9736cea870759cf87bf752344fd3fb2dbf7a3921e9c60e140213900), phase: Connect, event: Begin }",
            "update: ProgressUpdate { resource: Blob(sha256:75ac8019c9736cea870759cf87bf752344fd3fb2dbf7a3921e9c60e140213900), phase: Connect, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:75ac8019c9736cea870759cf87bf752344fd3fb2dbf7a3921e9c60e140213900), phase: Decompress, event: BeginSized(163) }",
            "update: ProgressUpdate { resource: Blob(sha256:75ac8019c9736cea870759cf87bf752344fd3fb2dbf7a3921e9c60e140213900), phase: Decompress, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:75ac8019c9736cea870759cf87bf752344fd3fb2dbf7a3921e9c60e140213900), phase: Download, event: BeginSized(163) }",
            "update: ProgressUpdate { resource: Blob(sha256:75ac8019c9736cea870759cf87bf752344fd3fb2dbf7a3921e9c60e140213900), phase: Download, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:7a14fb4cd302ea60d4b208f17bb50098b52a17183a2137c08299a3b915d7cbae), phase: Connect, event: Begin }",
            "update: ProgressUpdate { resource: Blob(sha256:7a14fb4cd302ea60d4b208f17bb50098b52a17183a2137c08299a3b915d7cbae), phase: Connect, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:7a14fb4cd302ea60d4b208f17bb50098b52a17183a2137c08299a3b915d7cbae), phase: Decompress, event: BeginSized(31337938) }",
            "update: ProgressUpdate { resource: Blob(sha256:7a14fb4cd302ea60d4b208f17bb50098b52a17183a2137c08299a3b915d7cbae), phase: Decompress, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:7a14fb4cd302ea60d4b208f17bb50098b52a17183a2137c08299a3b915d7cbae), phase: Download, event: BeginSized(31337938) }",
            "update: ProgressUpdate { resource: Blob(sha256:7a14fb4cd302ea60d4b208f17bb50098b52a17183a2137c08299a3b915d7cbae), phase: Download, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:b8b1fecc905c746712dc231f73c5d630927892af991ce140673eb77dd9b697cd), phase: Connect, event: Begin }",
            "update: ProgressUpdate { resource: Blob(sha256:b8b1fecc905c746712dc231f73c5d630927892af991ce140673eb77dd9b697cd), phase: Connect, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:b8b1fecc905c746712dc231f73c5d630927892af991ce140673eb77dd9b697cd), phase: Decompress, event: BeginSized(844) }",
            "update: ProgressUpdate { resource: Blob(sha256:b8b1fecc905c746712dc231f73c5d630927892af991ce140673eb77dd9b697cd), phase: Decompress, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:b8b1fecc905c746712dc231f73c5d630927892af991ce140673eb77dd9b697cd), phase: Download, event: BeginSized(844) }",
            "update: ProgressUpdate { resource: Blob(sha256:b8b1fecc905c746712dc231f73c5d630927892af991ce140673eb77dd9b697cd), phase: Download, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:da5958a2de8e69f762888ac8df90e995c74b5643f0a72ae09176f178de25b67c), phase: Connect, event: Begin }",
            "update: ProgressUpdate { resource: Blob(sha256:da5958a2de8e69f762888ac8df90e995c74b5643f0a72ae09176f178de25b67c), phase: Connect, event: Complete }",
            "update: ProgressUpdate { resource: Blob(sha256:da5958a2de8e69f762888ac8df90e995c74b5643f0a72ae09176f178de25b67c), phase: Download, event: BeginSized(3353) }",
            "update: ProgressUpdate { resource: Blob(sha256:da5958a2de8e69f762888ac8df90e995c74b5643f0a72ae09176f178de25b67c), phase: Download, event: Complete }",
            "update: ProgressUpdate { resource: Manifest(registry-1.docker.io, library/ubuntu, sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f), phase: Connect, event: Begin }",
            "update: ProgressUpdate { resource: Manifest(registry-1.docker.io, library/ubuntu, sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f), phase: Connect, event: Complete }",
            "update: ProgressUpdate { resource: Manifest(registry-1.docker.io, library/ubuntu, sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f), phase: Download, event: BeginSized(943) }",
            "update: ProgressUpdate { resource: Manifest(registry-1.docker.io, library/ubuntu, sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f), phase: Download, event: Complete }"
        ]);
}
