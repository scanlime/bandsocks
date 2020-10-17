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
    use super::{de::deserialize, ser::serialize, SysFd};
    use heapless::Vec;
    use postcard::Error;
    use serde::{Deserialize, Serialize};
    use typenum::*;

    pub type BytesMax = U128;
    pub type FilesMax = U8;

    pub type Bytes = Vec<u8, BytesMax>;
    pub type Files = Vec<SysFd, FilesMax>;

    pub struct IPCBuffer {
        bytes: Bytes,
        files: Files,
        byte_offset: usize,
        file_offset: usize,
    }

    pub struct IPCSlice<'a> {
        pub bytes: &'a mut [u8],
        pub files: &'a mut [SysFd],
    }

    impl<'a> IPCBuffer {
        pub fn new() -> Self {
            IPCBuffer {
                bytes: Vec::new(),
                files: Vec::new(),
                byte_offset: 0,
                file_offset: 0,
            }
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

        pub fn as_mut(&'a mut self) -> IPCSlice<'a> {
            IPCSlice {
                bytes: &mut self.bytes[self.byte_offset..],
                files: &mut self.files[self.file_offset..],
            }
        }

        pub fn push_back<T: Serialize>(&mut self, message: &T) -> Result<(), Error> {
            serialize(self, message)
        }

        pub fn pop_front<'d, T: Deserialize<'d>>(&'d mut self) -> Result<T, Error> {
            deserialize(self)
        }

        pub fn extend_bytes(&mut self, data: &[u8]) -> Result<(), ()> {
            self.bytes.extend_from_slice(data)
        }

        pub fn push_back_byte(&mut self, data: u8) -> Result<(), ()> {
            self.bytes.push(data).map_err(|_| ())
        }
    }
}

mod serde_marker {
    pub const SYSFD: &str = "_SysFd";
    pub fn is_sysfd(s: &str) -> bool {
        s as *const str == SYSFD as *const str
    }
}

#[macro_use]
mod serde_macro {
    macro_rules! forward {
        () => {};
        ( @result ) => { core::result::Result<Self::Ok, Self::Error> };
        ( @collect $fn:ident, $generics:tt, $args:tt ) => {
            forward!{ @expand $fn, $generics, $args, $args }
        };
        ( @expand $fn:ident, ($($g:tt)*), ($($p1:ident : $t1:ty),*), ($($p2:ident : $t2:ty),*) ) => {
            fn $fn $($g)* (self, $($p1 : $t1),*) -> forward!{@result} {
                (&mut self.inner).$fn($($p1),*)
            }
        };
        ( fn $fn:ident(self); $($tail:tt)* ) => {
            forward!{ @collect $fn, (), () }
            forward!{ $($tail)* }
        };
        ( fn $fn:ident(self, $($par:ident : $t:ty),* ); $($tail:tt)* ) => {
            forward!{ @collect $fn, (), ( $( $par : $t ),* )}
            forward!{ $($tail)* }
        };
        ( fn $fn:ident<T: ?$t1:ident + $t2:ident>(self, $($par:ident : $t:ty),* ); $($tail:tt)* ) => {
            forward!{ @collect $fn, (<T: ?$t1 + $t2>), ( $( $par : $t ),* )}
            forward!{ $($tail)* }
        };
    }
}

mod ser {
    use super::{buffer::IPCBuffer, serde_marker, SysFd};
    use core::fmt::Display;
    use postcard::{flavors::SerFlavor, Error};
    use serde::ser::*;

    pub fn serialize<T: Serialize>(buffer: &mut IPCBuffer, message: &T) -> Result<(), Error> {
        let mut serializer = IPCSerializer {
            inner: postcard::Serializer {
                output: IPCBufferRef(buffer),
            },
        };
        message.serialize(&mut serializer)
    }

