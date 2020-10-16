// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

pub use ssmarshal::{deserialize, serialize};

pub const BUFFER_SIZE: usize = 128;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Hash32, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Hash32, Serialize, Deserialize)]
#[repr(C)]
pub struct VPid(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Signal(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Errno(pub i32);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct File();

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[repr(C)]
pub struct MessageToSand {
    pub task: VPid,
    pub op: ToSand,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[repr(C)]
pub struct MessageFromSand {
    pub task: VPid,
    pub op: FromSand,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[repr(C)]
pub enum ToSand {
    OpenReply(Result<File, Errno>),
    KillReply(Result<(), Errno>),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[repr(C)]
pub enum FromSand {
    SysAccess(u64, u64),
    SysOpen(u64, u64, u64),
    SysKill(VPid, Signal),
}
