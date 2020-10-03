// This code may not be used for any purpose. Be gay, do crime.

#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(const_in_array_repeat_expressions)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch="x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

#[macro_use]
extern crate memoffset;

#[macro_use]
extern crate serde;

#[macro_use]
mod nolibc;

mod abi;
mod bpf;
mod emulator;
mod seccomp;
mod process;
mod protocol;
mod ptrace;
mod tracer;

use core::ptr;
use sc::syscall;
use crate::nolibc::SysFd;
use crate::tracer::Tracer;

const SELF_EXE: &'static [u8] = b"/proc/self/exe\0";
const STAGE_1_TRACER: &'static [u8] = b"sand\0";
const STAGE_2_LOADER: &'static [u8] = b"sand-exec\0";
enum RunMode { Unknown, Tracer(SysFd), Loader }

fn main(argv: &[*const u8], envp: &[*const u8]) {
    match check_environment_determine_mode(argv, envp) {
        RunMode::Unknown => panic!("where am i"),

        RunMode::Tracer(fd) => {
            seccomp::policy_for_tracer();

            say_hi_to_ipc_server(fd);
            
            let mut tracer = Tracer::new();
            let argv = [ STAGE_2_LOADER.as_ptr(), ptr::null() ];
            let envp: [*const u8; 1] = [ ptr::null() ];
            tracer.spawn(SELF_EXE, &argv, &envp);
            tracer.handle_events();
        }
        
        RunMode::Loader => {
            seccomp::policy_for_loader();

            println!("loader says hey, argc={}", argv.len());
            let argv = [ b"sh\0".as_ptr(), ptr::null() ];
            let envp: [*const u8; 1] = [ ptr::null() ];
            unsafe { syscall!(EXECVE, b"/bin/sh\0".as_ptr(), argv.as_ptr(), envp.as_ptr()) };
        }
    }
}

fn say_hi_to_ipc_server(socket: SysFd) {
    let flags = abi::MSG_DONTWAIT;
    let iov = abi::IOVec {
        base: b"hello world".as_ptr() as *mut usize,
        len: 5,
    };
    let msghdr = abi::MsgHdr {
        msg_name: ptr::null_mut(),
        msg_namelen: 0,
        msg_iter: abi::IOVIter {
            iter_type: abi::ITER_IOVEC,
            iov_offset: 0,
            count: 1,
            iov_ptr: &iov as *const abi::IOVec,
            nr_segs: 1,
        },
        msg_control: ptr::null_mut(),
        msg_control_is_user: false,
        msg_controllen: 0,
        msg_flags: 0,
        msg_iocb: ptr::null_mut()
    };
    let result = unsafe { syscall!(SENDMSG, socket.0, &msghdr as *const abi::MsgHdr, flags) as isize };
    if result != 0 {
        panic!("ipc sendmsg failed ({})", result);
    }
}
    
fn check_environment_determine_mode(argv: &[*const u8], envp: &[*const u8]) -> RunMode {
    // All modes require the sealed exe and an argv[0]
    let required_tests = check_sealed_exe_environment().is_ok();
    let argv0 = unsafe { nolibc::c_str_slice(*argv.first().unwrap()) };

    if required_tests && argv0 == STAGE_1_TRACER && argv.len() == 1 && envp.len() == 1 {
        // Stage 1: no other args, a single 'FD' environment variable
        match parse_envp_as_fd(envp) {
            Some(fd) => RunMode::Tracer(fd),
            None => RunMode::Unknown,
        }
        
    } else if required_tests && argv0 == STAGE_2_LOADER && argv.len() == 1 && envp.len() == 0 {
        // Stage 2: no other args, empty environment
        RunMode::Loader

    } else {
        RunMode::Unknown
    }
}

fn parse_envp_as_fd(envp: &[*const u8]) -> Option<SysFd> {
    let envp0 = unsafe { nolibc::c_str_slice(*envp.first().unwrap()) };
    let envp0 = core::str::from_utf8(nolibc::c_unwrap_nul(envp0)).unwrap();
    let mut parts = envp0.splitn(2, "=");
    match (parts.next(), parts.next()) {
        (Some("FD"), Some(right)) => match right.parse::<u32>() {
            Ok(fd) if fd > 2 => Some(SysFd(fd)),
            _ => None,
        },
        _ => None
    }
}

fn check_sealed_exe_environment() -> Result<(), ()> {
    // This is probably not super important, but as part of checking out the runtime environment
    // during startup, it's easy to make sure this seems to be the sealed binary that we expected
    // the runtime to create for us. This is invoked unconditionally; in stage 1 it will run
    // normally, *before* the seccomp filter, so these will all be real syscalls. In stage 2
    // these syscalls will be emulated by the tracer.

    let exe_fd = unsafe { syscall!(OPEN, SELF_EXE.as_ptr(), abi::O_RDONLY, 0) as isize };
    if exe_fd > 0 {
        let seals = unsafe { syscall!(FCNTL, exe_fd, abi::F_GET_SEALS) };
        unsafe { syscall!(CLOSE, exe_fd) };

        let expected = abi::F_SEAL_SEAL | abi::F_SEAL_SHRINK | abi::F_SEAL_GROW | abi::F_SEAL_WRITE;
        if seals == expected {
            Ok(())
        } else {
            Err(())
        }
    } else {
        Err(())
    }
}
