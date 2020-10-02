// This code may not be used for any purpose. Be gay, do crime.

use sc::syscall;
use core::slice;
use core::str;
use core::convert::TryInto;
use core::panic::PanicInfo;
use core::fmt::{self, Write};

pub struct SysFd(pub usize);

impl fmt::Write for SysFd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() == unsafe { syscall!(WRITE, self.0, s.as_ptr() as usize, s.len()) } {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

pub fn exit(code: usize) -> ! {
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

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        let mut stderr = $crate::nolibc::SysFd(2);
        drop(core::fmt::write(&mut stderr, core::format_args!( $($arg)* )));
    });
}

#[macro_export]
macro_rules! println {
    () => ({
        print!("\n");
    });
    ($($arg:tt)*) => ({
        print!( $($arg)* );
        println!();
    });
}

pub unsafe fn c_strlen(mut s: *const u8) -> usize {
    let mut result = 0;
    while 0 != *s {
        result += 1;
        s = s.offset(1);
    }
    result
}

pub unsafe fn c_str_as_bytes(s: *const u8) -> &'static [u8] {
    slice::from_raw_parts(s, 1 + c_strlen(s))
}

#[no_mangle]
fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {
    let argv = unsafe { slice::from_raw_parts(argv, argc.try_into().unwrap()) };
    crate::main(argv);
    exit(0);
}

// These are never called, but the startup code takes their address
#[no_mangle] fn __libc_csu_init() {}
#[no_mangle] fn __libc_csu_fini() {}
#[no_mangle] fn main() {}

// Random x86_64 kernel ABI constants we need
pub const O_RDONLY: usize = 0;
pub const F_GET_SEALS: usize = 1034;
pub const F_SEAL_SEAL: usize = 1;
pub const F_SEAL_SHRINK: usize = 2;
pub const F_SEAL_GROW: usize = 4;
pub const F_SEAL_WRITE: usize = 8;
