// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

mod nolibc;

mod modes {
    pub const STAGE_1_TRACER: &'static [u8] = b"sand\0";
    pub const STAGE_2_LOADER: &'static [u8] = b"sand-exec\0";
}

fn main(argv: &[*const u8]) -> Result<(), usize> {

    let argv0 = unsafe { nolibc::c_str_as_bytes(*argv.first().unwrap()) };

    if argv0 == modes::STAGE_1_TRACER {
        println!("hello from the tracer");
    } else if argv0 == modes::STAGE_2_LOADER {
        println!("loader says hey");
    } else {
        panic!("unexpected parameters");        
    }

    Ok(())
}
