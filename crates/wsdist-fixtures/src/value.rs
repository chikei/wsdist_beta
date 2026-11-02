use std::fmt;
use std::ops::{Index, IndexMut};

use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub type Map = indexmap::IndexMap<String, Value>;

pub(crate) const MAX_DEPTH: usize = 128;

/// A number with an explicit integer or binary64 kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    U64(u64),
    I64(i64),
    F64(f64),
}

impl Number {
    pub fn from_f64(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self::F64(value))
    }

    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            Self::U64(value) => Some(value),
            Self::I64(value) => u64::try_from(value).ok(),
            Self::F64(_) => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Self::U64(value) => i64::try_from(value).ok(),
            Self::I64(value) => Some(value),
            Self::F64(_) => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        Some(match *self {
            Self::U64(value) => value as f64,
            Self::I64(value) => value as f64,
            Self::F64(value) => value,
        })
    }

    pub fn is_u64(&self) -> bool {
        self.as_u64().is_some()
    }

    pub fn is_i64(&self) -> bool {
        self.as_i64().is_some()
    }

    pub fn is_f64(&self) -> bool {
        matches!(self, Self::F64(_))
    }
}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::U64(value) => value.fmt(formatter),
            Self::I64(value) => value.fmt(formatter),
            Self::F64(value) => value.fmt(formatter),
        }
    }
}

/// The final, directly decoded fixture representation. Objects preserve insertion order.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map),
}

impl Value {
    pub fn is_null(&self) -> bool { matches!(self, Self::Null) }
    pub fn is_boolean(&self) -> bool { matches!(self, Self::Bool(_)) }
    pub fn is_number(&self) -> bool { matches!(self, Self::Number(_)) }
    pub fn is_string(&self) -> bool { matches!(self, Self::String(_)) }
    pub fn is_array(&self) -> bool { matches!(self, Self::Array(_)) }
    pub fn is_object(&self) -> bool { matches!(self, Self::Object(_)) }
    pub fn is_u64(&self) -> bool { self.as_number().is_some_and(Number::is_u64) }
    pub fn is_i64(&self) -> bool { self.as_number().is_some_and(Number::is_i64) }
    pub fn is_f64(&self) -> bool { self.as_number().is_some_and(Number::is_f64) }

    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(value) = self { Some(*value) } else { None }
    }

    pub fn as_number(&self) -> Option<&Number> {
        if let Self::Number(value) = self { Some(value) } else { None }
    }

    pub fn as_u64(&self) -> Option<u64> { self.as_number()?.as_u64() }
    pub fn as_i64(&self) -> Option<i64> { self.as_number()?.as_i64() }
    pub fn as_f64(&self) -> Option<f64> { self.as_number()?.as_f64() }

    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(value) = self { Some(value) } else { None }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        if let Self::Array(value) = self { Some(value) } else { None }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        if let Self::Array(value) = self { Some(value) } else { None }
    }

    pub fn as_object(&self) -> Option<&Map> {
        if let Self::Object(value) = self { Some(value) } else { None }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Map> {
        if let Self::Object(value) = self { Some(value) } else { None }
    }

    pub fn get<I: NativeIndex>(&self, index: I) -> Option<&Value> {
        index.index_into(self)
    }

    pub fn get_mut<I: NativeIndex>(&mut self, index: I) -> Option<&mut Value> {
        index.index_into_mut(self)
    }

    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

/// The string-key and array-index operations supported by native values.
pub trait NativeIndex {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value>;
    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value>;
    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value;
}

impl NativeIndex for usize {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        value.as_array()?.get(*self)
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        value.as_array_mut()?.get_mut(*self)
    }

    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value {
        self.index_into_mut(value).expect("native fixture array index out of bounds")
    }
}

impl NativeIndex for str {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        value.as_object()?.get(self)
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        value.as_object_mut()?.get_mut(self)
    }

    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value {
        if value.is_null() {
            *value = Value::Object(Map::new());
        }
        let object = value.as_object_mut().expect("cannot index a non-object native fixture with a string");
        let index = match object.get_index_of(self) {
            Some(index) => index,
            None => object.insert_full(self.to_owned(), Value::Null).0,
        };
        object.get_index_mut(index).unwrap().1
    }
}

impl NativeIndex for String {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> { self.as_str().index_into(value) }
    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> { self.as_str().index_into_mut(value) }
    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value { self.as_str().index_or_insert(value) }
}

impl<T: NativeIndex + ?Sized> NativeIndex for &T {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> { (**self).index_into(value) }
    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> { (**self).index_into_mut(value) }
    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value { (**self).index_or_insert(value) }
}

impl<I: NativeIndex> Index<I> for Value {
    type Output = Value;

    fn index(&self, index: I) -> &Self::Output {
        static NULL: Value = Value::Null;
        index.index_into(self).unwrap_or(&NULL)
    }
}

impl<I: NativeIndex> IndexMut<I> for Value {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        index.index_or_insert(self)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if formatter.alternate() {
            formatter.write_str(&serde_json::to_string_pretty(self).map_err(|_| fmt::Error)?)
        } else {
            formatter.write_str(&serde_json::to_string(self).map_err(|_| fmt::Error)?)
        }
    }
}

