// This code may not be used for any purpose. Be gay, do crime.

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

mod seccomp;
mod tracer;

use std::ffi::CString;
use libc::c_char;
use std::ptr::null;

pub mod modes {
    pub const STAGE_1_TRACER: &'static str = "sand";
    pub const STAGE_2_LOADER: &'static str = "sand-exec";
}
    
fn main() {
    pentacle::ensure_sealed().unwrap();
    seccomp::activate();
    match std::env::args().next() {
        Some(mode) if mode == modes::STAGE_1_TRACER => tracer::run(),
        Some(mode) if mode == modes::STAGE_2_LOADER => exec_inner(),
        _ => interactive_startup(),
    }
}

pub fn exec_self(mode: &'static str) {
    let mode = CString::new(mode).unwrap();
    let self_exe = CString::new("/proc/self/exe").unwrap();

    let argv: Vec<*const c_char> = vec![ mode.as_ptr(), null() ];
    let envp: Vec<*const c_char> = vec![ null() ];
    
    let result = unsafe { libc::execve(self_exe.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
    panic!("sand: exec_self fault ({})", result);
}

fn exec_inner() {
    let exe = CString::new("/bin/sh").unwrap();

    let argv: Vec<*const c_char> = vec![ exe.as_ptr(), null() ];
    let envp: Vec<*const c_char> = vec![ null() ];
    
    let result = unsafe { libc::execve(exe.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
    panic!("sand: exec_inner fault ({})", result);
}

fn interactive_startup() {
    // Started under unknown conditions... this shouldn't happen when we're in the
    // runtime, but this is where we end up when running the binary manually for testing.
    // Restart as the stage 1 tracer.
    
    println!("hi.");
    exec_self(modes::STAGE_1_TRACER);
}
