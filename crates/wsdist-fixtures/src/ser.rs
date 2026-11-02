use std::fmt;

use serde::ser::{
    self, Impossible, Serialize, SerializeMap, SerializeSeq, SerializeStruct,
    SerializeStructVariant, SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};

use crate::value::MAX_DEPTH;
use crate::{Map, Number, Value, ValueError};

#[derive(Clone, Copy)]
pub(crate) struct Serializer { remaining: usize }

impl Serializer {
    pub(crate) fn new() -> Self { Self { remaining: MAX_DEPTH } }

    fn child(self) -> Result<Self, ValueError> {
        Ok(Self { remaining: self.remaining.checked_sub(1).ok_or_else(||
            <ValueError as ser::Error>::custom("fixture nesting exceeds 128 containers")
        )? })
    }
}

impl ser::Serializer for Serializer {
    type Ok = Value;
    type Error = ValueError;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = Sequence;
    type SerializeTupleVariant = TupleVariant;
    type SerializeMap = Object;
    type SerializeStruct = Object;
    type SerializeStructVariant = StructVariant;

    fn serialize_bool(self, value: bool) -> Result<Value, ValueError> { Ok(Value::Bool(value)) }
    fn serialize_i8(self, value: i8) -> Result<Value, ValueError> { self.serialize_i64(i64::from(value)) }
    fn serialize_i16(self, value: i16) -> Result<Value, ValueError> { self.serialize_i64(i64::from(value)) }
    fn serialize_i32(self, value: i32) -> Result<Value, ValueError> { self.serialize_i64(i64::from(value)) }
    fn serialize_i64(self, value: i64) -> Result<Value, ValueError> { Ok(Value::from(value)) }
    fn serialize_i128(self, value: i128) -> Result<Value, ValueError> {
        if let Ok(value) = u64::try_from(value) {
            self.serialize_u64(value)
        } else if let Ok(value) = i64::try_from(value) {
            self.serialize_i64(value)
        } else {
            Err(ser::Error::custom("integer outside the native JSON number range"))
        }
    }
    fn serialize_u8(self, value: u8) -> Result<Value, ValueError> { self.serialize_u64(u64::from(value)) }
    fn serialize_u16(self, value: u16) -> Result<Value, ValueError> { self.serialize_u64(u64::from(value)) }
    fn serialize_u32(self, value: u32) -> Result<Value, ValueError> { self.serialize_u64(u64::from(value)) }
    fn serialize_u64(self, value: u64) -> Result<Value, ValueError> { Ok(Value::Number(Number::U64(value))) }
    fn serialize_u128(self, value: u128) -> Result<Value, ValueError> {
        self.serialize_u64(u64::try_from(value).map_err(|_| <ValueError as ser::Error>::custom("integer outside the native JSON number range"))?)
    }
    fn serialize_f32(self, value: f32) -> Result<Value, ValueError> { self.serialize_f64(f64::from(value)) }
    fn serialize_f64(self, value: f64) -> Result<Value, ValueError> { Ok(Value::from(value)) }
    fn serialize_char(self, value: char) -> Result<Value, ValueError> { Ok(Value::String(value.to_string())) }
    fn serialize_str(self, value: &str) -> Result<Value, ValueError> { Ok(Value::String(value.to_owned())) }
    fn serialize_bytes(self, value: &[u8]) -> Result<Value, ValueError> {
        let _ = self.child()?;
        Ok(Value::Array(value.iter().map(|value| Value::from(*value)).collect()))
    }
    fn serialize_none(self) -> Result<Value, ValueError> { Ok(Value::Null) }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value, ValueError> { value.serialize(self) }
    fn serialize_unit(self) -> Result<Value, ValueError> { Ok(Value::Null) }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value, ValueError> { Ok(Value::Null) }
    fn serialize_unit_variant(self, _name: &'static str, _index: u32, variant: &'static str) -> Result<Value, ValueError> {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _name: &'static str, value: &T) -> Result<Value, ValueError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self, _name: &'static str, _index: u32, variant: &'static str, value: &T,
    ) -> Result<Value, ValueError> {
        let mut values = Map::with_capacity(1);
        values.insert(variant.to_owned(), value.serialize(self.child()?)?);
        Ok(Value::Object(values))
    }
    fn serialize_seq(self, length: Option<usize>) -> Result<Sequence, ValueError> {
        let serializer = self.child()?;
        let mut values = Vec::new();
        if let Some(length) = length {
            values.try_reserve_exact(length).map_err(<ValueError as ser::Error>::custom)?;
        }
        Ok(Sequence { values, serializer })
    }
    fn serialize_tuple(self, length: usize) -> Result<Sequence, ValueError> { self.serialize_seq(Some(length)) }
    fn serialize_tuple_struct(self, _name: &'static str, length: usize) -> Result<Sequence, ValueError> { self.serialize_seq(Some(length)) }
    fn serialize_tuple_variant(
        self, _name: &'static str, _index: u32, variant: &'static str, length: usize,
    ) -> Result<TupleVariant, ValueError> {
        Ok(TupleVariant { variant, sequence: self.child()?.serialize_seq(Some(length))? })
    }
    fn serialize_map(self, length: Option<usize>) -> Result<Object, ValueError> {
        let serializer = self.child()?;
        let mut values = Map::new();
        if let Some(length) = length {
            values.try_reserve(length).map_err(<ValueError as ser::Error>::custom)?;
        }
        Ok(Object { values, pending_key: None, serializer })
    }
    fn serialize_struct(self, _name: &'static str, length: usize) -> Result<Object, ValueError> { self.serialize_map(Some(length)) }
    fn serialize_struct_variant(
        self, _name: &'static str, _index: u32, variant: &'static str, length: usize,
    ) -> Result<StructVariant, ValueError> {
        Ok(StructVariant { variant, object: self.child()?.serialize_map(Some(length))? })
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<Value, ValueError> {
        Ok(Value::String(value.to_string()))
    }
}

pub(crate) struct Sequence { values: Vec<Value>, serializer: Serializer }

impl SerializeSeq for Sequence {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueError> {
        self.values.push(value.serialize(self.serializer)?);
        Ok(())
    }

    fn end(self) -> Result<Value, ValueError> { Ok(Value::Array(self.values)) }
}

impl SerializeTuple for Sequence {
    type Ok = Value;
    type Error = ValueError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueError> { SerializeSeq::serialize_element(self, value) }
    fn end(self) -> Result<Value, ValueError> { SerializeSeq::end(self) }
}
impl SerializeTupleStruct for Sequence {
    type Ok = Value;
    type Error = ValueError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueError> { SerializeSeq::serialize_element(self, value) }
    fn end(self) -> Result<Value, ValueError> { SerializeSeq::end(self) }
}

pub(crate) struct Object {
    values: Map,
    pending_key: Option<String>,
    serializer: Serializer,
}

impl SerializeMap for Object {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), ValueError> {
        if self.pending_key.is_some() {
            return Err(ser::Error::custom("native object key without a preceding value"));
        }
        self.pending_key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueError> {
        let key = self.pending_key.take().ok_or_else(|| <ValueError as ser::Error>::custom("native object value without a key"))?;
        self.values.insert(key, value.serialize(self.serializer)?);
        Ok(())
    }

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(&mut self, key: &K, value: &V) -> Result<(), ValueError> {
        if self.pending_key.is_some() {
            return Err(ser::Error::custom("native object key without a preceding value"));
        }
        self.values.insert(key.serialize(KeySerializer)?, value.serialize(self.serializer)?);
        Ok(())
    }

    fn end(self) -> Result<Value, ValueError> {
        if self.pending_key.is_some() { return Err(ser::Error::custom("native object key without a value")); }
        Ok(Value::Object(self.values))
    }
}

impl SerializeStruct for Object {
    type Ok = Value;
    type Error = ValueError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), ValueError> {
        self.values.insert(key.to_owned(), value.serialize(self.serializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value, ValueError> { SerializeMap::end(self) }
}

pub(crate) struct TupleVariant { variant: &'static str, sequence: Sequence }

impl SerializeTupleVariant for TupleVariant {
    type Ok = Value;
    type Error = ValueError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueError> { SerializeSeq::serialize_element(&mut self.sequence, value) }
    fn end(self) -> Result<Value, ValueError> {
        let mut values = Map::with_capacity(1);
        values.insert(self.variant.to_owned(), SerializeSeq::end(self.sequence)?);
        Ok(Value::Object(values))
    }
}

pub(crate) struct StructVariant { variant: &'static str, object: Object }

impl SerializeStructVariant for StructVariant {
    type Ok = Value;
    type Error = ValueError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), ValueError> { SerializeStruct::serialize_field(&mut self.object, key, value) }
    fn end(self) -> Result<Value, ValueError> {
        let mut values = Map::with_capacity(1);
        values.insert(self.variant.to_owned(), SerializeMap::end(self.object)?);
        Ok(Value::Object(values))
    }
}

struct KeySerializer;

fn invalid_key() -> ValueError { ser::Error::custom("native object keys must be strings or scalar JSON keys") }

macro_rules! key_integer {
    ($($method:ident : $type:ty),* $(,)?) => {$(
        fn $method(self, value: $type) -> Result<String, ValueError> { Ok(value.to_string()) }
    )*};
}

impl ser::Serializer for KeySerializer {
    type Ok = String;
    type Error = ValueError;
    type SerializeSeq = Impossible<String, ValueError>;
    type SerializeTuple = Impossible<String, ValueError>;
    type SerializeTupleStruct = Impossible<String, ValueError>;
    type SerializeTupleVariant = Impossible<String, ValueError>;
    type SerializeMap = Impossible<String, ValueError>;
    type SerializeStruct = Impossible<String, ValueError>;
    type SerializeStructVariant = Impossible<String, ValueError>;

    fn serialize_str(self, value: &str) -> Result<String, ValueError> { Ok(value.to_owned()) }
    fn serialize_char(self, value: char) -> Result<String, ValueError> { Ok(value.to_string()) }
    fn serialize_bool(self, value: bool) -> Result<String, ValueError> { Ok(value.to_string()) }
    key_integer! {
        serialize_i8: i8, serialize_i16: i16, serialize_i32: i32,
        serialize_i64: i64, serialize_i128: i128, serialize_u8: u8,
        serialize_u16: u16, serialize_u32: u32, serialize_u64: u64, serialize_u128: u128,
    }
    fn serialize_f32(self, value: f32) -> Result<String, ValueError> {
        if !value.is_finite() { return Err(invalid_key()); }
        serde_json::to_string(&value).map_err(<ValueError as ser::Error>::custom)
    }
    fn serialize_f64(self, value: f64) -> Result<String, ValueError> {
        if !value.is_finite() { return Err(invalid_key()); }
        serde_json::to_string(&value).map_err(<ValueError as ser::Error>::custom)
    }
    fn serialize_bytes(self, _value: &[u8]) -> Result<String, ValueError> { Err(invalid_key()) }
    fn serialize_none(self) -> Result<String, ValueError> { Err(invalid_key()) }
    fn serialize_some<T: Serialize + ?Sized>(self, _value: &T) -> Result<String, ValueError> { Err(invalid_key()) }
    fn serialize_unit(self) -> Result<String, ValueError> { Err(invalid_key()) }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<String, ValueError> { Err(invalid_key()) }
    fn serialize_unit_variant(self, _name: &'static str, _index: u32, variant: &'static str) -> Result<String, ValueError> { Ok(variant.to_owned()) }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _name: &'static str, value: &T) -> Result<String, ValueError> { value.serialize(self) }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self, _name: &'static str, _index: u32, _variant: &'static str, _value: &T,
    ) -> Result<String, ValueError> { Err(invalid_key()) }
    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq, ValueError> { Err(invalid_key()) }
    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple, ValueError> { Err(invalid_key()) }
    fn serialize_tuple_struct(self, _name: &'static str, _length: usize) -> Result<Self::SerializeTupleStruct, ValueError> { Err(invalid_key()) }
    fn serialize_tuple_variant(
        self, _name: &'static str, _index: u32, _variant: &'static str, _length: usize,
    ) -> Result<Self::SerializeTupleVariant, ValueError> { Err(invalid_key()) }
    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap, ValueError> { Err(invalid_key()) }
    fn serialize_struct(self, _name: &'static str, _length: usize) -> Result<Self::SerializeStruct, ValueError> { Err(invalid_key()) }
    fn serialize_struct_variant(
        self, _name: &'static str, _index: u32, _variant: &'static str, _length: usize,
    ) -> Result<Self::SerializeStructVariant, ValueError> { Err(invalid_key()) }
    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<String, ValueError> { Ok(value.to_string()) }
}
