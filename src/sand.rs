// The 'sand' module is not part of this crate's workspace,
// since it needs different build options and we need it to be
// fully built in order to include this data. See the build.rs

pub const PROGRAM_DATA: &'static [u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/sand-target/release/bandsocks-sand"
));

pub mod protocol {
    include!("../sand/src/protocol.rs");
}

pub mod protocol_std {
    use super::protocol::SysFd;
    use std::os::{
        raw::c_int,
        unix::{io::AsRawFd, prelude::RawFd},
    };

    impl AsRawFd for SysFd {
        fn as_raw_fd(&self) -> RawFd {
            self.0 as c_int
        }
    }
}

pub mod log {
    use super::protocol::{LogLevel, LogMessage, VPid};

    pub fn max_log_level() -> LogLevel {
        match log::max_level() {
            log::LevelFilter::Off => LogLevel::Off,
            log::LevelFilter::Error => LogLevel::Error,
            log::LevelFilter::Warn => LogLevel::Warn,
            log::LevelFilter::Info => LogLevel::Info,
            log::LevelFilter::Debug => LogLevel::Debug,
            log::LevelFilter::Trace => LogLevel::Trace,
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
        log::log!(level, "{:?} {:x?}", task, message);
    }
}
