use std::sync::Arc;

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::model::{CalcError, LegacyKey, LegacyTruth, RandomSource, RandomStream};

/// A complete scalar random protocol record. Uniform parameters retain their
/// binary64 bits, including signed zero and explicitly tagged nonfinites.
#[derive(Clone, Copy, Debug)]
pub enum TapeRecord {
    Uniform { stream: RandomStream, low: f64, high: f64, value: f64 },
    Bounded { stream: RandomStream, upper_exclusive: usize, value: usize },
    Reseed { stream: RandomStream, seed: u64 },
}

impl TapeRecord {
    pub const fn uniform(stream: RandomStream, low: f64, high: f64, value: f64) -> Self {
        Self::Uniform { stream, low, high, value }
    }

    pub const fn bounded(stream: RandomStream, upper_exclusive: usize, value: usize) -> Self {
        Self::Bounded { stream, upper_exclusive, value }
    }

    pub const fn reseed(stream: RandomStream, seed: u64) -> Self {
        Self::Reseed { stream, seed }
    }

}

/// Strict replay with immutable shared evidence and a clone-local cursor.
/// Failed operations do not advance the cursor; there is no production fallback.
#[derive(Clone, Debug)]
pub struct ScriptedTape {
    records: Arc<[TapeRecord]>,
    cursor: usize,
}

impl ScriptedTape {
    pub fn new(records: Vec<TapeRecord>) -> Self {
        Self::from_records(records.into())
    }

    pub fn from_records(records: Arc<[TapeRecord]>) -> Self {
        Self { records, cursor: 0 }
    }


    pub fn position(&self) -> usize { self.cursor }

    fn record(&self) -> Result<TapeRecord, CalcError> {
        self.records.get(self.cursor).copied().ok_or_else(|| record_error(self.cursor, "record", "tape exhausted"))
    }

    #[inline]
    pub fn try_uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> Result<f64, CalcError> {
        let expected = self.record()?;
        if let TapeRecord::Uniform { stream: recorded_stream, low: recorded_low, high: recorded_high, value } = expected {
            if stream == recorded_stream && low.to_bits() == recorded_low.to_bits() && high.to_bits() == recorded_high.to_bits() {
                self.cursor += 1;
                return Ok(value);
            }
        }
        Err(record_error(self.cursor, "uniform", format!(
            "expected {expected:?}; got {stream:?} uniform with low bits {:#018x}, high bits {:#018x}", low.to_bits(), high.to_bits(),
        )))
    }

    #[inline]
    pub fn try_bounded(&mut self, stream: RandomStream, upper_exclusive: usize) -> Result<usize, CalcError> {
        let expected = self.record()?;
        if let TapeRecord::Bounded { stream: recorded_stream, upper_exclusive: recorded_upper, value } = expected {
            if stream == recorded_stream && upper_exclusive == recorded_upper && value < upper_exclusive {
                self.cursor += 1;
                return Ok(value);
            }
        }
        Err(record_error(self.cursor, "bounded", format!("expected {expected:?}; got {stream:?} bounded({upper_exclusive})")))
    }

    pub fn try_reseed(&mut self, stream: RandomStream, seed: u64) -> Result<(), CalcError> {
        let expected = self.record()?;
        if let TapeRecord::Reseed { stream: recorded_stream, seed: recorded_seed } = expected {
            if stream == recorded_stream && seed == recorded_seed {
                self.cursor += 1;
                return Ok(());
            }
        }
        Err(record_error(self.cursor, "reseed", format!("expected {expected:?}; got {stream:?} reseed({seed})")))
    }

    pub fn assert_finished(&self) -> Result<(), CalcError> {
        if self.cursor == self.records.len() { return Ok(()); }
        Err(record_error(self.cursor, "records", format!("{} unused records remain", self.records.len() - self.cursor)))
    }
}

