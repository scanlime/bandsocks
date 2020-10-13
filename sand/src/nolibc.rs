use sc::syscall;
use core::slice;
use core::str;
use core::ptr::null;
use core::panic::PanicInfo;
use core::fmt::{self, Write};
use crate::abi;

#[derive(Debug, Clone)]
pub struct SysFd(pub u32);

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

pub unsafe fn c_strlen(s: *const u8) -> usize {
    let mut count: usize = 0;
    while *s.offset(count as isize) != 0 {
        count += 1;
    }
    count
}

pub fn c_unwrap_nul(s: &[u8]) -> &[u8] {
    assert_eq!(s.last(), Some(&0u8));
    &s[0..s.len() - 1]
}

pub unsafe fn c_str_slice(s: *const u8) -> &'static [u8] {
    slice::from_raw_parts(s, 1 + c_strlen(s))
}

pub unsafe fn c_strv_len(strv: *const *const u8) -> usize {
    let mut count: usize = 0;
    while *strv.offset(count as isize) != null() {
        count += 1;
    }
    count
}

pub unsafe fn c_strv_slice(strv: *const *const u8) -> &'static [*const u8] {
    slice::from_raw_parts(strv, c_strv_len(strv))
}

#[naked]
unsafe extern fn sigreturn() {
    syscall!(RT_SIGRETURN);
    unreachable!();
}

pub fn signal(signum: u32, handler: extern fn(u32)) -> Result<(), isize> {
    let sigaction = abi::SigAction {
        sa_flags: abi::SA_RESTORER,
        sa_handler: handler,
        sa_restorer: sigreturn,
        sa_mask: [0; 16],
    };
    match unsafe { syscall!(RT_SIGACTION,
                            signum, (&sigaction) as *const abi::SigAction,
                            0, core::mem::size_of::<abi::SigSet>()) } {
        0 => Ok(()),
        other => Err(other as isize),
    }
}

pub fn fcntl(fd: &SysFd, op: usize, arg: usize) -> Result<(), isize> {
    match unsafe { syscall!(FCNTL, fd.0, op, arg) } {
        0 => Ok(()),
        other => Err(other as isize),
    }
}

#[no_mangle]
fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {

    // At this point, the argument and environment are in back-to-back
    // null terminated arrays of null terminated strings.

    let argv_slice = unsafe { c_strv_slice(argv) };
    assert_eq!(argc as usize, argv_slice.len());
    let envp_slice = unsafe { c_strv_slice(argv.offset(argv_slice.len() as isize + 1)) };

    crate::main(argv_slice, envp_slice);

    // Must explicitly invoke exit or we are just smashing the stack
    exit(0);
}

// These are never called, but the startup code takes their address
#[no_mangle] fn __libc_csu_init() {}
#[no_mangle] fn __libc_csu_fini() {}
#[no_mangle] fn main() {}
