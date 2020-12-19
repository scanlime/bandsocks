#![no_std]
#![feature(asm)]
#![feature(naked_functions)]
#![feature(negative_impls)]
#![feature(const_in_array_repeat_expressions)]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch = "x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

#[macro_use] extern crate memoffset;

#[cfg(test)]
#[macro_use]
extern crate std;

extern crate alloc;

#[macro_use]
mod nolibc;

mod abi;
mod binformat;
mod init;
mod ipc;
mod parser;
mod process;
mod ptrace;
mod remote;
mod seccomp;
mod tracer;

pub use bandsocks_protocol as protocol;
pub use nolibc::{c_str_slice, c_strv_slice, c_unwrap_nul, exit, write_stderr, PageAllocator};
pub use protocol::exit::*;

use crate::{
    ipc::Socket,
    nolibc::File,
    protocol::{Errno, SysFd},
    tracer::Tracer,
};
use core::str;

pub const STAGE_1_TRACER: &[u8] = b"sand\0";
pub const STAGE_2_INIT_LOADER: &[u8] = b"sand-exec\0";

enum RunMode {
    Unknown,
    Tracer(File),
    InitLoader(File),
}

pub unsafe fn c_main(argv: &[*const u8], envp: &[*const u8]) -> usize {
    match check_environment_determine_mode(argv, envp) {
        RunMode::Unknown => panic!("where am i"),

        RunMode::Tracer(socket_file) => {
            stdio_for_tracer(&socket_file);
            seccomp::policy_for_tracer();
            Tracer::new(Socket::new(socket_file), process::task::task_fn).run();
        }

        RunMode::InitLoader(args_file) => {
            seccomp::policy_for_loader();
            stdio_for_loader();
            init::with_args_file(&args_file);
        }
    }
    EXIT_OK
}

fn stdio_for_tracer(socket_file: &File) {
    // The tracer has its original stderr, the ipc socket, and nothing else.
    // We don't want stdin or stdout really, but it's useful to keep the descriptors
    // reserved, so keep copies of stderr there. The stderr stream is normally
    // unused, but we keep it around for panic!() and friends.
    //
    // requires access to the real /proc, so this must run before seccomp.
    File::dup2(&File::stderr(), &File::stdin()).expect("closing stdin");
    File::dup2(&File::stderr(), &File::stdout()).expect("closing stdout");
    close_all_except(&[
        &File::stdin(),
        &File::stdout(),
        &File::stderr(),
        socket_file,
    ]);
}

fn stdio_for_loader() {
    // Replace the loader's stdin, stdout, and stderr with objects from the virtual
    // filesystem. These are not real open() calls at this point, they're being
    // trapped.
    let v_stdin =
        unsafe { File::open(b"/proc/1/fd/0\0", abi::O_RDONLY, 0) }.expect("no init stdin");
    let v_stdout =
        unsafe { File::open(b"/proc/1/fd/1\0", abi::O_WRONLY, 0) }.expect("no init stdout");
    let v_stderr =
        unsafe { File::open(b"/proc/1/fd/2\0", abi::O_WRONLY, 0) }.expect("no init stderr");
    File::dup2(&v_stdin, &File::stdin()).unwrap();
    File::dup2(&v_stdout, &File::stdout()).unwrap();
    File::dup2(&v_stderr, &File::stderr()).unwrap();
    v_stdin.close().unwrap();
    v_stdout.close().unwrap();
    v_stderr.close().unwrap();
}

unsafe fn check_environment_determine_mode(argv: &[*const u8], envp: &[*const u8]) -> RunMode {
    let argv0 = c_str_slice(*argv.first().unwrap());
    if argv0 == STAGE_1_TRACER
        && argv.len() == 1
        && envp.len() == 1
        && check_sealed_exe() == Ok(true)
    {
        match parse_envp_to_file(envp) {
            Some(file) => RunMode::Tracer(file),
            None => RunMode::Unknown,
        }
    } else if argv0 == STAGE_2_INIT_LOADER && argv.len() == 1 && envp.len() == 1 {
        match parse_envp_to_file(envp) {
            Some(file) => RunMode::InitLoader(file),
            None => RunMode::Unknown,
        }
    } else {
        RunMode::Unknown
    }
}

unsafe fn parse_envp_to_file(envp: &[*const u8]) -> Option<File> {
    let envp0 = c_str_slice(*envp.first().unwrap());
    let envp0 = str::from_utf8(c_unwrap_nul(envp0)).unwrap();
    let mut parts = envp0.splitn(2, '=');
    match (parts.next(), parts.next().map(|val| val.parse::<u32>())) {
        (Some("FD"), Some(Ok(fd))) => Some(File::new(SysFd(fd))),
        _ => None,
    }
}

fn close_all_except(allowed: &[&File]) {
    let dir = File::open_self_fd().expect("opening proc self fd");
    let mut fcount = 0;

    // the directory fd is implicitly included in the allowed list; it's closed
    // last.
    let fcount_expected = 1 + allowed.len();
    let is_allowed = |f: &File| f == &dir || allowed.contains(&f);

    for result in nolibc::DirIterator::<typenum::U512, _, _>::new(&dir, |dirent| {
        assert!(dirent.d_type == abi::DT_DIR || dirent.d_type == abi::DT_LNK);
        if dirent.d_type == abi::DT_LNK {
            Some(File::new(SysFd(
                str::from_utf8(dirent.d_name)
                    .expect("proc fd utf8")
                    .parse()
                    .expect("proc fd number"),
            )))
        } else {
            assert_eq!(dirent.d_type, abi::DT_DIR);
            assert!(dirent.d_name == b".." || dirent.d_name == b".");
            None
        }
    }) {
        let result: Option<File> = result.expect("reading proc fd");
        if let Some(file) = result {
            if is_allowed(&file) {
                fcount += 1;
            } else {
                file.close().expect("closing fd leak");
            }
        }
    }

    dir.close().expect("proc self fd leak");
    assert!(fcount == fcount_expected);
}

fn check_sealed_exe() -> Result<bool, Errno> {
    let exe = File::open_self_exe()?;
    let seals = exe.fcntl(abi::F_GET_SEALS, 0);
    exe.close().expect("exe fd leak");
    let expected = abi::F_SEAL_SEAL | abi::F_SEAL_SHRINK | abi::F_SEAL_GROW | abi::F_SEAL_WRITE;
    Ok(seals? as usize == expected)
}
