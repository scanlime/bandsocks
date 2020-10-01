// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

// These are never called, but the startup code takes their address
#[no_mangle] fn __libc_csu_init() {}
#[no_mangle] fn __libc_csu_fini() {}
#[no_mangle] fn main() {}

use sc::syscall;
use core::slice;
use core::str::{self, Utf8Error};
use core::convert::TryInto;
use core::panic::PanicInfo;
use core::fmt::{self, Write};

fn exit(code: usize) -> ! {
    unsafe { syscall!(EXIT, code) };
    unreachable!()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut stderr = SysFd(2);
    if let Some(args) = info.message() {
        drop(fmt::write(&mut stderr, *args));
    }
    drop(write!(&mut stderr, "\npanic!\n"));
    exit(128)
}

struct SysFd(usize);

impl fmt::Write for SysFd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() == unsafe { syscall!(WRITE, self.0, s.as_ptr() as usize, s.len()) } {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

unsafe fn c_strlen(mut s: *const u8) -> usize {
    let mut result = 0;
    while 0 != *s {
        result += 1;
        s = s.offset(1);
    }
    result
}

unsafe fn from_c_str(s: *const u8) -> Result<&'static str, Utf8Error> {
    str::from_utf8(slice::from_raw_parts(s, 1 + c_strlen(s)))
}

#[no_mangle]
fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {
    let argv = unsafe { slice::from_raw_parts(argv, argc.try_into().unwrap()) };
    let argv0 = unsafe { from_c_str(*argv.first().unwrap()).unwrap() };
    
    write!(&mut SysFd(2), "Hello World from {}\n", argv0).unwrap();

    exit(0);
}
