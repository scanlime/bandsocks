#![no_std]
#![no_main]
#![feature(panic_info_message)]

use bandsocks_sand::{c_main, nolibc, print, println};
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(message) = info.message() {
        nolibc::write_stderr(*message);
    }
    print!("\ncontainer panic!");
    if let Some(location) = info.location() {
        print!(" at {}:{}", location.file(), location.line());
    }
    println!();
    nolibc::exit(nolibc::EXIT_PANIC)
}

#[cfg(not(test))]
#[no_mangle]
fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {
    c_main(argc, argv)
}

#[cfg(not(test))]
#[no_mangle]
fn __libc_csu_init() {
    unreachable!()
}

#[cfg(not(test))]
#[no_mangle]
fn __libc_csu_fini() {
    unreachable!()
}

#[no_mangle]
fn main() -> usize {
    #[cfg(not(test))]
    unreachable!();
    #[cfg(test)]
    nolibc::EXIT_SUCCESS
}
