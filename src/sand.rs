pub use bandsocks_protocol as protocol;

#[cfg(not(doc))]
const PROGRAM_DATA: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/sand/target/release/bandsocks-sand"
));

#[cfg(doc)]
const PROGRAM_DATA: &[u8] = b"";

use crate::errors::RuntimeError;
use protocol::{LogLevel, LogMessage, VPid};
use std::{
    fs::File,
    io::Write,
    os::unix::{
        io::{AsRawFd, RawFd},
        process::CommandExt,
    },
    process::{Command, Stdio},
};

lazy_static! {
    static ref PROGRAM_FILE: Result<File, RuntimeError> = create_program_file();
}

fn create_program_file() -> Result<File, RuntimeError> {
    let memfd = memfd::MemfdOptions::default()
        .allow_sealing(true)
        .create("bandsocks-sand")?;
    memfd.as_file().write_all(PROGRAM_DATA)?;
    memfd.add_seals(
        &[
            memfd::FileSeal::SealWrite,
            memfd::FileSeal::SealShrink,
            memfd::FileSeal::SealGrow,
            memfd::FileSeal::SealSeal,
        ]
        .iter()
        .cloned()
        .collect(),
    )?;
    Ok(memfd.into_file())
}

pub fn command(fd: RawFd) -> Result<Command, RuntimeError> {
    let file = match &*PROGRAM_FILE {
        Err(err) => return Err(RuntimeError::ProgramAllocError(err.to_string())),
        Ok(file) => file,
    };
    let mut cmd = Command::new(format!("/proc/self/fd/{}", file.as_raw_fd()));
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.arg0("sand");
    cmd.env_clear();
    cmd.env("FD", fd.to_string());
    Ok(cmd)
}

pub fn max_log_level() -> LogLevel {
    if log::log_enabled!(log::Level::Trace) {
        LogLevel::Trace
    } else if log::log_enabled!(log::Level::Debug) {
        LogLevel::Debug
    } else if log::log_enabled!(log::Level::Info) {
        LogLevel::Info
    } else if log::log_enabled!(log::Level::Warn) {
        LogLevel::Warn
    } else if log::log_enabled!(log::Level::Error) {
        LogLevel::Error
    } else {
        LogLevel::Off
    }
}

pub fn task_log(task: VPid, level: LogLevel, message: LogMessage) {
    let level = match level {
        LogLevel::Off => return,
        LogLevel::Error => log::Level::Error,
        LogLevel::Warn => log::Level::Warn,
        LogLevel::Info => log::Level::Info,
        LogLevel::Debug => log::Level::Debug,
        LogLevel::Trace => log::Level::Trace,
    };
    log::log!(level, "{:?} {:?}", task, message);
}
