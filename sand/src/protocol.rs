// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

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

pub mod buffer {
    use super::{de, ser, SysFd};
    use core::fmt;
    use heapless::{consts::*, Vec};
    use serde::{de::DeserializeOwned, Serialize};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum Error {
        Unimplemented,
        UnexpectedEnd,
        BufferFull,
        InvalidValue,
        Serialize,
        Deserialize,
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    pub type Result<T> = core::result::Result<T, Error>;
    pub type BytesMax = U128;
    pub type FilesMax = U8;

    #[derive(Default)]
    pub struct IPCBuffer {
        bytes: Vec<u8, BytesMax>,
        files: Vec<SysFd, FilesMax>,
        byte_offset: usize,
        file_offset: usize,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct IPCSlice<'a> {
        pub bytes: &'a [u8],
        pub files: &'a [SysFd],
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct IPCSliceMut<'a> {
        pub bytes: &'a mut [u8],
        pub files: &'a mut [SysFd],
    }

    impl<'a> IPCBuffer {
        pub fn new() -> Self {
            Default::default()
        }

        pub fn reset(&mut self) {
            self.bytes.clear();
            self.files.clear();
            self.byte_offset = 0;
            self.file_offset = 0;
        }

        pub fn byte_capacity(&self) -> usize {
            self.bytes.capacity()
        }

        pub unsafe fn set_len(&mut self, num_bytes: usize, num_files: usize) {
            assert_eq!(self.byte_offset, 0);
            assert_eq!(self.file_offset, 0);
            assert!(num_bytes <= self.bytes.capacity());
            assert!(num_files <= self.files.capacity());
            self.bytes.set_len(num_bytes);
            self.files.set_len(num_files);
        }

        pub fn is_empty(&self) -> bool {
            self.byte_offset == self.bytes.len() && self.file_offset == self.files.len()
        }

        pub fn as_slice(&'a self) -> IPCSlice<'a> {
            IPCSlice {
                bytes: &self.bytes[self.byte_offset..],
                files: &self.files[self.file_offset..],
            }
        }

        pub fn as_slice_mut(&'a mut self) -> IPCSliceMut<'a> {
            IPCSliceMut {
                bytes: &mut self.bytes[self.byte_offset..],
                files: &mut self.files[self.file_offset..],
            }
        }

        pub fn push_back<T: Serialize>(&mut self, message: &T) -> Result<()> {
            let mut serializer = ser::IPCSerializer::new(self);
            message.serialize(&mut serializer)
        }

        pub fn pop_front<T: Clone + DeserializeOwned>(&'a mut self) -> Result<T> {
            let mut deserializer = de::IPCDeserializer::new(self);
            T::deserialize(&mut deserializer)
        }

        pub fn extend_bytes(&mut self, data: &[u8]) -> Result<()> {
            self.bytes
                .extend_from_slice(data)
                .map_err(|_| Error::BufferFull)
        }

        pub fn push_back_byte(&mut self, data: u8) -> Result<()> {
            self.bytes.push(data).map_err(|_| Error::BufferFull)
        }

        pub fn push_back_file(&mut self, file: SysFd) -> Result<()> {
            self.files.push(file).map_err(|_| Error::BufferFull)
        }

        pub fn front_bytes(&self, len: usize) -> Result<&[u8]> {
            let bytes = self.as_slice().bytes;
            if len <= bytes.len() {
                Ok(&bytes[..len])
            } else {
                Err(Error::UnexpectedEnd)
            }
        }

        pub fn front_files(&self, len: usize) -> Result<&[SysFd]> {
            let files = self.as_slice().files;
            if len <= files.len() {
                Ok(&files[..len])
            } else {
                Err(Error::UnexpectedEnd)
            }
        }

        pub fn pop_front_bytes(&mut self, len: usize) {
            let new_offset = self.byte_offset + len;
            assert!(new_offset <= self.bytes.len());
            self.byte_offset = new_offset;
        }

        pub fn pop_front_files(&mut self, len: usize) {
            let new_offset = self.file_offset + len;
            assert!(new_offset <= self.files.len());
            self.file_offset = new_offset;
        }

        pub fn pop_front_byte(&mut self) -> Result<u8> {
            let result = self.front_bytes(1).map(|slice| slice[0]);
            if result.is_ok() {
                self.pop_front_bytes(1);
            }
            result
        }

        pub fn pop_front_file(&mut self) -> Result<SysFd> {
            let result = self.front_files(1).map(|slice| slice[0].clone());
            if result.is_ok() {
                self.pop_front_files(1);
            }
            result
        }
    }
}

mod ser {
    use super::{
        buffer::{Error, IPCBuffer, Result},
        SysFd,
    };
    use core::{fmt::Display, result};
    use serde::{ser, ser::SerializeTupleStruct};

    const SYSFD: &str = "SysFd@ser";

    pub struct IPCSerializer<'a> {
        output: &'a mut IPCBuffer,
        in_sysfd: bool,
    }

    impl<'a> IPCSerializer<'a> {
        pub fn new(output: &'a mut IPCBuffer) -> Self {
            IPCSerializer {
                output,
                in_sysfd: false,
            }
        }
    }

    impl ser::Serialize for SysFd {
        fn serialize<S: ser::Serializer>(&self, serializer: S) -> result::Result<S::Ok, S::Error> {
            let mut tuple = serializer.serialize_tuple_struct(SYSFD, 1)?;
            tuple.serialize_field(&self.0)?;
            tuple.end()
        }
    }

    impl ser::StdError for Error {}

    impl ser::Error for Error {
        fn custom<T: Display>(_msg: T) -> Self {
            Error::Serialize
        }
    }

    macro_rules! to_le_bytes {
        ($gen_fn:ident, $num:ty ) => {
            fn $gen_fn(self, v: $num) -> Result<()> {
                assert_eq!(self.in_sysfd, false);
                self.output.extend_bytes(&v.to_le_bytes())
            }
        };
    }

    impl<'a, 'b> ser::Serializer for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
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

        fn collect_str<T: ?Sized + Display>(self, _v: &T) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_bool(self, v: bool) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(v as u8)
        }