impl RandomSource for ScriptedTape {
    #[inline]
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        self.try_uniform(stream, low, high).unwrap_or_else(|error| panic!("{error}"))
    }

    #[inline]
    fn bounded(&mut self, upper_exclusive: usize) -> usize {
        self.try_bounded(RandomStream::Primary, upper_exclusive).unwrap_or_else(|error| panic!("{error}"))
    }

    fn reseed(&mut self, seed: u64) {
        self.try_reseed(RandomStream::Primary, seed).unwrap_or_else(|error| panic!("{error}"));
    }
}

fn record_error(index: usize, field: &'static str, message: impl Into<String>) -> CalcError {
    CalcError::Input { operation: "rng.tape", field, message: format!("record {index}: {}", message.into()) }
}

#[derive(Debug)]
enum TapeText {
    Primary, Pdif, Uniform, Bounded, Reseed, Nan, PositiveInfinity, NegativeInfinity,
    Other(Box<str>),
}

impl TapeText {
    fn from_str(value: &str) -> Self {
        match value {
            "NumpyHost" => Self::Primary, "NumbaPython" => Self::Pdif,
            "uniform" => Self::Uniform, "bounded" => Self::Bounded, "reseed" => Self::Reseed,
            "NaN" => Self::Nan, "+Infinity" => Self::PositiveInfinity, "-Infinity" => Self::NegativeInfinity,
            value => Self::Other(value.into()),
        }
    }
    fn as_str(&self) -> &str {
        match self {
            Self::Primary => "NumpyHost", Self::Pdif => "NumbaPython", Self::Uniform => "uniform",
            Self::Bounded => "bounded", Self::Reseed => "reseed", Self::Nan => "NaN",
            Self::PositiveInfinity => "+Infinity", Self::NegativeInfinity => "-Infinity",
            Self::Other(value) => value,
        }
    }
}

#[derive(Debug)]
enum TapeScalar {
    Null, Bool, U64(u64), I64(i64), F64(f64), Text(TapeText), Nonfinite(f64), Other,
}

impl TapeScalar {
    fn text(&self) -> Option<&str> {
        if let Self::Text(value) = self { Some(value.as_str()) } else { None }
    }
}

impl<'de> Deserialize<'de> for TapeScalar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ScalarVisitor;
        impl<'de> Visitor<'de> for ScalarVisitor {
            type Value = TapeScalar;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("a captured random scalar") }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> { Ok(TapeScalar::Null) }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> { Ok(TapeScalar::Bool) }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> { Ok(TapeScalar::U64(value)) }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> { Ok(TapeScalar::I64(value)) }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                if !value.is_finite() { return Err(E::custom("number out of range")); }
                Ok(TapeScalar::F64(value))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> { Ok(TapeScalar::Text(TapeText::from_str(value))) }
            fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<Self::Value, A::Error> {
                while values.next_element::<LegacyTruth>()?.is_some() {}
                Ok(TapeScalar::Other)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut tag = None;
                let mut other = false;
                while let Some(key) = map.next_key::<TapeKey>()? {
                    if matches!(key, TapeKey::Nonfinite) {
                        tag = Some(match map.next_value::<TapeScalar>()? {
                            TapeScalar::Text(TapeText::Nan) => Some(f64::NAN),
                            TapeScalar::Text(TapeText::PositiveInfinity) => Some(f64::INFINITY),
                            TapeScalar::Text(TapeText::NegativeInfinity) => Some(f64::NEG_INFINITY),
                            _ => None,
                        });
                    } else {
                        map.next_value::<LegacyTruth>()?;
                        other = true;
                    }
                }
                Ok(match (other, tag) {
                    (false, Some(Some(value))) => TapeScalar::Nonfinite(value),
                    _ => TapeScalar::Other,
                })
            }
        }
        deserializer.deserialize_any(ScalarVisitor)
    }
}

enum TapeKey { Stream, Operation, Parameters, Low, High, Upper, Seed, Value, Nonfinite, Other }

