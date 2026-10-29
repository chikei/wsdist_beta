use std::sync::Arc;

use serde_json::{Map, Value};

use crate::model::{CalcError, LegacyF64, RandomSource, RandomStream};

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

    fn from_json(value: &Value, index: usize) -> Result<Self, CalcError> {
        let record = value.as_object().ok_or_else(|| record_error(index, "record", "expected an object"))?;
        let stream = match required(record, index, "stream")?.as_str() {
            Some("NumpyHost") => RandomStream::Primary,
            Some("NumbaPython") => RandomStream::Pdif,
            other => return Err(record_error(index, "stream", format!("unsupported stream {other:?}"))),
        };
        match required(record, index, "operation")?.as_str() {
            Some("uniform") => Ok(Self::uniform(
                stream,
                number(parameter(record, index, 0, 2, "low")?, index, "low")?,
                number(parameter(record, index, 1, 2, "high")?, index, "high")?,
                number(required(record, index, "value")?, index, "value")?,
            )),
            Some("bounded") => {
                let upper = integer(parameter(record, index, 0, 1, "upper_exclusive")?, index, "upper_exclusive")?;
                let result = integer(required(record, index, "value")?, index, "value")?;
                if upper == 0 || result >= upper {
                    return Err(record_error(index, "value", "bounded result is outside its nonempty exclusive range"));
                }
                Ok(Self::bounded(stream, upper, result))
            }
            Some("reseed") => {
                let seed = parameter(record, index, 0, 1, "seed")?.as_u64()
                    .ok_or_else(|| record_error(index, "seed", "expected a nonnegative u64 integer"))?;
                if record.get("value").is_some_and(|value| !value.is_null()) {
                    return Err(record_error(index, "value", "reseed has no return value"));
                }
                Ok(Self::reseed(stream, seed))
            }
            other => Err(record_error(index, "operation", format!("unsupported operation {other:?}"))),
        }
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

    /// Decode captured draw arrays once, outside action/hit loops. Accepts the
    /// combat `parameters` array and direct-helper named scalar parameters.
    /// Capture metadata is intentionally not part of the random protocol.
    pub fn from_json_records(records: &Value) -> Result<Self, CalcError> {
        let records = records.as_array().ok_or_else(|| record_error(0, "records", "expected a draw array"))?;
        let decoded = records.iter().enumerate().map(|(index, record)| TapeRecord::from_json(record, index))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(decoded))
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

fn required<'a>(record: &'a Map<String, Value>, index: usize, name: &'static str) -> Result<&'a Value, CalcError> {
    record.get(name).ok_or_else(|| record_error(index, name, "missing field"))
}

fn parameter<'a>(record: &'a Map<String, Value>, index: usize, position: usize, arity: usize, name: &'static str) -> Result<&'a Value, CalcError> {
    if let Some(parameters) = record.get("parameters") {
        if record.contains_key(name) {
            return Err(record_error(index, name, "parameter specified twice"));
        }
        let parameters = parameters.as_array().ok_or_else(|| record_error(index, "parameters", "expected an array"))?;
        if parameters.len() != arity {
            return Err(record_error(index, "parameters", format!("expected {arity} parameters, got {}", parameters.len())));
        }
        Ok(&parameters[position])
    } else {
        required(record, index, name)
    }
}

fn number(value: &Value, index: usize, field: &'static str) -> Result<f64, CalcError> {
    serde_json::from_value::<LegacyF64>(value.clone()).map(|value| value.0)
        .map_err(|error| record_error(index, field, error.to_string()))
}

fn integer(value: &Value, index: usize, field: &'static str) -> Result<usize, CalcError> {
    value.as_u64().and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| record_error(index, field, "expected a representable nonnegative integer"))
}
