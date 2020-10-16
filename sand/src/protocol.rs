// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

use serde::{
    de::{Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};
use core::fmt;
pub use ssmarshal::{deserialize, serialize};

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
pub struct VPtr(pub usize);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct VString(pub VPtr);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Errno(pub i32);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SysFd(pub u32);

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
    OpenProcessReply,
    SysOpenReply(Result<SysFd, Errno>),
    SysAccessReply(Result<(), Errno>),
    SysKillReply(Result<(), Errno>),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[repr(C)]
pub struct SysAccess {
    pub dir: Option<SysFd>,
    pub path: VString,
    pub mode: i32,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[repr(C)]
pub enum FromSand {
    OpenProcess(SysPid),
    SysAccess(SysAccess),
    SysOpen(SysAccess, i32),
    SysKill(VPid, Signal),
}

impl Serialize for SysFd {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_unit_struct("SysFd")
    }
}

impl<'d> Deserialize<'d> for SysFd {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'d>,
    {
        struct SysFdVisitor;

        impl<'d> Visitor<'d> for SysFdVisitor {
            type Value = SysFd;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a `SysFd`")
            }

            fn visit_unit<E>(self) -> Result<SysFd, E> {
                Ok(SysFd(42))
            }
        }

        deserializer.deserialize_unit_struct("SysFd", SysFdVisitor)
    }
}
