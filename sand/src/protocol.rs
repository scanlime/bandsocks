// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

use core::fmt;
use heapless::Vec;
use typenum::*;
use postcard::{Error, flavors::SerFlavor};
use serde::{
    de::{Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};

pub type BufferedBytesMax = U256;
pub type BufferedFilesMax = U16;

pub type BytesBuffer = Vec<u8, BufferedBytesMax>;
pub type FilesBuffer = Vec<SysFd, BufferedFilesMax>;

pub struct IPCBuffer {
    bytes: BytesBuffer,
    files: FilesBuffer,
    byte_offset: usize,
    file_offset: usize,
}

impl IPCBuffer {
    pub fn new() -> Self {
        IPCBuffer {
            bytes: Vec::new(),
            files: Vec::new(),
            byte_offset: 0,
            file_offset: 0
        }
    }

    pub fn reset(&mut self) {
        assert!(self.is_empty());
        self.bytes.clear();
        self.files.clear();
        self.byte_offset = 0;
        self.file_offset = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.byte_offset == self.bytes.len() && self.file_offset == self.files.len()
    }

    pub fn as_mut_parts(&mut self) -> (&mut BytesBuffer, &mut FilesBuffer) {
        assert_eq!(self.byte_offset, 0);
        assert_eq!(self.file_offset, 0);
        (&mut self.bytes, &mut self.files)
    }

    pub fn push_back<T: Serialize>(&mut self, message: &T) -> Result<(), Error> {
        let mut serializer = postcard::Serializer { output: IPCBufferRef(self) };
        message.serialize(&mut serializer)
    }

    pub fn pop_front<'d, T: Deserialize<'d>>(&'d mut self) -> Result<T, Error> {
        let input = &self.bytes[self.byte_offset..];
        let (message, remainder) = postcard::take_from_bytes(input)?;
        self.byte_offset += input.len() - remainder.len();
        Ok(message)
    }
}

struct IPCBufferRef<'a>(&'a mut IPCBuffer);

impl<'a> SerFlavor for IPCBufferRef<'a> {
    type Output = &'a mut IPCBuffer;

    fn try_extend(&mut self, data: &[u8]) -> core::result::Result<(), ()> {
        self.0.bytes.extend_from_slice(data)
    }

    fn try_push(&mut self, data: u8) -> core::result::Result<(), ()> {
        self.0.bytes.push(data).map_err(|_| ())
    }

    fn release(self) -> core::result::Result<Self::Output, ()> {
        unreachable!();
    }
}

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