impl Serialize for Number {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            match *self {
                Self::U64(value) => serializer.serialize_u64(value),
                Self::I64(value) => serializer.serialize_i64(value),
                Self::F64(value) => serializer.serialize_f64(value),
            }
        } else {
            // These indices are the version-1 bitcode schema; never reorder them.
            match self {
                Self::U64(value) => serializer.serialize_newtype_variant("Number", 0, "U64", value),
                Self::I64(value) => serializer.serialize_newtype_variant("Number", 1, "I64", value),
                Self::F64(value) => serializer.serialize_newtype_variant("Number", 2, "F64", value),
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(variant_identifier)]
enum NumberTag { U64, I64, F64 }

struct NumberVisitor;

impl<'de> Visitor<'de> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a number")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Number, E> { Ok(Number::U64(value)) }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Number, E> { Ok(Number::I64(value)) }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Number, E> { Ok(Number::F64(value)) }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Number, A::Error> {
        let (tag, variant) = data.variant()?;
        match tag {
            NumberTag::U64 => variant.newtype_variant().map(Number::U64),
            NumberTag::I64 => variant.newtype_variant().map(Number::I64),
            NumberTag::F64 => variant.newtype_variant().map(Number::F64),
        }
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(NumberVisitor)
        } else {
            deserializer.deserialize_enum("Number", &["U64", "I64", "F64"], NumberVisitor)
        }
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            match self {
                Self::Null => serializer.serialize_unit(),
                Self::Bool(value) => serializer.serialize_bool(*value),
                Self::Number(value) => value.serialize(serializer),
                Self::String(value) => serializer.serialize_str(value),
                Self::Array(value) => value.serialize(serializer),
                Self::Object(value) => value.serialize(serializer),
            }
        } else {
            // Explicit stable indices match ValueTag below.
            match self {
                Self::Null => serializer.serialize_unit_variant("Value", 0, "Null"),
                Self::Bool(value) => serializer.serialize_newtype_variant("Value", 1, "Bool", value),
                Self::Number(value) => serializer.serialize_newtype_variant("Value", 2, "Number", value),
                Self::String(value) => serializer.serialize_newtype_variant("Value", 3, "String", value),
                Self::Array(value) => serializer.serialize_newtype_variant("Value", 4, "Array", value),
                Self::Object(value) => serializer.serialize_newtype_variant("Value", 5, "Object", value),
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(variant_identifier)]
enum ValueTag { Null, Bool, Number, String, Array, Object }

struct ValueSeed { remaining: usize }
struct ValueVisitor { remaining: usize }
struct ArraySeed { remaining: usize }
struct ObjectSeed { remaining: usize }

impl<'de> DeserializeSeed<'de> for ValueSeed {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        let visitor = ValueVisitor { remaining: self.remaining };
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(visitor)
        } else {
            deserializer.deserialize_enum("Value", &["Null", "Bool", "Number", "String", "Array", "Object"], visitor)
        }
    }
}

impl<'de> DeserializeSeed<'de> for ArraySeed {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_seq(ValueVisitor { remaining: self.remaining })
    }
}