impl<'de> Deserialize<'de> for TapeKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;
        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = TapeKey;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("a random record field") }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(match value {
                    "stream" => TapeKey::Stream, "operation" => TapeKey::Operation, "parameters" => TapeKey::Parameters,
                    "low" => TapeKey::Low, "high" => TapeKey::High, "upper_exclusive" => TapeKey::Upper,
                    "seed" => TapeKey::Seed, "value" => TapeKey::Value, "nonfinite" => TapeKey::Nonfinite,
                    _ => TapeKey::Other,
                })
            }
        }
        deserializer.deserialize_identifier(KeyVisitor)
    }
}

enum Parameters {
    Array { values: [TapeScalar; 2], len: usize },
    Other,
}

impl<'de> Deserialize<'de> for Parameters {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ParametersVisitor;
        impl<'de> Visitor<'de> for ParametersVisitor {
            type Value = Parameters;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("captured random parameters") }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> { Ok(Parameters::Other) }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> { Ok(Parameters::Other) }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> { Ok(Parameters::Other) }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> { Ok(Parameters::Other) }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                if !value.is_finite() { return Err(E::custom("number out of range")); }
                Ok(Parameters::Other)
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> { Ok(Parameters::Other) }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                while map.next_key::<LegacyKey>()?.is_some() { map.next_value::<LegacyTruth>()?; }
                Ok(Parameters::Other)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut values = [TapeScalar::Null, TapeScalar::Null];
                let mut len = 0;
                while let Some(value) = input.next_element::<TapeScalar>()? {
                    if len < values.len() { values[len] = value; }
                    len += 1;
                }
                Ok(Parameters::Array { values, len })
            }
        }
        deserializer.deserialize_any(ParametersVisitor)
    }
}

#[derive(Default)]
struct CapturedRecord {
    stream: Option<TapeScalar>,
    operation: Option<TapeScalar>,
    parameters: Option<Parameters>,
    low: Option<TapeScalar>,
    high: Option<TapeScalar>,
    upper: Option<TapeScalar>,
    seed: Option<TapeScalar>,
    value: Option<TapeScalar>,
}

impl<'de> Deserialize<'de> for CapturedRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RecordVisitor;
        impl<'de> Visitor<'de> for RecordVisitor {
            type Value = CapturedRecord;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("a captured random record object") }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut record = CapturedRecord::default();
                while let Some(key) = map.next_key::<TapeKey>()? {
                    match key {
                        TapeKey::Stream => record.stream = Some(map.next_value()?),
                        TapeKey::Operation => record.operation = Some(map.next_value()?),
                        TapeKey::Parameters => record.parameters = Some(map.next_value()?),
                        TapeKey::Low => record.low = Some(map.next_value()?),
                        TapeKey::High => record.high = Some(map.next_value()?),
                        TapeKey::Upper => record.upper = Some(map.next_value()?),
                        TapeKey::Seed => record.seed = Some(map.next_value()?),
                        TapeKey::Value => record.value = Some(map.next_value()?),
                        TapeKey::Nonfinite | TapeKey::Other => { map.next_value::<LegacyTruth>()?; }
                    }
                }
                Ok(record)
            }
        }
        deserializer.deserialize_map(RecordVisitor)
    }
}

fn required<'a>(value: &'a Option<TapeScalar>, index: usize, name: &'static str) -> Result<&'a TapeScalar, CalcError> {
    value.as_ref().ok_or_else(|| record_error(index, name, "missing field"))
}

fn parameter<'a>(
    record: &'a CapturedRecord, direct: &'a Option<TapeScalar>, index: usize,
    position: usize, arity: usize, name: &'static str,
) -> Result<&'a TapeScalar, CalcError> {
    if let Some(parameters) = &record.parameters {
        if direct.is_some() { return Err(record_error(index, name, "parameter specified twice")); }
        let Parameters::Array { values, len } = parameters else {
            return Err(record_error(index, "parameters", "expected an array"));
        };
        if *len != arity {
            return Err(record_error(index, "parameters", format!("expected {arity} parameters, got {len}")));
        }
        Ok(&values[position])
    } else {
        required(direct, index, name)
    }
}

