//! Special purpose serialization for IPC messages

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
