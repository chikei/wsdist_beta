use std::slice;

use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use serde::Deserializer;

use crate::{Number, Value, ValueError};

impl<'de> IntoDeserializer<'de, ValueError> for &'de Value {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self { self }
}

impl<'de> Deserializer<'de> for &'de Value {
    type Error = ValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(Number::U64(value)) => visitor.visit_u64(*value),
            Value::Number(Number::I64(value)) => visitor.visit_i64(*value),
            Value::Number(Number::F64(value)) => visitor.visit_f64(*value),
            Value::String(value) => visitor.visit_borrowed_str(value),
            Value::Array(values) => {
                let mut sequence = Sequence { values: values.iter() };
                let result = visitor.visit_seq(&mut sequence)?;
                if sequence.values.len() != 0 {
                    return Err(de::Error::custom("unconsumed native fixture array elements"));
                }
                Ok(result)
            }
            Value::Object(values) => {
                let mut object = Object { values: values.iter(), pending: None };
                let result = visitor.visit_map(&mut object)?;
                if object.pending.is_some() || object.values.len() != 0 {
                    return Err(de::Error::custom("unconsumed native fixture object entries"));
                }
                Ok(result)
            }
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        if self.is_null() { visitor.visit_none() } else { visitor.visit_some(self) }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self, _name: &'static str, visitor: V,
    ) -> Result<V::Value, ValueError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self, _name: &'static str, _variants: &'static [&'static str], visitor: V,
    ) -> Result<V::Value, ValueError> {
        match self {
            Value::String(name) => visitor.visit_enum(NativeEnum { name, value: None }),
            Value::Object(values) if values.len() == 1 => {
                let (name, value) = values.first().unwrap();
                visitor.visit_enum(NativeEnum { name, value: Some(value) })
            }
            _ => Err(de::Error::custom("expected a string or a single-key object for an enum")),
        }
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        match self {
            Value::String(value) => visitor.visit_borrowed_bytes(value.as_bytes()),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        // The complete source has already been validated into the native tree.
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

struct Sequence<'de> { values: slice::Iter<'de, Value> }

impl<'de> SeqAccess<'de> for &mut Sequence<'de> {
    type Error = ValueError;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, ValueError> {
        self.values.next().map(|value| seed.deserialize(value)).transpose()
    }

    fn size_hint(&self) -> Option<usize> { Some(self.values.len()) }
}

struct Object<'de> {
    values: indexmap::map::Iter<'de, String, Value>,
    pending: Option<&'de Value>,
}

impl<'de> MapAccess<'de> for &mut Object<'de> {
    type Error = ValueError;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, ValueError> {
        if self.pending.is_some() {
            return Err(de::Error::custom("native fixture map value requested out of order"));
        }
        match self.values.next() {
            Some((key, value)) => {
                self.pending = Some(value);
                seed.deserialize(Key(key)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, ValueError> {
        let value = self.pending.take().ok_or_else(|| <ValueError as de::Error>::custom("native fixture map value without key"))?;
        seed.deserialize(value)
    }

    fn size_hint(&self) -> Option<usize> { Some(self.values.len()) }
}

struct NativeEnum<'de> { name: &'de str, value: Option<&'de Value> }

impl<'de> EnumAccess<'de> for NativeEnum<'de> {
    type Error = ValueError;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self), ValueError> {
        let variant = seed.deserialize(de::value::BorrowedStrDeserializer::<ValueError>::new(self.name))?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for NativeEnum<'de> {
    type Error = ValueError;

    fn unit_variant(self) -> Result<(), ValueError> {
        match self.value {
            None | Some(Value::Null) => Ok(()),
            Some(_) => Err(de::Error::custom("expected null for a unit enum variant")),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, ValueError> {
        let value = self.value.ok_or_else(|| <ValueError as de::Error>::custom("missing enum variant value"))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _length: usize, visitor: V) -> Result<V::Value, ValueError> {
        let value = self.value.ok_or_else(|| <ValueError as de::Error>::custom("missing tuple variant value"))?;
        if !value.is_array() {
            return Err(de::Error::custom("expected an array for a tuple enum variant"));
        }
        value.deserialize_any(visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self, _fields: &'static [&'static str], visitor: V,
    ) -> Result<V::Value, ValueError> {
        let value = self.value.ok_or_else(|| <ValueError as de::Error>::custom("missing struct variant value"))?;
        if !value.is_object() {
            return Err(de::Error::custom("expected an object for a struct enum variant"));
        }
        value.deserialize_any(visitor)
    }
}

struct Key<'de>(&'de str);

macro_rules! key_number {
    ($($method:ident => $parse:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
            // Match JSON's numeric-key grammar without constructing a JSON value.
            // `end` accepts whitespace, while map keys must consume every byte.
            if !matches!(self.0.as_bytes().first(), Some(b'0'..=b'9' | b'-'))
                || !self.0.as_bytes().last().is_some_and(|byte| byte.is_ascii_digit())
            {
                return Err(de::Error::custom("invalid native fixture numeric object key"));
            }
            let mut parser = serde_json::Deserializer::from_str(self.0);
            let value = parser.$parse(visitor).map_err(de::Error::custom)?;
            parser.end().map_err(de::Error::custom)?;
            Ok(value)
        }
    )*};
}

impl<'de> Deserializer<'de> for Key<'de> {
    type Error = ValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        visitor.visit_borrowed_str(self.0)
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        match self.0 {
            "true" => visitor.visit_bool(true),
            "false" => visitor.visit_bool(false),
            _ => Err(de::Error::custom("invalid native fixture boolean object key")),
        }
    }

    key_number! {
        deserialize_i8 => deserialize_any,
        deserialize_i16 => deserialize_any,
        deserialize_i32 => deserialize_any,
        deserialize_i64 => deserialize_any,
        deserialize_i128 => deserialize_i128,
        deserialize_u8 => deserialize_any,
        deserialize_u16 => deserialize_any,
        deserialize_u32 => deserialize_any,
        deserialize_u64 => deserialize_any,
        deserialize_u128 => deserialize_u128,
        deserialize_f32 => deserialize_f32,
        deserialize_f64 => deserialize_any,
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ValueError> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self, _name: &'static str, visitor: V,
    ) -> Result<V::Value, ValueError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self, _name: &'static str, _variants: &'static [&'static str], visitor: V,
    ) -> Result<V::Value, ValueError> {
        visitor.visit_enum(de::value::BorrowedStrDeserializer::<ValueError>::new(self.0))
    }

    serde::forward_to_deserialize_any! {
        char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map
        struct identifier ignored_any
    }
}
