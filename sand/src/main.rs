// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(const_in_array_repeat_expressions)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

#[macro_use]
extern crate memoffset;

#[macro_use]
mod nolibc;

mod abi;
mod bpf;
mod emulator;
mod seccomp;
mod process;
mod ptrace;
mod tracer;

pub const SELF_EXE: &'static [u8] = b"/proc/self/exe\0";
pub const ARGV_MAX: usize = 4;

mod modes {
    pub const STAGE_1_TRACER: &'static [u8] = b"sand\0";
    pub const STAGE_2_LOADER: &'static [u8] = b"sand-exec\0";
}

use core::ptr::null;
use sc::syscall;
use crate::tracer::Tracer;

fn main(argv: &[*const u8]) {
    ensure_sealed();
    seccomp::activate();

    let argv0 = unsafe { nolibc::c_str_as_bytes(*argv.first().unwrap()) };
    if argv0 == modes::STAGE_1_TRACER {
        tracer_main(argv);
    } else if argv0 == modes::STAGE_2_LOADER {
        loader_main(argv);
    } else {
        panic!("unexpected parameters");
    }
}

fn ensure_sealed() {
    let exe_fd = unsafe { syscall!(OPEN, SELF_EXE.as_ptr(), abi::O_RDONLY, 0) as isize };
    if exe_fd < 0 {
        panic!("can't open self");
    }
    
    let seals = unsafe { syscall!(FCNTL, exe_fd, abi::F_GET_SEALS) };
    unsafe { syscall!(CLOSE, exe_fd) };

    let expected = abi::F_SEAL_SEAL | abi::F_SEAL_SHRINK | abi::F_SEAL_GROW | abi::F_SEAL_WRITE;
    if seals != expected {
        panic!("exe was not sealed as expected");
    }
}    

fn empty_envp() -> [*const u8; 1] {
    [ null() ]
}

fn make_next_stage_argv(mode: &'static [u8], src: &[*const u8]) -> [*const u8; ARGV_MAX] {
    let mut dest = [ null(); ARGV_MAX ];
    dest[0] = mode.as_ptr();
    for i in 1..src.len() {
        dest[i] = src[i];
    }
    dest[src.len()] = null();
    dest
}   

fn tracer_main(argv: &[*const u8]) {
    let mut tracer = Tracer::new();
    tracer.spawn(SELF_EXE,
                 &make_next_stage_argv(modes::STAGE_2_LOADER, argv),
                 &empty_envp());
    tracer.handle_events();
}

fn loader_main(argv: &[*const u8]) {
    println!("loader says hey, argc={}", argv.len());
    let argv = [ b"sh\0".as_ptr(), null() ];
    let envp = empty_envp();
    unsafe { syscall!(EXECVE, b"/bin/sh\0".as_ptr(), argv.as_ptr(), envp.as_ptr()) };
}
