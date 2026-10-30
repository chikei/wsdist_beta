//! Native ordered fixture values and strictly versioned bitcode/Zstandard storage.

use std::error::Error;
use std::fmt;

mod de;
mod ser;
mod storage;
mod value;

pub use storage::{
    MAX_PAYLOAD_BYTES, content_hash, decode_value, encode_value, read, read_json,
    read_json_value, read_value, write_value,
};
pub use value::{Map, NativeIndex, Number, Value};

pub type Result<T, E = Box<dyn Error + Send + Sync>> = std::result::Result<T, E>;

/// A conversion error from the native serde value serializer or deserializer.
#[derive(Debug)]
pub struct ValueError(String);

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ValueError {}

impl serde::de::Error for ValueError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl serde::ser::Error for ValueError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

/// Deserializes a typed projection by borrowing a native fixture tree.
pub fn from_value<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
    Ok(T::deserialize(value)?)
}

/// Serializes a value directly into the native tree using ordinary JSON semantics.
pub fn to_value<T: serde::Serialize>(value: T) -> Result<Value> {
    Ok(value.serialize(ser::Serializer::new())?)
}

/// Drops a native tree within this crate's optimized compilation boundary.
pub fn drop_value(value: Value) {
    drop(value);
}