impl<'de> DeserializeSeed<'de> for ObjectSeed {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_map(ValueVisitor { remaining: self.remaining })
    }
}

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a native fixture value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> { Ok(Value::Null) }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> { Ok(Value::Null) }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> { Ok(Value::Bool(value)) }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> { Ok(Value::Number(Number::U64(value))) }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> { Ok(Value::Number(Number::I64(value))) }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> { Ok(Value::Number(Number::F64(value))) }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> { Ok(Value::String(value.to_owned())) }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> { Ok(Value::String(value)) }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let remaining = self.remaining.checked_sub(1).ok_or_else(|| de::Error::custom("fixture nesting exceeds 128 containers"))?;
        // A corrupt size hint must not force an unbounded up-front allocation.
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(4096));
        while let Some(value) = sequence.next_element_seed(ValueSeed { remaining })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Value, A::Error> {
        let remaining = self.remaining.checked_sub(1).ok_or_else(|| de::Error::custom("fixture nesting exceeds 128 containers"))?;
        let mut values = Map::with_capacity(object.size_hint().unwrap_or(0).min(4096));
        while let Some(key) = object.next_key::<String>()? {
            let value = object.next_value_seed(ValueSeed { remaining })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Value, A::Error> {
        let (tag, variant) = data.variant()?;
        match tag {
            ValueTag::Null => { variant.unit_variant()?; Ok(Value::Null) }
            ValueTag::Bool => variant.newtype_variant().map(Value::Bool),
            ValueTag::Number => variant.newtype_variant().map(Value::Number),
            ValueTag::String => variant.newtype_variant().map(Value::String),
            ValueTag::Array => variant.newtype_variant_seed(ArraySeed { remaining: self.remaining }),
            ValueTag::Object => variant.newtype_variant_seed(ObjectSeed { remaining: self.remaining }),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
        ValueSeed { remaining: MAX_DEPTH }.deserialize(deserializer)
    }
}

macro_rules! unsigned_from {
    ($($type:ty),* $(,)?) => {$(
        impl From<$type> for Number {
            fn from(value: $type) -> Self { Self::U64(value as u64) }
        }
        impl From<$type> for Value {
            fn from(value: $type) -> Self { Self::Number(Number::from(value)) }
        }
        impl PartialEq<$type> for Value {
            fn eq(&self, other: &$type) -> bool { self.as_u64() == Some(*other as u64) }
        }
        impl PartialEq<Value> for $type {
            fn eq(&self, other: &Value) -> bool { other == self }
        }
    )*};
}

macro_rules! signed_from {
    ($($type:ty),* $(,)?) => {$(
        impl From<$type> for Number {
            fn from(value: $type) -> Self {
                if value < 0 { Self::I64(value as i64) } else { Self::U64(value as u64) }
            }
        }
        impl From<$type> for Value {
            fn from(value: $type) -> Self { Self::Number(Number::from(value)) }
        }
        impl PartialEq<$type> for Value {
            fn eq(&self, other: &$type) -> bool { self.as_i64() == Some(*other as i64) }
        }
        impl PartialEq<Value> for $type {
            fn eq(&self, other: &Value) -> bool { other == self }
        }
    )*};
}

unsigned_from!(u8, u16, u32, u64, usize);
signed_from!(i8, i16, i32, i64, isize);

impl From<f64> for Value {
    fn from(value: f64) -> Self { Number::from_f64(value).map_or(Self::Null, Self::Number) }
}
impl From<f32> for Value {
    fn from(value: f32) -> Self { Self::from(f64::from(value)) }
}
impl From<bool> for Value {
    fn from(value: bool) -> Self { Self::Bool(value) }
}
impl From<String> for Value {
    fn from(value: String) -> Self { Self::String(value) }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self { Self::String(value.to_owned()) }
}
impl From<Number> for Value {
    fn from(value: Number) -> Self { Self::Number(value) }
}
impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self { Self::Array(value) }
}
impl From<Map> for Value {
    fn from(value: Map) -> Self { Self::Object(value) }
}
impl From<()> for Value {
    fn from(_: ()) -> Self { Self::Null }
}
impl<T: Into<Value>> FromIterator<T> for Value {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::Array(values.into_iter().map(Into::into).collect())
    }
}

impl PartialEq<bool> for Value {
    fn eq(&self, other: &bool) -> bool { self.as_bool() == Some(*other) }
}
impl PartialEq<Value> for bool {
    fn eq(&self, other: &Value) -> bool { other == self }
}
impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool { self.as_str() == Some(other) }
}
impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool { self == *other }
}
impl PartialEq<String> for Value {
    fn eq(&self, other: &String) -> bool { self == other.as_str() }
}
impl PartialEq<Value> for str {
    fn eq(&self, other: &Value) -> bool { other == self }
}
impl PartialEq<Value> for &str {
    fn eq(&self, other: &Value) -> bool { other == *self }
}
impl PartialEq<Value> for String {
    fn eq(&self, other: &Value) -> bool { other == self.as_str() }
}
impl PartialEq<f64> for Value {
    fn eq(&self, other: &f64) -> bool { self.as_f64() == Some(*other) }
}
impl PartialEq<Value> for f64 {
    fn eq(&self, other: &Value) -> bool { other == self }
}
impl PartialEq<f32> for Value {
    fn eq(&self, other: &f32) -> bool { self.as_f64() == Some(f64::from(*other)) }
}
impl PartialEq<Value> for f32 {
    fn eq(&self, other: &Value) -> bool { other == self }
}

impl PartialEq<serde_json::Number> for Number {
    fn eq(&self, other: &serde_json::Number) -> bool {
        match self {
            Self::U64(value) => !other.is_f64() && other.as_u64() == Some(*value),
            Self::I64(value) => !other.is_f64() && other.as_i64() == Some(*value),
            Self::F64(value) => other.is_f64() && other.as_f64() == Some(*value),
        }
    }
}
impl PartialEq<Number> for serde_json::Number {
    fn eq(&self, other: &Number) -> bool { other == self }
}

impl PartialEq<serde_json::Value> for Value {
    fn eq(&self, other: &serde_json::Value) -> bool {
        match (self, other) {
            (Self::Null, serde_json::Value::Null) => true,
            (Self::Bool(left), serde_json::Value::Bool(right)) => left == right,
            (Self::Number(left), serde_json::Value::Number(right)) => left == right,
            (Self::String(left), serde_json::Value::String(right)) => left == right,
            (Self::Array(left), serde_json::Value::Array(right)) => left.len() == right.len() && left.iter().zip(right).all(|(left, right)| left == right),
            (Self::Object(left), serde_json::Value::Object(right)) => left.len() == right.len() && left.iter().all(|(key, value)| right.get(key).is_some_and(|other| value == other)),
            _ => false,
        }
    }
}
impl PartialEq<Value> for serde_json::Value {
    fn eq(&self, other: &Value) -> bool { other == self }
}