        fn serialize_f32(self, _v: f32) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_f64(self, _v: f64) -> Result<()> {
            Err(Error::Unimplemented)
        }

        to_le_bytes!(serialize_u16, u16);
        to_le_bytes!(serialize_i16, i16);
        to_le_bytes!(serialize_i32, i32);
        to_le_bytes!(serialize_u64, u64);
        to_le_bytes!(serialize_i64, i64);

        fn serialize_u32(self, v: u32) -> Result<()> {
            if self.in_sysfd {
                self.output.push_back_file(SysFd(v))
            } else {
                self.output.extend_bytes(&v.to_le_bytes())
            }
        }

        fn serialize_none(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(0)
        }

        fn serialize_some<T: ?Sized + ser::Serialize>(self, v: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(1)?;
            v.serialize(self)
        }

        fn serialize_i8(self, v: i8) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(v as u8)
        }

        fn serialize_u8(self, v: u8) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(v)
        }

        fn serialize_unit(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }

        fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
            self.serialize_unit()
        }

        fn serialize_unit_variant(
            self,
            _name: &'static str,
            variant_index: u32,
            _var: &'static str,
        ) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            if variant_index < 0x100 {
                self.output.push_back_byte(variant_index as u8)
            } else {
                Err(Error::InvalidValue)
            }
        }

        fn serialize_char(self, _v: char) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_str(self, _v: &str) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_bytes(self, _v: &[u8]) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_newtype_struct<T>(self, _: &'static str, value: &T) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            value.serialize(self)
        }

        fn serialize_newtype_variant<T>(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            value: &T,
        ) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            self.serialize_unit_variant(name, variant_index, variant)?;
            value.serialize(self)
        }

        fn serialize_tuple_struct(self, name: &'static str, _len: usize) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            self.in_sysfd = name == SYSFD;
            Ok(self)
        }

        fn serialize_seq(self, _len: Option<usize>) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_tuple(self, _len: usize) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_map(self, _len: Option<usize>) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_tuple_variant(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            _len: usize,
        ) -> Result<Self> {
            self.serialize_unit_variant(name, variant_index, variant)?;
            Ok(self)
        }

        fn serialize_struct_variant(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            _len: usize,
        ) -> Result<Self> {
            self.serialize_unit_variant(name, variant_index, variant)?;
            Ok(self)
        }
    }

    impl<'a, 'b> ser::SerializeSeq for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeTuple for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeTupleStruct for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            self.in_sysfd = false;
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeTupleVariant for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeMap for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_key<T: ?Sized + ser::Serialize>(&mut self, key: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            key.serialize(&mut **self)
        }

        fn serialize_value<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeStruct for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeStructVariant for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }
}

mod de {
    use super::{
        buffer::{Error, IPCBuffer, Result},
        SysFd,
    };
    use core::{fmt, fmt::Display, result};
    use serde::de;

    const SYSFD: &str = "SysFd@de";

