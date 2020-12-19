#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

#[cfg(not(test))]
mod main_no_std {
    use bandsocks_sand::{
        c_main, c_strv_slice, exit, print, println, write_stderr, PageAllocator, EXIT_OUT_OF_MEM,
        EXIT_PANIC,
    };
    use core::panic::PanicInfo;

    #[global_allocator]
    static GLOBAL_ALLOC: PageAllocator = PageAllocator;

    fn container_panic() {
        print!("\ncontainer panic!");
    }

    #[alloc_error_handler]
    fn out_of_memory(layout: core::alloc::Layout) -> ! {
        container_panic();
        println!(" out of memory allocating {:?}", layout);
        exit(EXIT_OUT_OF_MEM);
    }

    #[panic_handler]
    fn panic(info: &PanicInfo) -> ! {
        if let Some(message) = info.message() {
            write_stderr(*message);
        }
        container_panic();
        if let Some(location) = info.location() {
            print!(" at {}:{}", location.file(), location.line());
        }
        println!();
        exit(EXIT_PANIC);
    }

    #[no_mangle]
    unsafe fn __libc_start_main(_: usize, argc: isize, argv: *const *const u8) -> isize {
        let argv_slice = c_strv_slice(argv);
        assert_eq!(argc as usize, argv_slice.len());
        let envp_slice = c_strv_slice(argv.offset(argv_slice.len() as isize + 1));
        let result = c_main(argv_slice, envp_slice);
        exit(result)
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
}

#[no_mangle]
fn main() -> usize {
    #[cfg(not(test))]
    unreachable!();
    #[cfg(test)]
    bandsocks_sand::EXIT_OK
}
