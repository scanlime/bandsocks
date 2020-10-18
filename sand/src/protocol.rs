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
    use serde::{de::DeserializeOwned, Serialize};
    use typenum::*;

    pub type BytesMax = U128;
    pub type FilesMax = U8;

    pub type Bytes = Vec<u8, BytesMax>;
    pub type Files = Vec<SysFd, FilesMax>;

    #[derive(Default)]
    pub struct IPCBuffer {
        bytes: Bytes,
        files: Files,
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

        pub fn push_back<T: Serialize>(&mut self, message: &T) -> Result<(), Error> {
            serialize(self, message)
        }

        pub fn pop_front<T: Clone + DeserializeOwned>(&'a mut self) -> Result<T, Error> {
            let original = self.as_slice();
            let original_bytes_len = original.bytes.len();
            let (message, remainder) = deserialize::<T>(original)?;
            let bytes_used = original_bytes_len - remainder.bytes.len();
            self.pop_front_bytes(bytes_used);
            Ok(message)
        }

        pub fn extend_bytes(&mut self, data: &[u8]) -> Result<(), ()> {
            self.bytes.extend_from_slice(data)
        }

        pub fn push_back_byte(&mut self, data: u8) -> Result<(), ()> {
            self.bytes.push(data).map_err(|_| ())
        }

        pub fn push_back_file(&mut self, file: SysFd) -> Result<(), ()> {
            self.files.push(file).map_err(|_| ())
        }

        pub fn pop_front_bytes(&mut self, len: usize) {
            let new_offset = self.byte_offset + len;
            assert!(new_offset <= self.bytes.len());
            self.byte_offset = new_offset;
        }
    }
}

mod ser {
    use super::{buffer::IPCBuffer, SysFd};
    use core::fmt::Display;
    use postcard::{flavors::SerFlavor, Error};
    use serde::ser::*;

    pub fn serialize<T: Serialize>(output: &mut IPCBuffer, message: &T) -> Result<(), Error> {
        let mut serializer = IPCSerializer {
            in_sysfd_tuple_struct: false,
            inner: postcard::Serializer { output },
        };
        message.serialize(&mut serializer)
    }

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

    type Inner<'a> = postcard::Serializer<&'a mut IPCBuffer>;

    const SYSFD_MARKER: &str = "_@SysFd";

