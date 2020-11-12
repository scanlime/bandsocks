#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(message) = info.message() {
        bandsocks_sand::nolibc::write_stderr(*message);
    }
    bandsocks_sand::print!("\ncontainer panic!");
    if let Some(location) = info.location() {
        bandsocks_sand::print!(" at {}:{}", location.file(), location.line());
    }
   bandsocks_sand::println!();
bandsocks_sand::nolibc::exit(bandsocks_sand::nolibc::EXIT_SUCCESS);
}

#[cfg(not(test))]
#[no_mangle]
fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {
    bandsocks_sand::c_main(argc, argv)
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
    bandsocks_sand::nolibc::EXIT_SUCCESS
}
