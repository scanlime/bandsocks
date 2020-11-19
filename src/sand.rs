pub const PROGRAM_DATA: &'static [u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/sand-target/release/bandsocks-sand"
));

pub mod protocol {
    include!("../sand/src/protocol.rs");
}

use protocol::{LogLevel, LogMessage, SysFd, VPid};
use std::os::{
    raw::c_int,
    unix::{io::AsRawFd, prelude::RawFd},
};

impl AsRawFd for SysFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0 as c_int
    }
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