    struct IPCSerializer<'a> {
        in_sysfd_tuple_struct: bool,
        inner: Inner<'a>,
    }

    impl<'a> SerFlavor for &'a mut IPCBuffer {
        type Output = ();

        fn try_extend(&mut self, data: &[u8]) -> Result<(), ()> {
            self.extend_bytes(data)
        }

        fn try_push(&mut self, data: u8) -> Result<(), ()> {
            self.push_back_byte(data)
        }

        fn release(self) -> Result<(), ()> {
            unreachable!();
        }
    }

    impl Serialize for SysFd {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut tuple = serializer.serialize_tuple_struct(SYSFD_MARKER, 1)?;
            tuple.serialize_field(&self.0)?;
            tuple.end()
        }
    }

    impl<'a> IPCSerializer<'a> {
        fn serialize_sysfd(&mut self, value: SysFd) -> Result<(), Error> {
            self.inner
                .output
                .push_back_file(value)
                .map_err(|_| Error::SerializeBufferFull)
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

        forward! {
            fn collect_str<T: ?Sized + Display>(self, v: &T);
            fn serialize_bool(self, v: bool);
            fn serialize_i8(self, v: i8);
            fn serialize_u8(self, v: u8);
            fn serialize_i16(self, v: i16);
            fn serialize_u16(self, v: u16);
            fn serialize_i32(self, v: i32);
            fn serialize_f32(self, v: f32);
            fn serialize_i64(self, v: i64);
            fn serialize_f64(self, v: f64);
            fn serialize_u64(self, v: u64);
            fn serialize_none(self);
            fn serialize_some<T: ?Sized + Serialize>(self, v: &T);
            fn serialize_unit(self);
            fn serialize_unit_struct(self, name: &'static str);
            fn serialize_unit_variant(self, name: &'static str, varidx: u32, var: &'static str);
        }

        fn is_human_readable(&self) -> bool {
            false
        }

        fn serialize_char(self, _v: char) -> Result<(), Error> {
            Err(Error::WontImplement)
        }

        fn serialize_str(self, _v: &str) -> Result<(), Error> {
            Err(Error::WontImplement)
        }

        fn serialize_bytes(self, _v: &[u8]) -> Result<(), Error> {
            Err(Error::WontImplement)
        }

        fn serialize_u32(mut self, v: u32) -> Result<(), Error> {
            if self.in_sysfd_tuple_struct {
                (&mut self).serialize_sysfd(SysFd(v))
            } else {
                (&mut self.inner).serialize_u32(v)
            }
        }

        fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self, Error> {
            if name == SYSFD_MARKER {
                assert_eq!(self.in_sysfd_tuple_struct, false);
                self.in_sysfd_tuple_struct = true;
            }
            (&mut self.inner).serialize_tuple_struct(name, len)?;
            Ok(self)
        }

        fn serialize_newtype_struct<T>(self, _: &'static str, value: &T) -> Result<(), Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(self)
        }

        fn serialize_newtype_variant<T>(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            value: &T,
        ) -> Result<(), Error>
        where
            T: ?Sized + Serialize,
        {
            (&mut self.inner).serialize_unit_variant(name, variant_index, variant)?;
            value.serialize(self)
        }

        fn serialize_seq(self, len: Option<usize>) -> Result<Self, Error> {
            (&mut self.inner).serialize_seq(len)?;
            Ok(self)
        }

        fn serialize_tuple(self, len: usize) -> Result<Self, Error> {
            (&mut self.inner).serialize_tuple(len)?;
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
        type Ok = ();
        type Error = Error;
        fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            assert_eq!(self.in_sysfd_tuple_struct, false);
            Ok(())
        }
    }

    impl<'a, 'b> SerializeTuple for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            assert_eq!(self.in_sysfd_tuple_struct, false);
            Ok(())
        }
    }

    impl<'a, 'b> SerializeTupleStruct for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            self.in_sysfd_tuple_struct = false;
            Ok(())
        }
    }

    impl<'a, 'b> SerializeTupleVariant for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            assert_eq!(self.in_sysfd_tuple_struct, false);
            Ok(())
        }
    }

    impl<'a, 'b> SerializeMap for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            key.serialize(&mut **self)
        }
        fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            assert_eq!(self.in_sysfd_tuple_struct, false);
            Ok(())
        }
    }

    impl<'a, 'b> SerializeStruct for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            assert_eq!(self.in_sysfd_tuple_struct, false);
            Ok(())
        }
    }

    impl<'a, 'b> SerializeStructVariant for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize,
        {
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<Self::Ok, Self::Error> {
            assert_eq!(self.in_sysfd_tuple_struct, false);
            Ok(())
        }
    }
}

mod de {
    use super::{buffer::IPCSlice, SysFd};
    use postcard::Error;
    use serde::de::*;

    pub fn deserialize<'a, T>(buffer: IPCSlice<'a>) -> Result<(T, IPCSlice<'a>), Error>
    where
        T: Deserialize<'a>,
    {
        println!(
            "deserialize {} bytes and {} files",
            buffer.bytes.len(),
            buffer.files.len()
        );
        let (message, bytes) = postcard::take_from_bytes(buffer.bytes)?;
        let files = buffer.files;
        Ok((message, IPCSlice { bytes, files }))
    }

    impl<'d> Deserialize<'d> for SysFd {
        fn deserialize<D: Deserializer<'d>>(deserializer: D) -> Result<Self, D::Error> {
            println!("would deserialize a file here");
            Ok(SysFd(999))
        }
    }
}

#[cfg(test)]
mod test {
    use super::{
        buffer::{IPCBuffer, IPCSlice},
        *,
    };
    use postcard::Error;

