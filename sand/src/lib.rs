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
mod nolibc;

mod abi;
mod binformat;
mod init;
mod ipc;
mod parser;
mod process;
mod protocol;
mod ptrace;
mod remote;
mod seccomp;
mod tracer;

pub use nolibc::{
    c_str_slice, c_strv_slice, c_unwrap_nul, exit, write_stderr, EXIT_IO_ERROR, EXIT_PANIC,
    EXIT_SUCCESS,
};

use crate::{
    ipc::Socket,
    protocol::{Errno, SysFd},
    tracer::Tracer,
};
use core::str;

pub const STAGE_1_TRACER: &[u8] = b"sand\0";
pub const STAGE_2_INIT_LOADER: &[u8] = b"sand-exec\0";

enum RunMode {
    Unknown,
    Tracer(SysFd),
    InitLoader(SysFd),
}

pub unsafe fn c_main(argv: &[*const u8], envp: &[*const u8]) -> usize {
    match check_environment_determine_mode(argv, envp) {
        RunMode::Unknown => panic!("where am i"),

        RunMode::Tracer(fd) => {
            close_all_except(&[&nolibc::stderr(), &fd]);
            seccomp::policy_for_tracer();
            Tracer::new(Socket::from_sys_fd(&fd), process::task::task_fn).run();
        }

        RunMode::InitLoader(fd) => {
            seccomp::policy_for_loader();
            init::with_args_from_fd(&fd);
        }
    }
    EXIT_SUCCESS
}

unsafe fn check_environment_determine_mode(argv: &[*const u8], envp: &[*const u8]) -> RunMode {
    let argv0 = c_str_slice(*argv.first().unwrap());
    if argv0 == STAGE_1_TRACER
        && argv.len() == 1
        && envp.len() == 1
        && check_sealed_exe() == Ok(true)
    {
        match parse_envp_as_fd(envp) {
            Some(fd) => RunMode::Tracer(fd),
            None => RunMode::Unknown,
        }
    } else if argv0 == STAGE_2_INIT_LOADER && argv.len() == 1 && envp.len() == 1 {
        match parse_envp_as_fd(envp) {
            Some(fd) => RunMode::InitLoader(fd),
            None => RunMode::Unknown,
        }
    } else {
        RunMode::Unknown
    }
}

unsafe fn parse_envp_as_fd(envp: &[*const u8]) -> Option<SysFd> {
    let envp0 = c_str_slice(*envp.first().unwrap());
    let envp0 = str::from_utf8(c_unwrap_nul(envp0)).unwrap();
    let mut parts = envp0.splitn(2, '=');
    match (parts.next(), parts.next().map(|val| val.parse::<u32>())) {
        (Some("FD"), Some(Ok(fd))) => Some(SysFd(fd)),
        _ => None,
    }
}

fn close_all_except(fd_allowed: &[&SysFd]) {
    let dir_fd = nolibc::open_self_fd().expect("opening proc self fd");
    let mut fd_count = 0;

    // the directory fd is implicitly included in the allowed list; it's closed
    // last.
    let fd_count_expected = 1 + fd_allowed.len();
    let fd_test = |fd: &SysFd| *fd == dir_fd || fd_allowed.contains(&fd);

    for result in nolibc::DirIterator::<typenum::U512, _, _>::new(&dir_fd, |dirent| {
        assert!(dirent.d_type == abi::DT_DIR || dirent.d_type == abi::DT_LNK);
        if dirent.d_type == abi::DT_LNK {
            Some(SysFd(
                str::from_utf8(dirent.d_name)
                    .expect("proc fd utf8")
                    .parse()
                    .expect("proc fd number"),
            ))
        } else {
            assert_eq!(dirent.d_type, abi::DT_DIR);
            assert!(dirent.d_name == b".." || dirent.d_name == b".");
            None
        }
    }) {
        let result: Option<SysFd> = result.expect("reading proc fd");
        if let Some(fd) = result {
            fd_count += 1;
            if !fd_test(&fd) {
                fd.close().expect("closing fd leak");
            }
        }
    }

    dir_fd.close().expect("proc self fd leak");
    assert!(fd_count >= fd_count_expected);
}

fn check_sealed_exe() -> Result<bool, Errno> {
    let exe = nolibc::open_self_exe()?;
    let seals = nolibc::fcntl(&exe, abi::F_GET_SEALS, 0);
    exe.close().expect("exe fd leak");
    let expected = abi::F_SEAL_SEAL | abi::F_SEAL_SHRINK | abi::F_SEAL_GROW | abi::F_SEAL_WRITE;
    Ok(seals? as usize == expected)
}
