use crate::{abi, types::*};

/// Any message sent from the IPC server to the sand process
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum MessageToSand {
    Task {
        task: VPid,
        op: ToTask,
    },
    Init {
        args: SysFd,
        tracer_settings: TracerSettings,
    },
}

/// Any message sent from the sand process to the IPC server
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum MessageFromSand {
    Task { task: VPid, op: FromTask },
}

/// Fixed size header for the variable sized initial args data
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct InitArgsHeader {
    pub dir_len: usize,
    pub filename_len: usize,
    pub argv_len: usize,
    pub arg_count: usize,
    pub envp_len: usize,
    pub env_count: usize,
}

impl InitArgsHeader {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const InitArgsHeader as *const u8,
                core::mem::size_of_val(self),
            )
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut InitArgsHeader as *mut u8,
                core::mem::size_of_val(self),
            )
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum LogMessage {
    Emulated(abi::Syscall),
    Remote(abi::Syscall),
    Signal(u8, abi::UserRegs),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct TracerSettings {
    pub max_log_level: LogLevel,
    pub instruction_trace: bool,
}

/// A message delivered to one of the lightweight tasks in the tracer
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum ToTask {
    OpenProcessReply(ProcessHandle),
    FileReply(Result<(VFile, SysFd), Errno>),
    FileStatReply(Result<(VFile, FileStat), Errno>),
    SizeReply(Result<usize, Errno>),
    Reply(Result<(), Errno>),
}

/// A message originating from one lightweight task in the tracer
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum FromTask {
    OpenProcess(SysPid),
    FileAccess {
        dir: Option<VFile>,
        path: VString,
        mode: i32,
    },
    FileOpen {
        dir: Option<VFile>,
        path: VString,
        flags: i32,
        mode: i32,
    },
    FileStat {
        file: Option<VFile>,
        path: Option<VString>,
        follow_links: FollowLinks,
    },
    ReadLink(VString, VStringBuffer),
    ProcessKill(VPid, Signal),
    ChangeWorkingDir(VString),
    GetWorkingDir(VStringBuffer),
    Exited(i32),
    Log(LogLevel, LogMessage),
}
