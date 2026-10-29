//! Build-only loading of frozen JSON, including byte-exact multipart storage.
//! A descriptor's first top-level key must be the literal `"fixture_format"`.
//! This prefix lets ordinary and typed JSON deserialize once without a Value
//! intermediate, reserialization, or a second whole-document scan.

use std::error::Error;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};

use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use xxhash_rust::xxh3::xxh3_128;

pub type Result<T, E = Box<dyn Error + Send + Sync>> = std::result::Result<T, E>;

const MAX_PART_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Descriptor {
    fixture_format: String,
    content_xxh3_128: String,
    byte_length: u64,
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    path: String,
    byte_length: u64,
}

/// Loads and fully validates ordinary JSON or a `byte-parts-v2` fixture.
///
/// The concrete `Value` decode stays inside this crate. An optional index hash
/// must be exactly 32 lowercase hexadecimal digits and covers the logical JSON
/// bytes, including whitespace, rather than the physical descriptor bytes.
/// Multipart fixtures additionally require their own independently checked hash.
pub fn read_value(path: &Path, expected_content_xxh3_128: Option<&str>) -> Result<Value> {
    let bytes = logical_bytes(path, expected_content_xxh3_128)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: parsing fixture JSON: {error}", path.display()).into())
}

/// Loads a typed fixture directly from its complete logical JSON bytes.
///
/// Uses the same storage and content-hash checks as [`read_value`], without a
/// `Value` intermediate or reserialization.
pub fn read<T: DeserializeOwned>(path: &Path, expected_content_xxh3_128: Option<&str>) -> Result<T> {
    let bytes = logical_bytes(path, expected_content_xxh3_128)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: parsing fixture JSON: {error}", path.display()).into())
}

/// Returns the default-secret, unseeded XXH3-128 content-equality hash.
///
/// This is a noncryptographic hash of the exact supplied bytes. Its canonical
/// stored representation is the numeric value formatted with `{:032x}`, not its
/// native-endian memory representation or a truncated 64-bit hasher result.
pub fn content_hash(bytes: &[u8]) -> u128 {
    xxh3_128(bytes)
}

fn logical_bytes(path: &Path, expected_content_xxh3_128: Option<&str>) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .map_err(|error| format!("{}: reading fixture JSON: {error}", path.display()))?;
    let descriptor_prefix = bytes.trim_ascii_start().strip_prefix(b"{")
        .is_some_and(|object| object.trim_ascii_start().starts_with(b"\"fixture_format\""));
    if !descriptor_prefix {
        verify_hash(path, &bytes, None, expected_content_xxh3_128)?;
        return Ok(bytes);
    }
    let descriptor: Descriptor = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid multipart fixture descriptor: {error}", path.display()))?;
    if descriptor.fixture_format != "byte-parts-v2" {
        return Err(format!("{}: unsupported fixture_format {:?}", path.display(), descriptor.fixture_format).into());
    }
    let source = reconstruct(path, &descriptor)?;
    verify_hash(path, &source, Some(&descriptor.content_xxh3_128), expected_content_xxh3_128)?;
    Ok(source)
}

fn reconstruct(path: &Path, descriptor: &Descriptor) -> Result<Vec<u8>> {
    let mut total = 0_u64;
    for (index, part) in descriptor.parts.iter().enumerate() {
        let relative = Path::new(&part.path);
        if relative.as_os_str().is_empty()
            || !relative.components().all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(format!("{}: part {index} path {:?} must be a nonempty relative path without traversal", path.display(), part.path).into());
        }
        if part.byte_length > MAX_PART_BYTES {
            return Err(format!("{}: part {index} ({}) byte_length {} exceeds 16 MiB", path.display(), part.path, part.byte_length).into());
        }
        total = total.checked_add(part.byte_length)
            .ok_or_else(|| format!("{}: aggregate part byte_length overflow", path.display()))?;
    }
    if total != descriptor.byte_length {
        return Err(format!("{}: aggregate part byte_length {total} differs from declared byte_length {}", path.display(), descriptor.byte_length).into());
    }
    let length = usize::try_from(total)
        .map_err(|error| format!("{}: fixture byte_length {total} cannot fit in memory: {error}", path.display()))?;
    // Reserve the complete original once; each part is read directly into its slice.
    let mut source = Vec::new();
    source.try_reserve_exact(length)
        .map_err(|error| format!("{}: allocating {length} fixture bytes: {error}", path.display()))?;
    source.resize(length, 0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut offset = 0;
    for (index, part) in descriptor.parts.iter().enumerate() {
        let part_path = parent.join(&part.path);
        let mut file = File::open(&part_path)
            .map_err(|error| format!("{}: opening part {index} ({}): {error}", path.display(), part_path.display()))?;
        // Each part length fits usize because their checked sum already does.
        let end = offset + part.byte_length as usize;
        file.read_exact(&mut source[offset..end])
            .map_err(|error| format!("{}: reading {} bytes from part {index} ({}): {error}", path.display(), part.byte_length, part_path.display()))?;
        let mut extra = [0_u8; 1];
        if file.read(&mut extra)
            .map_err(|error| format!("{}: checking part {index} ({}) length: {error}", path.display(), part_path.display()))? != 0
        {
            return Err(format!("{}: part {index} ({}) contains bytes beyond declared byte_length {}", path.display(), part_path.display(), part.byte_length).into());
        }
        offset = end;
    }
    Ok(source)
}

fn parse_content_hash(expected: &str) -> Option<u128> {
    if expected.len() != 32 {
        return None;
    }
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

fn verify_hash(
    path: &Path,
    bytes: &[u8],
    source_content_xxh3_128: Option<&str>,
    expected_content_xxh3_128: Option<&str>,
) -> Result<()> {
    if source_content_xxh3_128.is_none() && expected_content_xxh3_128.is_none() {
        return Ok(());
    }
    let actual = content_hash(bytes);
    for (label, expected) in [
        ("multipart source", source_content_xxh3_128),
        ("index", expected_content_xxh3_128),
    ] {
        if let Some(expected) = expected {
            let expected_hash = parse_content_hash(expected).ok_or_else(|| {
                format!("{}: {label} content_xxh3_128 must contain exactly 32 lowercase hexadecimal digits, got {expected:?}", path.display())
            })?;
            if actual != expected_hash {
                return Err(format!("{}: {label} XXH3-128 mismatch: expected {expected}, got {actual:032x}", path.display()).into());
            }
        }
    }
    Ok(())
}
