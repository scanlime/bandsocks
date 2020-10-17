// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

use core::fmt;
use heapless::Vec;
use postcard::{flavors::SerFlavor, Error};
use serde::{ser, de};
use typenum::*;

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
pub enum FromSand {
    OpenProcess(SysPid),
    SysAccess(SysAccess),
    SysOpen(SysAccess, i32),
    SysKill(VPid, Signal),
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
pub struct SysAccess {
    pub dir: Option<SysFd>,
    pub path: VString,
    pub mode: i32,
}

pub type BufferedBytesMax = U128;
pub type BufferedFilesMax = U8;

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
            file_offset: 0,
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

    pub fn push_back<T: ser::Serialize>(&mut self, message: &T) -> Result<(), Error> {
        let mut serializer = postcard::Serializer {
            output: IPCBufferRef(self),
        };
        message.serialize(&mut serializer)
    }

    pub fn pop_front<'d, T: de::Deserialize<'d>>(&'d mut self) -> Result<T, Error> {
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

impl ser::Serialize for SysFd {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_struct("SysFd")
    }
}

impl<'d> de::Deserialize<'d> for SysFd {
    fn deserialize<D: de::Deserializer<'d>>(deserializer: D) -> Result<Self, D::Error> {
        struct SysFdVisitor;
        impl<'d> de::Visitor<'d> for SysFdVisitor {
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

type InnerSerializer<'a> = postcard::Serializer<IPCBufferRef<'a>>;

struct IPCSerializer<'a> {
    inner: InnerSerializer<'a>,
}

impl<'a, 'b> ser::Serializer for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn is_human_readable(&self) -> bool {
        false
    }
    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_bool(&mut self.inner, v)
    }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_i8(&mut self.inner, v)
    }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_u8(&mut self.inner, v)
    }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_i16(&mut self.inner, v)
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_u16(&mut self.inner, v)
    }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_i32(&mut self.inner, v)
    }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_f32(&mut self.inner, v)
    }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_u32(&mut self.inner, v)
    }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_i64(&mut self.inner, v)
    }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_f64(&mut self.inner, v)
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_u64(&mut self.inner, v)
    }
    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_char(&mut self.inner, v)
    }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_str(&mut self.inner, v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_bytes(&mut self.inner, v)
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_none(&mut self.inner)
    }
    fn serialize_some<T: ?Sized + ser::Serialize>(self, v: &T) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_some(&mut self.inner, v)
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_unit(&mut self.inner)
    }
    fn serialize_unit_struct(self, n: &'static str) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_unit_struct(&mut self.inner, n)
    }
    fn serialize_unit_variant(self, n: &'static str, i: u32, v: &'static str) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_unit_variant(&mut self.inner, n, i, v)
    }
    fn serialize_newtype_struct<T: ?Sized + ser::Serialize>(self, n: &'static str, v: &T) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_newtype_struct(&mut self.inner, n, v)
    }
    fn serialize_newtype_variant<T: ?Sized + ser::Serialize>(self, n: &'static str, i: u32, v: &'static str, t: &T) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_newtype_variant(&mut self.inner, n, i, v, t)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_seq(&mut self.inner, len)?;
        Ok(self)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_tuple(&mut self.inner, len)?;
        Ok(self)
    }
    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_tuple_struct(&mut self.inner, name, len)?;
        Ok(self)
    }
    fn serialize_tuple_variant(self, n: &'static str, i: u32, v: &'static str, l: usize) -> Result<Self::SerializeTupleVariant, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_tuple_variant(&mut self.inner, n, i, v, l)?;
        Ok(self)
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_map(&mut self.inner, len)?;
        Ok(self)
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_struct(&mut self.inner, name, len)?;
        Ok(self)
    }
    fn serialize_struct_variant(self, n: &'static str, i: u32, v: &'static str, l: usize) -> Result<Self::SerializeStructVariant, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::serialize_struct_variant(&mut self.inner, n, i, v, l)?;
        Ok(self)
    }
    fn collect_str<T: ?Sized + fmt::Display>(self, v: &T) -> Result<Self::Ok, Self::Error> {
        <&mut InnerSerializer<'a> as ser::Serializer>::collect_str(&mut self.inner, v)
    }
}

impl<'a, 'b> ser::SerializeSeq for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeSeq>::serialize_element(&mut &mut self.inner, value)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a, 'b> ser::SerializeTuple for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeTuple>::serialize_element(&mut &mut self.inner, value)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a, 'b> ser::SerializeTupleStruct for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeTupleStruct>::serialize_field(&mut &mut self.inner, value)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a, 'b> ser::SerializeTupleVariant for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeTupleVariant>::serialize_field(&mut &mut self.inner, value)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}


impl<'a, 'b> ser::SerializeMap for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_key<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeMap>::serialize_key(&mut &mut self.inner, value)
    }
    fn serialize_value<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeMap>::serialize_value(&mut &mut self.inner, value)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}


impl<'a, 'b> ser::SerializeStruct for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, k: &'static str, v: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeStruct>::serialize_field(&mut &mut self.inner, k, v)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}


impl<'a, 'b> ser::SerializeStructVariant for &'b mut IPCSerializer<'a> {
    type Ok = <&'b mut InnerSerializer<'a> as ser::Serializer>::Ok;
    type Error = <&'b mut InnerSerializer<'a> as ser::Serializer>::Error;
    fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, k: &'static str, v: &T) -> Result<(), Self::Error> {
        <&mut InnerSerializer<'a> as ser::SerializeStructVariant>::serialize_field(&mut &mut self.inner, k, v)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}