    struct IPCBufferRef<'a>(&'a mut IPCBuffer);

    type Inner<'a> = postcard::Serializer<IPCBufferRef<'a>>;

    struct IPCSerializer<'a> {
        inner: Inner<'a>,
    }

    impl<'a> SerFlavor for IPCBufferRef<'a> {
        type Output = &'a mut IPCBuffer;

        fn try_extend(&mut self, data: &[u8]) -> Result<(), ()> {
            self.0.extend_bytes(data)
        }

        fn try_push(&mut self, data: u8) -> Result<(), ()> {
            self.0.push_back_byte(data)
        }

        fn release(self) -> Result<Self::Output, ()> {
            unreachable!();
        }
    }

    impl Serialize for SysFd {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut tuple = serializer.serialize_tuple_struct(serde_marker::SYSFD, 1)?;
            tuple.serialize_field(&self.0)?;
            tuple.end()
        }
    }

    impl<'a, 'b> Serializer for &'b mut IPCSerializer<'a> {
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

        forward! {
            fn collect_str<T: ?Sized + Display>(self, v: &T);
            fn serialize_bool(self, v: bool);
            fn serialize_i8(self, v: i8);
            fn serialize_u8(self, v: u8);
            fn serialize_i16(self, v: i16);
            fn serialize_u16(self, v: u16);
            fn serialize_i32(self, v: i32);
            fn serialize_f32(self, v: f32);
            fn serialize_u32(self, v: u32);
            fn serialize_i64(self, v: i64);
            fn serialize_f64(self, v: f64);
            fn serialize_u64(self, v: u64);
            fn serialize_char(self, v: char);
            fn serialize_str(self, v: &str);
            fn serialize_bytes(self, v: &[u8]);
            fn serialize_none(self);
            fn serialize_unit(self);
            fn serialize_unit_struct(self, name: &'static str);
            fn serialize_unit_variant(self, name: &'static str, varidx: u32, var: &'static str);
            fn serialize_some<T: ?Sized + Serialize>(self, v: &T);
            fn serialize_newtype_struct<T: ?Sized + Serialize>(self, name: &'static str, v: &T);
            fn serialize_newtype_variant<T: ?Sized + Serialize>(self, name: &'static str,
                                    variant_index: u32, variant: &'static str, value: &T);
        }

        fn serialize_seq(self, len: Option<usize>) -> Result<Self, Error> {
            (&mut self.inner).serialize_seq(len)?;
            Ok(self)
        }

        fn serialize_tuple(self, len: usize) -> Result<Self, Error> {
            (&mut self.inner).serialize_tuple(len)?;
            Ok(self)
        }

        fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self, Error> {
            (&mut self.inner).serialize_tuple_struct(name, len)?;
            Ok(self)
        }

        fn serialize_map(self, len: Option<usize>) -> Result<Self, Error> {
            (&mut self.inner).serialize_map(len)?;
            Ok(self)
        }

        fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self, Error> {
            (&mut self.inner).serialize_struct(name, len)?;
            Ok(self)
        }

        fn serialize_tuple_variant(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, Error> {
            (&mut self.inner).serialize_tuple_variant(name, variant_index, variant, len)?;
            Ok(self)
        }

        fn serialize_struct_variant(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, Error> {
            (&mut self.inner).serialize_struct_variant(name, variant_index, variant, len)?;
            Ok(self)
        }
    }

    impl<'a, 'b> SerializeSeq for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_element<T: ?Sized + Serialize>(
            &mut self,
            value: &T,
        ) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeSeq>::serialize_element(&mut &mut self.inner, value)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }

    impl<'a, 'b> SerializeTuple for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_element<T: ?Sized + Serialize>(
            &mut self,
            value: &T,
        ) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeTuple>::serialize_element(&mut &mut self.inner, value)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }

    impl<'a, 'b> SerializeTupleStruct for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeTupleStruct>::serialize_field(&mut &mut self.inner, value)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }

    impl<'a, 'b> SerializeTupleVariant for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeTupleVariant>::serialize_field(&mut &mut self.inner, value)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }

    impl<'a, 'b> SerializeMap for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_key<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeMap>::serialize_key(&mut &mut self.inner, value)
        }
        fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeMap>::serialize_value(&mut &mut self.inner, value)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }

    impl<'a, 'b> SerializeStruct for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_field<T: ?Sized + Serialize>(
            &mut self,
            k: &'static str,
            v: &T,
        ) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeStruct>::serialize_field(&mut &mut self.inner, k, v)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }

    impl<'a, 'b> SerializeStructVariant for &'b mut IPCSerializer<'a> {
        type Ok = <&'b mut Inner<'a> as Serializer>::Ok;
        type Error = <&'b mut Inner<'a> as Serializer>::Error;
        fn serialize_field<T: ?Sized + Serialize>(
            &mut self,
            k: &'static str,
            v: &T,
        ) -> Result<(), Self::Error> {
            <&mut Inner<'a> as SerializeStructVariant>::serialize_field(&mut &mut self.inner, k, v)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            Ok(())
        }
    }
}

mod de {
    use super::{buffer::IPCBuffer, serde_marker, SysFd};
    use core::fmt::{self, Formatter};
    use postcard::Error;
    use serde::de::*;

    pub fn deserialize<'a, T: Deserialize<'a>>(buffer: &'a mut IPCBuffer) -> Result<T, Error> {
        //  let (bytes, files) = buffer.as_mut_parts();
        //  let (message, remainder): (T, &[u8]) = postcard::take_from_bytes(bytes)?;
        unreachable!();
    }

    struct SysFdVisitor;

    impl<'d> Visitor<'d> for SysFdVisitor {
        type Value = SysFd;
        fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
            write!(formatter, "a `SysFd`")
        }
    }

    impl<'d> Deserialize<'d> for SysFd {
        fn deserialize<D: Deserializer<'d>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_tuple_struct(serde_marker::SYSFD, 1, SysFdVisitor)
        }
    }
}
