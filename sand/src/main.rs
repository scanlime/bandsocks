#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(message) = info.message() {
        bandsocks_sand::write_stderr(*message);
    }
    bandsocks_sand::print!("\ncontainer panic!");
    if let Some(location) = info.location() {
        bandsocks_sand::print!(" at {}:{}", location.file(), location.line());
    }
    bandsocks_sand::println!();
    bandsocks_sand::exit(bandsocks_sand::EXIT_PANIC);
}

#[cfg(not(test))]
#[no_mangle]
unsafe fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {
    let argv_slice = bandsocks_sand::c_strv_slice(argv);
    assert_eq!(argc as usize, argv_slice.len());
    let envp_slice = bandsocks_sand::c_strv_slice(argv.offset(argv_slice.len() as isize + 1));
    let result = bandsocks_sand::c_main(argv_slice, envp_slice);
    bandsocks_sand::exit(result)
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
    bandsocks_sand::EXIT_SUCCESS
}
