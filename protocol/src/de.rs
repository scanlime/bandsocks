//! Special purpose de-serialization for IPC messages

use super::{
    buffer::{Error, IPCBuffer, Result},
    SysFd,
};
use core::{fmt, fmt::Display, result};
use serde::{de, de::IntoDeserializer};

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
                formatter.write_str("struct SysFD")
            }

            fn visit_u32<E>(self, v: u32) -> result::Result<SysFd, E> {
                Ok(SysFd(v))
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

impl<'d> IPCDeserializer<'d> {
    fn deserialize_sysfd<'a, V: de::Visitor<'d>>(&'a mut self, visitor: V) -> Result<V::Value> {
        let file = self.input.pop_front_file()?;
        visitor.visit_u32(file.0)
    }
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
        struct SeqAccess<'d, 'a> {
            deserializer: &'a mut IPCDeserializer<'d>,
            len: usize,
        }

        impl<'d, 'a> de::SeqAccess<'d> for SeqAccess<'d, 'a> {
            type Error = Error;

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }

            fn next_element_seed<S>(&mut self, seed: S) -> Result<Option<S::Value>>
            where
                S: de::DeserializeSeed<'d>,
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

        visitor.visit_seq(SeqAccess {
            deserializer: self,
            len,
        })
    }

    fn deserialize_tuple_struct<V: de::Visitor<'d>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        if name == SYSFD {
            assert_eq!(len, 1);
            self.deserialize_sysfd(visitor)
        } else {
            self.deserialize_tuple(len, visitor)
        }
    }

    fn deserialize_enum<V: de::Visitor<'d>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_enum(self)
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
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(fields.len(), visitor)
    }
}

impl<'d, 'a> de::VariantAccess<'d> for &'a mut IPCDeserializer<'d> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<V: de::DeserializeSeed<'d>>(self, seed: V) -> Result<V::Value> {
        de::DeserializeSeed::deserialize(seed, self)
    }

    fn tuple_variant<V: de::Visitor<'d>>(self, len: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V: de::Visitor<'d>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self, fields.len(), visitor)
    }
}

impl<'d, 'a> de::EnumAccess<'d> for &'a mut IPCDeserializer<'d> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: de::DeserializeSeed<'d>>(self, seed: V) -> Result<(V::Value, Self)> {
        let variant_index = self.input.pop_front_byte()?;
        let variant = (variant_index as u32).into_deserializer();
        let v = de::DeserializeSeed::deserialize(seed, variant)?;
        Ok((v, self))
    }
}
