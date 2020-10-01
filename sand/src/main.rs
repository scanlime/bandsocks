// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

mod nolibc;

use core::str;
use core::fmt::Write;
use nolibc::{c_str_as_bytes, SysFd};

mod modes {
    pub const STAGE_1_TRACER: &'static str = "sand";
    pub const STAGE_2_LOADER: &'static str = "sand-exec";
}

fn main(argv: &[*const u8]) -> Result<(), usize> {
    
    let argv0 = unsafe { c_str_as_bytes(*argv.first().unwrap()) };
    
    write!(&mut SysFd(2), "Hello World from {}\n", str::from_utf8(argv0).unwrap()).unwrap();

    Ok(())
}
