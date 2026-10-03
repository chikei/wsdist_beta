//! Build-only loading of frozen JSON, including byte-exact multipart storage.
//! A descriptor's first top-level key must be the literal `"fixture_format"`.
//! This prefix lets ordinary and typed JSON deserialize once without a Value
//! intermediate, reserialization, or a second whole-document scan.
use std::error::Error;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};

use serde::{Deserialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;
const MAX_PART_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Descriptor {
    fixture_format: String,
    sha256: String,
    byte_length: u64,
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    path: String,
    byte_length: u64,
}

pub fn read<T: DeserializeOwned>(path: &Path, expected_sha256: Option<&str>) -> Result<T> {
    let bytes = logical_bytes(path, expected_sha256)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: parsing fixture JSON: {error}", path.display()).into())
}

fn logical_bytes(path: &Path, expected_sha256: Option<&str>) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .map_err(|error| format!("{}: reading fixture JSON: {error}", path.display()))?;
    let descriptor_prefix = bytes.trim_ascii_start().strip_prefix(b"{")
        .is_some_and(|object| object.trim_ascii_start().starts_with(b"\"fixture_format\""));
    if !descriptor_prefix {
        verify_hash(path, &bytes, None, expected_sha256)?;
        return Ok(bytes);
    }
    let descriptor: Descriptor = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid multipart fixture descriptor: {error}", path.display()))?;
    if descriptor.fixture_format != "byte-parts-v1" {
        return Err(format!("{}: unsupported fixture_format {:?}", path.display(), descriptor.fixture_format).into());
    }
    let source = reconstruct(path, &descriptor)?;
    verify_hash(path, &source, Some(&descriptor.sha256), expected_sha256)?;
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

fn verify_hash(path: &Path, bytes: &[u8], source_sha256: Option<&str>, expected_sha256: Option<&str>) -> Result<()> {
    if source_sha256.is_none() && expected_sha256.is_none() {
        return Ok(());
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    for (label, expected) in [("multipart source", source_sha256), ("index", expected_sha256)] {
        if let Some(expected) = expected {
            if actual != expected {
                return Err(format!("{}: {label} SHA-256 mismatch: expected {expected}, got {actual}", path.display()).into());
            }
        }
    }
    Ok(())
}
