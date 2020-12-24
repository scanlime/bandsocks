use core::{default::Default, fmt};

/// Exit codes returned by the sand process
pub mod exit {
    pub const EXIT_OK: usize = 0;
    pub const EXIT_PANIC: usize = 120;
    pub const EXIT_DISCONNECTED: usize = 121;
    pub const EXIT_IO_ERROR: usize = 122;
    pub const EXIT_OUT_OF_MEM: usize = 123;
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum FollowLinks {
    NoFollow,
    Follow,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct FileStat {
    pub st_dev: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_atime: u64,
    pub st_atime_nsec: u64,
    pub st_mtime: u64,
    pub st_mtime_nsec: u64,
    pub st_ctime: u64,
    pub st_ctime_nsec: u64,
}

pub type INodeNum = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct VFile {
    pub inode: INodeNum,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
#[repr(C)]
pub struct SysFd(pub u32);

impl Default for SysFd {
    fn default() -> Self {
        SysFd(!0u32)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProcessHandle {
    pub mem: SysFd,
    pub maps: SysFd,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize, Hash)]
#[repr(C)]
pub struct Signal(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize, Hash)]
#[repr(C)]
pub struct Errno(pub i32);

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
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
