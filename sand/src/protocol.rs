// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

pub use ssmarshal::{serialize, deserialize};

pub const BUFFER_SIZE: usize = 128;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct VPid(pub u32);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub struct MessageToSand {
    task: VPid,
    op: ToSand,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub struct MessageFromSand {
    task: VPid,
    op: FromSand,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub struct AssociatedFd(bool);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub enum ToSand {
    OpenReply(i32, AssociatedFd)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub enum FromSand {
    SysOpen(usize, usize, usize)
}
