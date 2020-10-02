// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

mod nolibc;

pub const SELF_EXE: &'static [u8] = b"/proc/self/exe\0";

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

fn main(argv: &[*const u8]) -> Result<(), usize> {
    let _exe_fd = ensure_sealed();
    let argv0 = unsafe { nolibc::c_str_as_bytes(*argv.first().unwrap()) };
        
    if argv0 == modes::STAGE_1_TRACER {
        println!("hello from the tracer");
        exec_self(modes::STAGE_2_LOADER);
        
    } else if argv0 == modes::STAGE_2_LOADER {
        println!("loader says hey");
        
        let argv = [ b"sh\0".as_ptr(), null() ];
        let envp: [ *const u8; 1 ] = [ null() ];
        unsafe { syscall!(EXECVE, b"/bin/sh\0".as_ptr(), argv.as_ptr(), envp.as_ptr()) };

    } else {
        panic!("unexpected parameters");        
    }

    Ok(())
}