    #[test]
    fn u32() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&0x12345678u32).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[0x78, 0x56, 0x34, 0x12],
                files: &[],
            }
        );
        assert_eq!(buf.pop_front::<u32>().unwrap(), 0x12345678);
        assert!(buf.is_empty());
    }

    #[test]
    fn u8() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&0x42u8).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[0x42],
                files: &[],
            }
        );
        assert_eq!(buf.pop_front::<u8>().unwrap(), 0x42);
        assert!(buf.is_empty());
    }

    #[test]
    fn u64() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&0x12345678abcdabbau64).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[0xba, 0xab, 0xcd, 0xab, 0x78, 0x56, 0x34, 0x12],
                files: &[],
            }
        );
        assert_eq!(buf.pop_front::<u64>().unwrap(), 0x12345678abcdabbau64);
        assert!(buf.is_empty());
    }

    #[test]
    fn i32() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&-1i32).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[0xff, 0xff, 0xff, 0xff],
                files: &[],
            }
        );
        assert_eq!(buf.pop_front::<i32>().unwrap(), -1);
        assert!(buf.is_empty());
    }

    #[test]
    fn no_char() {
        let mut buf = IPCBuffer::new();
        assert_eq!(buf.push_back(&'ก'), Err(Error::WontImplement));
    }

    #[test]
    fn no_str() {
        let mut buf = IPCBuffer::new();
        assert_eq!(buf.push_back(&"yo"), Err(Error::WontImplement));
    }

    #[test]
    fn fixed_len_bytes() {
        let mut buf = IPCBuffer::new();
        let msg = (true, b"blah");
        buf.push_back(&msg).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[1, 98, 108, 97, 104],
                files: &[],
            }
        );
        let result = buf.pop_front::<(bool, [u8; 4])>().unwrap();
        assert_eq!(&result.1, msg.1);
        assert!(buf.is_empty());
    }

    #[test]
    fn vptr() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&VPtr(0x1122334455667788)).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11],
                files: &[],
            }
        );
        assert_eq!(buf.pop_front::<VPtr>().unwrap(), VPtr(0x1122334455667788));
        assert!(buf.is_empty());
    }

    #[test]
    fn sysfd() {
        let mut buf = IPCBuffer::new();
        buf.push_back(&SysFd(0x87654321)).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[],
                files: &[SysFd(0x87654321)],
            }
        );
        assert_eq!(buf.pop_front::<SysFd>().unwrap(), SysFd(0x87654321));
        assert!(buf.is_empty());
    }

    #[test]
    fn sysfd_multi() {
        let mut buf = IPCBuffer::new();
        type T = [SysFd; 4];
        let msg: T = [SysFd(5), SysFd(6), SysFd(7), SysFd(8)];
        buf.push_back(&msg).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[],
                files: &msg,
            }
        );
        assert_eq!(buf.pop_front::<T>().unwrap(), msg);
        assert!(buf.is_empty());
    }

    #[test]
    fn sysfd_result_ok() {
        let mut buf = IPCBuffer::new();
        type T = Result<SysFd, Errno>;
        let msg: T = Ok(SysFd(0x12341122));
        buf.push_back(&msg).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[0],
                files: &[SysFd(0x12341122)],
            }
        );
        assert_eq!(buf.pop_front::<T>().unwrap(), msg);
        assert!(buf.is_empty());
    }

    #[test]
    fn sysfd_result_err() {
        let mut buf = IPCBuffer::new();
        type T = Result<SysFd, Errno>;
        let msg: T = Err(Errno(-1));
        buf.push_back(&msg).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[1, 0xff, 0xff, 0xff, 0xff],
                files: &[],
            }
        );
        assert_eq!(buf.pop_front::<T>().unwrap(), msg);
        assert!(buf.is_empty());
    }

    #[test]
    fn tuple() {
        let mut buf = IPCBuffer::new();
        let msg = (true, false, false, 0xabcdu16, 999.0f64);
        buf.push_back(&msg).unwrap();
        assert_eq!(
            buf.as_slice(),
            IPCSlice {
                bytes: &[1, 0, 0, 205, 171, 0, 0, 0, 0, 0, 56, 143, 64],
                files: &[],
            }
        );
        assert_eq!(
            buf.pop_front::<(bool, bool, bool, u16, f64)>().unwrap(),
            msg
        );
        assert!(buf.is_empty());
    }
}
