// This code may not be used for any purpose. Be gay, do crime.

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

mod seccomp;
mod tracer;

use pentacle::ensure_sealed;
use std::process::Command;
use std::os::unix::process::CommandExt;

pub mod modes {
    pub const STAGE_1_TRACER: &'static str = "sand";
    pub const STAGE_2_LOADER: &'static str = "sand-exec";
}

fn main() {
    ensure_sealed().unwrap();
    let mode = std::env::args().next();

    if mode == Some(modes::STAGE_1_TRACER.to_string()) {
        seccomp::activate();
        tracer::run();

    } else if mode == Some(modes::STAGE_2_LOADER.to_string()) {
        // Start the sandboxed binary here once we have a sandbox. For now it's a shell to inspect the world.
        Command::new("/bin/sh").exec();

    } else {
        // Started under unknown conditions... this shouldn't happen when we're in the runtime,
        // but this is where we end up when running the binary manually for testing.
        // Restart as the stage 1 tracer.
        println!("hi.");
        Command::new("/proc/self/exe").arg0(modes::STAGE_1_TRACER);
    }
}
