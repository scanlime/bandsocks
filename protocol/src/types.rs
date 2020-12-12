use core::fmt;
use core::default::Default;

/// Exit codes returned by the sand process
pub mod exit {
    pub const EXIT_OK: usize = 0;
    pub const EXIT_PANIC: usize = 60;
    pub const EXIT_DISCONNECTED: usize = 61;
    pub const EXIT_IO_ERROR: usize = 62;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Deserialize, Serialize)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct FileStat {
    // to do
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProcessHandle {
    pub mem: SysFd,
    pub maps: SysFd,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Hash32)]
#[repr(C)]
pub struct SysFd(pub u32);

impl Default for SysFd {
    fn default() -> Self {
        SysFd(!0u32)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Hash32, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Signal(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Errno(pub i32);

#[derive(
    Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Hash, Hash32, Serialize, Deserialize,
)]
#[repr(C)]
pub struct VPid(pub u32);

#[derive(PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct VPtr(pub usize);

impl fmt::Debug for VPtr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VPtr({:x?})", self.0)
    }
}

impl VPtr {
    pub fn null() -> VPtr {
        VPtr(0)
    }

    pub fn add(&self, count: usize) -> VPtr {
        VPtr(self.0 + count)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct VString(pub VPtr);