fn number(value: &TapeScalar, index: usize, field: &'static str) -> Result<f64, CalcError> {
    match value {
        TapeScalar::U64(value) => Ok(*value as f64), TapeScalar::I64(value) => Ok(*value as f64),
        TapeScalar::F64(value) | TapeScalar::Nonfinite(value) => Ok(*value),
        _ => Err(record_error(index, field, "expected a number or an explicit nonfinite tag")),
    }
}

fn unsigned(value: &TapeScalar) -> Option<u64> {
    match value {
        TapeScalar::U64(value) => Some(*value),
        TapeScalar::I64(value) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn integer(value: &TapeScalar, index: usize, field: &'static str) -> Result<usize, CalcError> {
    unsigned(value).and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| record_error(index, field, "expected a representable nonnegative integer"))
}

struct TapeRecordSeed(usize);

impl<'de> DeserializeSeed<'de> for TapeRecordSeed {
    type Value = TapeRecord;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let index = self.0;
        let record = CapturedRecord::deserialize(deserializer)
            .map_err(|error| <D::Error as serde::de::Error>::custom(record_error(index, "record", error.to_string())))?;
        let decode = || -> Result<TapeRecord, CalcError> {
            let stream = match required(&record.stream, index, "stream")? {
                TapeScalar::Text(TapeText::Primary) => RandomStream::Primary,
                TapeScalar::Text(TapeText::Pdif) => RandomStream::Pdif,
                other => return Err(record_error(index, "stream", format!("unsupported stream {:?}", other.text()))),
            };
            match required(&record.operation, index, "operation")? {
                TapeScalar::Text(TapeText::Uniform) => Ok(TapeRecord::uniform(
                    stream,
                    number(parameter(&record, &record.low, index, 0, 2, "low")?, index, "low")?,
                    number(parameter(&record, &record.high, index, 1, 2, "high")?, index, "high")?,
                    number(required(&record.value, index, "value")?, index, "value")?,
                )),
                TapeScalar::Text(TapeText::Bounded) => {
                    let upper = integer(parameter(&record, &record.upper, index, 0, 1, "upper_exclusive")?, index, "upper_exclusive")?;
                    let value = integer(required(&record.value, index, "value")?, index, "value")?;
                    if upper == 0 || value >= upper {
                        return Err(record_error(index, "value", "bounded result is outside its nonempty exclusive range"));
                    }
                    Ok(TapeRecord::bounded(stream, upper, value))
                }
                TapeScalar::Text(TapeText::Reseed) => {
                    let seed = unsigned(parameter(&record, &record.seed, index, 0, 1, "seed")?)
                        .ok_or_else(|| record_error(index, "seed", "expected a nonnegative u64 integer"))?;
                    if record.value.as_ref().is_some_and(|value| !matches!(value, TapeScalar::Null)) {
                        return Err(record_error(index, "value", "reseed has no return value"));
                    }
                    Ok(TapeRecord::reseed(stream, seed))
                }
                other => Err(record_error(index, "operation", format!("unsupported operation {:?}", other.text()))),
            }
        };
        decode().map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for TapeRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TapeRecordSeed(0).deserialize(deserializer)
    }
}

impl<'de> Deserialize<'de> for ScriptedTape {
    /// Decode captured draw arrays directly into typed records, outside hit
    /// loops. Both positional and named parameters are accepted and validated.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TapeVisitor;
        impl<'de> Visitor<'de> for TapeVisitor {
            type Value = ScriptedTape;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("a captured random draw array") }
            fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut records = Vec::with_capacity(input.size_hint().unwrap_or(0));
                while let Some(record) = input.next_element_seed(TapeRecordSeed(records.len()))? { records.push(record); }
                Ok(ScriptedTape::new(records))
            }
        }
        deserializer.deserialize_seq(TapeVisitor)
    }
}
