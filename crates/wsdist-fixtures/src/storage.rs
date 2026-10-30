use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

use serde::de::DeserializeOwned;
use xxhash_rust::xxh3::xxh3_128;

use crate::value::MAX_DEPTH;
use crate::{Result, Value, from_value};

const MAGIC: &[u8; 8] = b"WSFIX\0\x01\0";
const HEADER_BYTES: u64 = 32;
const ZSTD_MAGIC: &[u8; 4] = b"\x28\xb5\x2f\xfd";

/// Maximum uncompressed bitcode payload, excluding the 32-byte application header.
pub const MAX_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;

/// Reads only a single versioned, checksummed Zstandard/bitcode fixture.
///
/// The optional index hash must be 32 lowercase hexadecimal digits over the
/// uncompressed bitcode payload. It is checked independently of the header hash.
pub fn read_value(path: &Path, expected: Option<&str>) -> Result<Value> {
    let file = File::open(path).map_err(|error| format!("{}: opening binary fixture: {error}", path.display()))?;
    decode_reader(BufReader::new(file), expected)
        .map_err(|error| format!("{}: decoding binary fixture: {error}", path.display()).into())
}

/// Reads a binary fixture, then borrows its native tree for typed deserialization.
pub fn read<T: DeserializeOwned>(path: &Path, expected: Option<&str>) -> Result<T> {
    let value = read_value(path, expected)?;
    from_value(&value).map_err(|error| format!("{}: deserializing native fixture: {error}", path.display()).into())
}

/// Explicitly reads readable metadata JSON directly into a native ordered tree.
pub fn read_json_value(path: &Path) -> Result<Value> {
    let file = File::open(path).map_err(|error| format!("{}: opening metadata JSON: {error}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("{}: parsing metadata JSON: {error}", path.display()).into())
}

/// Fully validates metadata JSON before deserializing a typed projection.
///
/// Unknown/discarded fields must be valid native JSON values too; this deliberately
/// does not let a typed serde visitor skip malformed or out-of-range numbers.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let value = read_json_value(path)?;
    from_value(&value).map_err(|error| format!("{}: deserializing metadata JSON: {error}", path.display()).into())
}

/// Encodes one native value as a level-3 frame with content size and checksum.
/// Returns physical storage bytes and the canonical uncompressed bitcode digest.
pub fn encode_value(value: &Value) -> Result<(Vec<u8>, String)> {
    validate_depth(value, MAX_DEPTH)?;
    let payload = bitcode::serialize(value).map_err(|error| format!("serializing fixture bitcode: {error}"))?;
    let length = payload.len() as u64;
    if length > MAX_PAYLOAD_BYTES {
        return Err(format!("fixture payload length {length} exceeds the 512 MiB limit").into());
    }
    let hash = content_hash(&payload);
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 3)?;
    encoder.include_checksum(true)?;
    encoder.include_contentsize(true)?;
    encoder.set_pledged_src_size(Some(HEADER_BYTES + length))?;
    encoder.write_all(MAGIC)?;
    encoder.write_all(&length.to_le_bytes())?;
    encoder.write_all(&hash.to_le_bytes())?;
    encoder.write_all(&payload)?;
    Ok((encoder.finish()?, format!("{hash:032x}")))
}

/// Decodes compressed bytes directly into the final native value.
pub fn decode_value(bytes: &[u8], expected: Option<&str>) -> Result<Value> {
    decode_reader(bytes, expected)
}

/// Writes a binary fixture and returns its canonical bitcode content digest.
pub fn write_value(path: &Path, value: &Value) -> Result<String> {
    let (bytes, hash) = encode_value(value)
        .map_err(|error| format!("{}: encoding binary fixture: {error}", path.display()))?;
    fs::write(path, bytes).map_err(|error| format!("{}: writing binary fixture: {error}", path.display()))?;
    Ok(hash)
}

/// Default-secret, unseeded XXH3-128 of the exact supplied bytes.
pub fn content_hash(bytes: &[u8]) -> u128 { xxh3_128(bytes) }

