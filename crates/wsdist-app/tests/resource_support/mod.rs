use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

pub fn gear_fragments(directory: &Path) -> io::Result<Vec<PathBuf>> {
    fn collect(directory: &Path, fragments: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                collect(&entry.path(), fragments)?;
            } else if kind.is_file() && entry.path().extension() == Some(OsStr::new("json5")) {
                fragments.push(entry.path());
            }
        }
        Ok(())
    }

    if !fs::symlink_metadata(directory)?.file_type().is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "gear root is not a regular directory"));
    }
    let mut fragments = Vec::new();
    collect(directory, &mut fragments)?;
    fragments.sort_by_cached_key(|path| {
        path.strip_prefix(directory).unwrap().components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>().join("/")
    });
    Ok(fragments)
}

pub fn copy_gear(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for path in gear_fragments(source)? {
        let destination = destination.join(path.strip_prefix(source).unwrap());
        fs::create_dir_all(destination.parent().unwrap())?;
        fs::copy(path, destination)?;
    }
    Ok(())
}

pub fn record_name2(record: &Value) -> Option<&str> {
    record.get("name2").or_else(|| record.get("name")).and_then(Value::as_str)
}

pub fn piece_fragment(directory: &Path, name2: &str) -> Result<(PathBuf, Value), Box<dyn Error>> {
    for path in gear_fragments(directory)? {
        let document: Value = json5::from_str(&fs::read_to_string(&path)?)?;
        let pieces = document["pieces"].as_array().ok_or_else(|| format!("Missing pieces in {}", path.display()))?;
        if pieces.iter().any(|record| record_name2(record) == Some(name2)
            && record["slots"].as_array().is_some_and(|slots| !slots.is_empty())) {
            return Ok((path, document));
        }
    }
    Err(format!("Selectable gear {name2:?} is missing from {}", directory.display()).into())
}
