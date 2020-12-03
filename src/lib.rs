//! 🅱️🧦 bandsocks
//! ================
//!
//! it's a lightweight sandbox for Linux, written in pure Rust!
//!
//! it runs docker images!
//!
//! it can run nested inside unprivileged containers!
//!
//! it's a library with minimal external dependencies!
//!
//! it's highly experimental!
//!
//! ```text
//! 🎶 🎹🧦 🎸🧦 🎸🧦 🎷🧦 🎺🧦 🥁🧦 🎶
//! ```
//!
//! Scope
//! =====
//!
//! Let's make it easy to run somewhat-untrusted computational workloads
//! like media codecs from inside an existing async rust network app. There is
//! no networking support. The container uses a virtual filesystem backed by
//! read-only image contents and mounted I/O channels.
//!
//! Getting Started
//! ===============
//!
//! The easiest way to start is via [Container::pull], which sets up a registry
//! client with default options, downloads an image to cache as necessary, and
//! provides a [ContainerBuilder] for further customization.
//!
//! ```
//! #[tokio::main]
//! async fn main() {
//!   let s = "busybox@sha256:cddb0e8f24f292e9b7baaba4d5f546db08f0a4b900be2048c6bd704bd90c13df";
//!   bandsocks::Container::pull(&s.parse().unwrap())
//!     .await.unwrap()
//!     .arg("busybox").arg("--help")
//!     .interact().await.unwrap();
//! }
//! ```
//!
//! Architecture
//! ============
//!
//! The isolation in bandsocks comes primarily from a seccomp system call
//! filter. A small set of system calls are allowed to pass through to the host
//! kernel, while many system calls are disallowed entirely and others are
//! emulated by our runtime.
//!
//! There is a 1:1 relationship between virtualized processes and host
//! processes, but virtual processes have their own ID namespace. File
//! descriptors are also mapped 1:1, so that read() and write() syscalls can be
//! a pass-through after files are opened via a slower emulated open() call.
//!
//! The emulated paths look a bit like User Mode Linux or gvisor. A ptrace-based
//! runtime we just call `sand` is responsible for emulating operations like
//! open() and exec() using only allowed syscalls. For filesystem access, `sand`
//! uses an inter-process communication channel to request live file descriptors
//! from the virtual filesystem.
//!
//! The `sand` runtime is also written in Rust, but it uses a lower-level style
//! that bypasses the Rust and C standard libraries in order to have tight
//! control over its system call usage. It is a single process per container,
//! using asynchronous rust to respond to events from all processes/threads
//! within that particular container.

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde;
#[macro_use] extern crate hash32_derive;

mod container;
mod errors;
mod filesystem;
mod image;
mod ipcserver;
mod manifest;
mod process;
mod registry;
mod sand;
mod taskcall;

pub use crate::{
    container::*,
    errors::*,
    filesystem::{mount::*, socket::*},
    image::*,
    registry::*,
};