    pub struct IPCDeserializer<'d> {
        input: &'d mut IPCBuffer,
    }

    impl<'a> IPCDeserializer<'a> {
        pub fn new(input: &'a mut IPCBuffer) -> Self {
            IPCDeserializer { input }
        }
    }

    impl<'d> de::Deserialize<'d> for SysFd {
        fn deserialize<D: de::Deserializer<'d>>(deserializer: D) -> result::Result<Self, D::Error> {
            struct SysFdVisitor;
            impl<'d> de::Visitor<'d> for SysFdVisitor {
                type Value = SysFd;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("struct SysFd")
                }

                fn visit_seq<V>(self, _seq: V) -> result::Result<SysFd, V::Error>
                where
                    V: de::SeqAccess<'d>,
                {
                    Ok(SysFd(1234))
                }
            }
            deserializer.deserialize_tuple_struct(SYSFD, 1, SysFdVisitor)
        }
    }

    impl de::Error for Error {
        fn custom<T: Display>(_msg: T) -> Self {
            Error::Deserialize
        }
    }

    macro_rules! from_le_bytes {
        ($gen_fn:ident, $visit_fn:ident, $num:ty, $len:expr) => {
            fn $gen_fn<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
                let mut bytes = [0u8; $len];
                bytes[..].copy_from_slice(self.input.front_bytes($len)?);
                self.input.pop_front_bytes($len);
                visitor.$visit_fn(<$num>::from_le_bytes(bytes))
            }
        };
    }

    impl<'d, 'a> de::Deserializer<'d> for &'a mut IPCDeserializer<'d> {
        type Error = Error;

        fn is_human_readable(&self) -> bool {
            false
        }

        fn deserialize_any<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_byte_buf<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_bytes<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_char<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_f32<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_f64<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_identifier<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_ignored_any<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_str<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_string<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        from_le_bytes!(deserialize_u16, visit_u16, u16, 2);
        from_le_bytes!(deserialize_i16, visit_i16, i16, 2);
        from_le_bytes!(deserialize_u32, visit_u32, u32, 4);
        from_le_bytes!(deserialize_i32, visit_i32, i32, 4);
        from_le_bytes!(deserialize_u64, visit_u64, u64, 8);
        from_le_bytes!(deserialize_i64, visit_i64, i64, 8);

        fn deserialize_u8<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            visitor.visit_u8(self.input.pop_front_byte()?)
        }

        fn deserialize_i8<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            visitor.visit_i8(self.input.pop_front_byte()? as i8)
        }

        fn deserialize_bool<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            match self.input.pop_front_byte()? {
                0 => visitor.visit_bool(false),
                1 => visitor.visit_bool(true),
                _ => Err(Error::InvalidValue),
            }
        }

        fn deserialize_option<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            match self.input.pop_front_byte()? {
                0 => visitor.visit_none(),
                1 => visitor.visit_some(self),
                _ => Err(Error::InvalidValue),
            }
        }

        fn deserialize_unit<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            visitor.visit_unit()
        }

        fn deserialize_unit_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value> {
            visitor.visit_unit()
        }

        fn deserialize_map<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_seq<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_tuple<V: de::Visitor<'d>>(self, len: usize, visitor: V) -> Result<V::Value> {
            visitor.visit_seq(SeqAccess {
                deserializer: self,
                len,
            })
        }

        fn deserialize_tuple_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            len: usize,
            visitor: V,
        ) -> Result<V::Value> {
            self.deserialize_tuple(len, visitor)
        }

        fn deserialize_enum<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            _visitor: V,
        ) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_newtype_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value> {
            visitor.visit_newtype_struct(self)
        }

        fn deserialize_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            _fields: &'static [&'static str],
            _visitor: V,
        ) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }
    }

    struct SeqAccess<'d, 'a> {
        deserializer: &'a mut IPCDeserializer<'d>,
        len: usize,
    }

    impl<'d, 'a> de::SeqAccess<'d> for SeqAccess<'d, 'a> {
        type Error = Error;

        fn size_hint(&self) -> Option<usize> {
            Some(self.len)
        }

        fn next_element_seed<V>(&mut self, seed: V) -> Result<Option<V::Value>>
        where
            V: de::DeserializeSeed<'d>,
        {
            if self.len > 0 {
                self.len -= 1;
                Ok(Some(de::DeserializeSeed::deserialize(
                    seed,
                    &mut *self.deserializer,
                )?))
            } else {
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{
        buffer::{Error, IPCBuffer, IPCSlice},
        Errno, SysFd, VPtr,
    };

    macro_rules! check {
        ($name:ident, $msg:expr, $t:ty, $bytes:expr, $files:expr) => {
            #[test]
            fn $name() {
                let mut buf = IPCBuffer::new();
                let msg: $t = $msg;
                let bytes: &[u8] = &$bytes;
                let files: &[SysFd] = &$files;
                buf.push_back(&msg).expect("push_back");
                assert_eq!(buf.as_slice().bytes, bytes);
                assert_eq!(buf.as_slice().files, files);
                assert_eq!(buf.pop_front::<$t>().expect("pop_front"), msg);
                assert!(buf.is_empty());
            }
        };
    }

    macro_rules! nope {
        ($name: ident, $msg:expr, $t:ty) => {
            #[test]
            fn $name() {
                let mut buf = IPCBuffer::new();
                let msg: $t = $msg;
                assert_eq!(buf.push_back(&msg), Err(Error::Unimplemented));
                assert!(buf.is_empty());
            }
        };
    }

    nope!(no_char, 'n', char);
    nope!(no_str, "blah", &str);
    nope!(no_f32, 1.0, f32);
    nope!(no_f64, 1.0, f64);

    check!(u32_1, 0x12345678, u32, [0x78, 0x56, 0x34, 0x12], []);
    check!(u32_2, 0x00000000, u32, [0x00, 0x00, 0x00, 0x00], []);
    check!(u32_3, 0xffffffff, u32, [0xff, 0xff, 0xff, 0xff], []);
    check!(u8_1, 0x42, u8, [0x42], []);
    check!(u8_2, 0x00, u8, [0x00], []);
    check!(u8_3, 0xff, u8, [0xff], []);
    check!(i32_1, 0x7fffffff, i32, [0xff, 0xff, 0xff, 0x7f], []);
    check!(i32_2, 0, i32, [0x00, 0x00, 0x00, 0x00], []);
    check!(i32_3, -1, i32, [0xff, 0xff, 0xff, 0xff], []);
    check!(i8_1, 50, i8, [50], []);
    check!(i8_2, 0, i8, [0x00], []);
    check!(i8_3, -1, i8, [0xff], []);
    check!(u64_1, 0, u64, [0, 0, 0, 0, 0, 0, 0, 0], []);
    check!(fd_1, SysFd(0x87654321), SysFd, [], [SysFd(0x87654321)]);
    check!(fd_2, SysFd(0), SysFd, [], [SysFd(0)]);
    check!(fd_ok, Ok(SysFd(123)), Result<SysFd, Errno>, [0], [SysFd(123)]);
    check!(fd_err, Err(Errno(-2)), Result<SysFd, Errno>, [1, 0xfe, 0xff, 0xff, 0xff], []);

    check!(
        fd_array,
        [SysFd(5), SysFd(4), SysFd(3), SysFd(2), SysFd(1)],
        [SysFd; 5],
        [],

        [SysFd(5), SysFd(4), SysFd(3), SysFd(2), SysFd(1)]
    );
    check!(
        vptr_1,
        VPtr(0x1122334455667788),
        VPtr,
        [0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11],
        []
    );
    check!(
        fixed_len_bytes,
        (true, *b"blahh", 1),
        (bool, [u8; 5], u32),
        [1, 98, 108, 97, 104, 104, 1, 0, 0, 0],
        []
    );
    check!(
        tuple_1,
        (true, false, false, 0xabcd, 0xaabbccdd00112233),
        (bool, bool, bool, u16, u64),
        [1, 0, 0, 0xcd, 0xab, 0x33, 0x22, 0x11, 0x00, 0xdd, 0xcc, 0xbb, 0xaa],
        []
    );

    #[test]
    fn bool() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&true).unwrap();
        assert_eq!(buf.as_slice().bytes, &[1]);
        assert_eq!(buf.pop_front::<bool>(), Ok(true));
        assert!(buf.is_empty());
        buf.push_back(&false).unwrap();
        assert_eq!(buf.as_slice().bytes, &[0]);
        assert_eq!(buf.pop_front::<bool>(), Ok(false));
        assert!(buf.is_empty());
        buf.push_back_byte(1).unwrap();
        buf.push_back_byte(0).unwrap();
        assert_eq!(buf.pop_front::<bool>(), Ok(true));
        assert_eq!(buf.pop_front::<bool>(), Ok(false));
        assert!(buf.is_empty());
        buf.push_back_byte(2).unwrap();
        assert_eq!(buf.pop_front::<bool>(), Err(Error::InvalidValue));
        assert!(buf.is_empty());
    }

    #[test]
    fn option() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&Some(false)).unwrap();
        buf.push_back(&Some(42u8)).unwrap();
        buf.push_back::<Option<u64>>(&None).unwrap();
        buf.push_back::<Option<()>>(&None).unwrap();
        assert_eq!(buf.as_slice().bytes, &[1, 0, 1, 42, 0, 0]);
        assert_eq!(buf.pop_front::<Option<bool>>(), Ok(Some(false)));
        assert_eq!(buf.pop_front::<Option<u8>>(), Ok(Some(42u8)));
        assert_eq!(buf.pop_front::<Option<u64>>(), Ok(None));
        assert_eq!(buf.pop_front::<Option<()>>(), Ok(None));
        assert!(buf.is_empty());
    }
}
