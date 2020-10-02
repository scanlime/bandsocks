// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

mod nolibc;
mod seccomp;
mod tracer;

pub const SELF_EXE: &'static [u8] = b"/proc/self/exe\0";
pub const ARGV_MAX: usize = 16;

mod modes {
    pub const STAGE_1_TRACER: &'static [u8] = b"sand\0";
    pub const STAGE_2_LOADER: &'static [u8] = b"sand-exec\0";
}

use core::ptr::null;
use sc::syscall;

fn ensure_sealed() {
    let exe_fd = unsafe { syscall!(OPEN, SELF_EXE.as_ptr(), nolibc::O_RDONLY, 0) as isize };
    if exe_fd < 0 {
        panic!("can't open self");
    }
    
    let seals = unsafe { syscall!(FCNTL, exe_fd, nolibc::F_GET_SEALS) };
    unsafe { syscall!(CLOSE, exe_fd) };

    let expected = nolibc::F_SEAL_SEAL | nolibc::F_SEAL_SHRINK | nolibc::F_SEAL_GROW | nolibc::F_SEAL_WRITE;
    if seals != expected {
        panic!("exe was not sealed as expected");
    }
}    

fn exec_self(mode: &'static [u8]) -> ! {
    let argv = [ mode.as_ptr(), null() ];
    let envp: [ *const u8; 1 ] = [ null() ];
    unsafe { syscall!(EXECVE, SELF_EXE.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
    panic!("exec failed");
}

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

fn empty_envp() -> [*const u8; 1] {
    [ null() ]
}

fn make_next_stage_argv(mode: &'static [u8], src: &[*const u8]) -> [*const u8; ARGV_MAX] {
    let mut dest = [ null(); ARGV_MAX ];
    const null_terminator: usize = 1;
    assert!(src.len() + null_terminator <= dest.len());
    for i in 1..src.len() {
        dest[i] = src[i];
    }
    dest[0] = mode.as_ptr();
    dest
}   

fn tracer_main(argv: &[*const u8]) {
    println!("hello from the tracer, argc={}", argv.len());

    let argv = make_next_stage_argv(modes::STAGE_2_LOADER, argv);
    let envp = empty_envp();
    
    unsafe { syscall!(EXECVE, SELF_EXE.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
}

fn loader_main(argv: &[*const u8]) {
    println!("loader says hey, argc={}", argv.len());
    let argv = [ b"sh\0".as_ptr(), null() ];
    let envp = empty_envp();
    unsafe { syscall!(EXECVE, b"/bin/sh\0".as_ptr(), argv.as_ptr(), envp.as_ptr()) };
}