fn decode_reader<R: BufRead>(mut reader: R, expected: Option<&str>) -> Result<Value> {
    let index_hash = expected.map(|hash| parse_content_hash(hash).ok_or_else(||
        format!("index content_xxh3_128 must contain exactly 32 lowercase hexadecimal digits, got {hash:?}")
    )).transpose()?;
    let prefix = reader.fill_buf()?;
    if prefix.len() < 5 || &prefix[..4] != ZSTD_MAGIC {
        return Err("fixture must contain one standard Zstandard frame; JSON and multipart storage are unsupported".into());
    }
    if prefix[4] & 4 == 0 {
        return Err("fixture Zstandard frame must include a content checksum".into());
    }
    let frame_length = zstd::zstd_safe::get_frame_content_size(prefix)
        .map_err(|_| "invalid or truncated Zstandard frame header")?
        .ok_or("fixture Zstandard frame must declare its uncompressed content size")?;
    if !(HEADER_BYTES..=HEADER_BYTES + MAX_PAYLOAD_BYTES).contains(&frame_length) {
        return Err(format!("fixture frame length {frame_length} is outside the 32-byte header plus 512 MiB payload limit").into());
    }
    let mut decoder = zstd::stream::read::Decoder::with_buffer(reader)?.single_frame();
    decoder.window_log_max(29)?;
    let mut header = [0_u8; HEADER_BYTES as usize];
    decoder.read_exact(&mut header).map_err(|error| format!("reading fixture header: {error}"))?;
    if &header[..8] != MAGIC {
        return Err("unsupported fixture magic or storage version".into());
    }
    let length = u64::from_le_bytes(header[8..16].try_into().unwrap());
    if length > MAX_PAYLOAD_BYTES {
        return Err(format!("fixture payload length {length} exceeds the 512 MiB limit").into());
    }
    if frame_length != HEADER_BYTES + length {
        return Err(format!("Zstandard content size {frame_length} differs from header plus declared payload length {}", HEADER_BYTES + length).into());
    }
    let length = usize::try_from(length).map_err(|_| "fixture payload length cannot fit in memory")?;
    let mut payload = Vec::new();
    payload.try_reserve_exact(length).map_err(|error| format!("allocating {length} fixture payload bytes: {error}"))?;
    payload.resize(length, 0);
    decoder.read_exact(&mut payload).map_err(|error| format!("reading fixture payload: {error}"))?;
    let mut extra = [0_u8; 1];
    // Reading through frame EOF is required to check the compression checksum.
    if decoder.read(&mut extra).map_err(|error| format!("finishing fixture frame: {error}"))? != 0 {
        return Err("fixture contains decompressed bytes beyond its declared payload length".into());
    }
    let mut reader = decoder.into_inner();
    if reader.read(&mut extra)? != 0 {
        return Err("fixture contains trailing bytes or concatenated Zstandard frames".into());
    }
    let actual = content_hash(&payload);
    let header_hash = u128::from_le_bytes(header[16..32].try_into().unwrap());
    if actual != header_hash {
        return Err(format!("header XXH3-128 mismatch: expected {header_hash:032x}, got {actual:032x}").into());
    }
    if let Some(index_hash) = index_hash {
        if actual != index_hash {
            return Err(format!("index XXH3-128 mismatch: expected {index_hash:032x}, got {actual:032x}").into());
        }
    }
    bitcode::deserialize(&payload).map_err(|error| format!("deserializing fixture bitcode: {error}").into())
}

fn parse_content_hash(expected: &str) -> Option<u128> {
    if expected.len() != 32 { return None; }
    let mut hash = 0_u128;
    for byte in expected.bytes() {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return None,
        };
        hash = (hash << 4) | u128::from(digit);
    }
    Some(hash)
}

fn validate_depth(value: &Value, remaining: usize) -> Result<()> {
    match value {
        Value::Array(values) => {
            let remaining = remaining.checked_sub(1).ok_or("fixture nesting exceeds 128 containers")?;
            for value in values { validate_depth(value, remaining)?; }
        }
        Value::Object(values) => {
            let remaining = remaining.checked_sub(1).ok_or("fixture nesting exceeds 128 containers")?;
            for value in values.values() { validate_depth(value, remaining)?; }
        }
        _ => {}
    }
    Ok(())
}
