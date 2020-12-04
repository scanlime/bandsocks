🅱️🧦 bandsocks
================

it's a sandbox!

it's a container runtime!

it's designed to nest inside unprivileged docker containers!

it's highly experimental and doesn't actually work yet!

```
🎶 🎹🧦 🎸🧦 🎸🧦 🎷🧦 🎺🧦 🥁🧦 🎶
```

[![Docs][docs-badge]][docs-url]
[![Crates.io][crates-badge]][crates-url]
[![Build Status][actions-badge]][actions-url]

[docs-badge]: https://docs.rs/bandsocks/badge.svg
[docs-url]: https://docs.rs/bandsocks
[crates-badge]: https://img.shields.io/crates/v/bandsocks.svg
[crates-url]: https://crates.io/crates/bandsocks
[actions-badge]: https://github.com/scanlime/bandsocks/workflows/Tests/badge.svg
[actions-url]: https://github.com/scanlime/bandsocks/actions?query=workflow%3ATests+branch%3Amaster

Takes inspiration from gaol, User Mode Linux, gvisor, chromium, and podman. The goal is to add an extra level of isolation to compute workloads we run as non-root within containers which are already somewhat locked down. This means that most high-powered kernel features like KVM and even user namespaces are off the table. The approach this project uses is based on seccomp to restrict system calls, and an emulated filesystem.

The intended API for this package is fairly high-level:

- download/unpack docker images into an emulated filesystem
- attach streams to emulated files for I/O
- run commands in the container

Non-goals include storage and networking. Networking will be fully disabled, and the virtual filesystem will be mostly in an immutable ramdisk made from images downloaded through a regsitry. Complete syscall support is also not a priority, as long as it can run computational workloads like media codecs.
