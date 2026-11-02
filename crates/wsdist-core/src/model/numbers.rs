use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyF64(pub f64);

impl Serialize for LegacyF64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.is_finite() { return serializer.serialize_f64(self.0); }
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("nonfinite", if self.0.is_nan() { "NaN" } else if self.0.is_sign_positive() { "+Infinity" } else { "-Infinity" })?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for LegacyF64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NumberVisitor;
        impl<'de> Visitor<'de> for NumberVisitor {
            type Value = LegacyF64;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result { formatter.write_str("a number or an explicit nonfinite tag") }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> { Ok(LegacyF64(value)) }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> { Ok(LegacyF64(value as f64)) }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> { Ok(LegacyF64(value as f64)) }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let key = map.next_key::<String>()?.ok_or_else(|| serde::de::Error::custom("empty nonfinite tag"))?;
                if key != "nonfinite" { return Err(serde::de::Error::custom(format!("unknown number tag {key:?}"))); }
                let value = match map.next_value::<String>()?.as_str() {
                    "NaN" => f64::NAN, "+Infinity" => f64::INFINITY, "-Infinity" => f64::NEG_INFINITY,
                    value => return Err(serde::de::Error::custom(format!("unknown nonfinite value {value:?}"))),
                };
                if map.next_key::<String>()?.is_some() { return Err(serde::de::Error::custom("nonfinite tag must have exactly one field")); }
                Ok(LegacyF64(value))
            }
        }
        deserializer.deserialize_any(NumberVisitor)
    }
}

pub fn serialize_legacy_f64<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> { LegacyF64(*value).serialize(serializer) }

pub fn serialize_optional_legacy_f64<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    value.map(LegacyF64).serialize(serializer)
}

pub fn serialize_legacy_f64_slice<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for &value in values { sequence.serialize_element(&LegacyF64(value))?; }
    sequence.end()
}

pub fn serialize_legacy_series<S: Serializer>(values: &[[f64; 2]], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for &[time, value] in values { sequence.serialize_element(&[LegacyF64(time), LegacyF64(value)])?; }
    sequence.end()
}
