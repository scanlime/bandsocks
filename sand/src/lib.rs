#![no_std]
#![feature(naked_functions)]
#![feature(negative_impls)]
#![feature(const_in_array_repeat_expressions)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch = "x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

#[macro_use] extern crate memoffset;
#[macro_use] extern crate serde;
#[macro_use] extern crate hash32_derive;

#[cfg(test)]
#[macro_use]
extern crate std;

#[macro_use]
pub mod nolibc;

mod abi;
mod ipc;
mod parser;
mod process;
mod protocol;
mod ptrace;
mod seccomp;
mod tracer;

use crate::{
    ipc::Socket, nolibc::c_strv_slice, process::task::task_fn, protocol::SysFd,
    ptrace::RawExecArgs, tracer::Tracer,
};
use core::ptr::null;
use sc::syscall;

const SELF_EXE: &[u8] = b"/proc/self/exe\0";
const STAGE_1_TRACER: &[u8] = b"sand\0";
const STAGE_2_LOADER: &[u8] = b"sand-exec\0";

enum RunMode {
    Unknown,
    Tracer(SysFd),
    Loader,
}

pub fn c_main(argc: isize, argv: *const *const u8) -> isize {
    let argv_slice = unsafe { c_strv_slice(argv) };
    assert_eq!(argc as usize, argv_slice.len());
    let envp_slice = unsafe { c_strv_slice(argv.offset(argv_slice.len() as isize + 1)) };

    match check_environment_determine_mode(argv_slice, envp_slice) {
        RunMode::Unknown => panic!("where am i"),

        RunMode::Tracer(fd) => {
            seccomp::policy_for_tracer();
            Tracer::new(Socket::from_sys_fd(&fd), task_fn).run(unsafe {
                &RawExecArgs::new(SELF_EXE, &[STAGE_2_LOADER.as_ptr(), null()], &[null()])
            });
        }

        RunMode::Loader => {
            // Running inside the tracer; load a more restrictive seccomp policy, then
            // issue a special form of 'exec' that the tracer accepts only once. This
            // becomes the first in-container process via the emulated ELF loader.
            seccomp::policy_for_loader();
            unsafe { syscall!(EXECVE, 0, 0, 0) };
        }
    }

    nolibc::exit(nolibc::EXIT_SUCCESS)
}

fn check_environment_determine_mode(argv: &[*const u8], envp: &[*const u8]) -> RunMode {
    let argv0 = unsafe { nolibc::c_str_slice(*argv.first().unwrap()) };
    if argv0 == STAGE_1_TRACER
        && argv.len() == 1
        && envp.len() == 1
        && check_sealed_exe_environment().is_ok()
    {
        // Stage 1: no other args, a single 'FD' env var, sealed exe
        match parse_envp_as_fd(envp) {
            Some(fd) => RunMode::Tracer(fd),
            None => RunMode::Unknown,
        }
    } else if argv0 == STAGE_2_LOADER && argv.len() == 1 && envp.is_empty() {
        // Stage 2: no other args, empty environment
        RunMode::Loader
    } else {
        RunMode::Unknown
    }
}

fn parse_envp_as_fd(envp: &[*const u8]) -> Option<SysFd> {
    let envp0 = unsafe { nolibc::c_str_slice(*envp.first().unwrap()) };
    let envp0 = core::str::from_utf8(nolibc::c_unwrap_nul(envp0)).unwrap();
    let mut parts = envp0.splitn(2, '=');
    match (parts.next(), parts.next().map(|val| val.parse::<u32>())) {
        (Some("FD"), Some(Ok(fd))) if fd > 2 => Some(SysFd(fd)),
        _ => None,
    }
}

fn check_sealed_exe_environment() -> Result<(), ()> {
    // This is probably not super important, but as part of checking out the runtime
    // environment during startup it's easy to make sure this seems to be the
    // sealed binary that we expected the runtime to create for us.

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
