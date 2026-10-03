use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde_json::{Map, Number, Value};
use wsdist_core::model::python_truth;

use crate::profile_registry::{INPUT_REGISTRY, InputKind};

pub type ProfileValues = Map<String, Value>;

/// Name2 references are saved separately from the widget registry, in legacy
/// slot order with Quicklook, TP and WS references adjacent for each slot.
pub const GEAR_PROFILE_KEYS: [&str; 48] = [
    "quicklook_main_item", "tp_quicklook_main_item", "ws_quicklook_main_item",
    "quicklook_sub_item", "tp_quicklook_sub_item", "ws_quicklook_sub_item",
    "quicklook_ranged_item", "tp_quicklook_ranged_item", "ws_quicklook_ranged_item",
    "quicklook_ammo_item", "tp_quicklook_ammo_item", "ws_quicklook_ammo_item",
    "quicklook_head_item", "tp_quicklook_head_item", "ws_quicklook_head_item",
    "quicklook_neck_item", "tp_quicklook_neck_item", "ws_quicklook_neck_item",
    "quicklook_ear1_item", "tp_quicklook_ear1_item", "ws_quicklook_ear1_item",
    "quicklook_ear2_item", "tp_quicklook_ear2_item", "ws_quicklook_ear2_item",
    "quicklook_body_item", "tp_quicklook_body_item", "ws_quicklook_body_item",
    "quicklook_hands_item", "tp_quicklook_hands_item", "ws_quicklook_hands_item",
    "quicklook_ring1_item", "tp_quicklook_ring1_item", "ws_quicklook_ring1_item",
    "quicklook_ring2_item", "tp_quicklook_ring2_item", "ws_quicklook_ring2_item",
    "quicklook_back_item", "tp_quicklook_back_item", "ws_quicklook_back_item",
    "quicklook_waist_item", "tp_quicklook_waist_item", "ws_quicklook_waist_item",
    "quicklook_legs_item", "tp_quicklook_legs_item", "ws_quicklook_legs_item",
    "quicklook_feet_item", "tp_quicklook_feet_item", "ws_quicklook_feet_item",
];

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("Cannot {operation} profiles at {path:?}: {source}")]
    Io {
        path: PathBuf,
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("Invalid profile JSON at {path:?}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("Invalid profile document {path:?} at {field}: expected {expected}")]
    Shape {
        /// None denotes an in-memory document supplied without a filesystem path.
        path: Option<PathBuf>,
        field: String,
        expected: &'static str,
    },
}

impl ProfileError {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Io { path, .. } | Self::Json { path, .. } => Some(path),
            Self::Shape { path, .. } => path.as_deref(),
        }
    }
}

/// An inert, unversioned collection of display-job-name profiles. Loading has no
/// write side effects. Restore order, missing-value retention and label handling
/// belong to the controller; storage never merges a missing job with `default`.
#[derive(Debug, Clone, Default)]
pub struct ProfileStore {
    document: Map<String, Value>,
}

impl ProfileStore {
    pub fn from_document(document: Map<String, Value>) -> Result<Self, ProfileError> {
        Self::checked(document, None)
    }

    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let file = File::open(path).map_err(|source| profile_io(path, "open", source))?;
        let value: Value = serde_json::from_reader(BufReader::new(file))
            .map_err(|source| profile_json_error(path, "read", source))?;
        let Value::Object(document) = value else {
            return Err(ProfileError::Shape {
                path: Some(path.to_owned()),
                field: "$".to_owned(),
                expected: "an object mapping display job names to profile objects",
            });
        };
        Self::checked(document, Some(path.to_owned()))
    }

    fn checked(
        document: Map<String, Value>,
        path: Option<PathBuf>,
    ) -> Result<Self, ProfileError> {
        for (job, profile) in &document {
            if !profile.is_object() {
                return Err(ProfileError::Shape {
                    path,
                    field: format!("$[{}]", Value::String(job.clone())),
                    expected: "an object mapping setting keys to values",
                });
            }
        }
        Ok(Self { document })
    }

    pub fn profile(&self, job: &str) -> Option<&ProfileValues> {
        self.document.get(job).and_then(Value::as_object)
    }

    pub fn document(&self) -> &Map<String, Value> {
        &self.document
    }

    /// Updates only supplied registered inputs and gear references. Unknown keys
    /// are retained independently in `default` and the selected job, and other
    /// profiles are untouched. Missing effective keys retain their previous value.
    /// The document is committed to memory only after atomic disk replacement.
    pub fn save(
        &mut self,
        path: &Path,
        job: &str,
        effective: &ProfileValues,
    ) -> Result<(), ProfileError> {
        let mut known = Vec::with_capacity(effective.len().min(INPUT_REGISTRY.len() + GEAR_PROFILE_KEYS.len()));
        for spec in INPUT_REGISTRY {
            if let Some(value) = effective.get(spec.key) {
                let saved = match spec.kind {
                    InputKind::Checkbox => Value::Bool(legacy_checkbox(value)),
                    InputKind::Combo | InputKind::Entry | InputKind::Label => {
                        Value::String(legacy_text(value))
                    }
                };
                known.push((spec.key, saved));
            }
        }
        for key in GEAR_PROFILE_KEYS {
            if let Some(value) = effective.get(key) {
                known.push((key, Value::String(legacy_text(value))));
            }
        }

        let mut next = self.document.clone();
        if job != "default" {
            let profile = saved_profile(&mut next, "default");
            for (key, value) in &known {
                set_saved_value(profile, key, value.clone());
            }
        }
        let profile = saved_profile(&mut next, job);
        for (key, value) in known {
            set_saved_value(profile, key, value);
        }

        // Serialization, flushing and syncing all finish in the same-directory
        // tempfile before persist. A failure leaves both the original and `self`.
        let parent = path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::Builder::new()
            .prefix(".wsdist-defaults-")
            .tempfile_in(parent)
            .map_err(|source| profile_io(path, "create a temporary file for", source))?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, &next)
                .map_err(|source| profile_json_error(path, "write", source))?;
            writer.write_all(b"\n").map_err(|source| profile_io(path, "write", source))?;
            writer.flush().map_err(|source| profile_io(path, "flush", source))?;
        }
        temporary.as_file().sync_all()
            .map_err(|source| profile_io(path, "sync", source))?;
        temporary.persist(path)
            .map_err(|error| profile_io(path, "replace", error.error))?;
        self.document = next;
        Ok(())
    }
}

fn profile_io(path: &Path, operation: &'static str, source: io::Error) -> ProfileError {
    ProfileError::Io { path: path.to_owned(), operation, source }
}

fn profile_json_error(
    path: &Path,
    operation: &'static str,
    source: serde_json::Error,
) -> ProfileError {
    match source.io_error_kind() {
        Some(kind) => profile_io(path, operation, io::Error::new(kind, source)),
        None => ProfileError::Json { path: path.to_owned(), source },
    }
}

fn saved_profile<'a>(document: &'a mut Map<String, Value>, name: &str) -> &'a mut ProfileValues {
    if !document.contains_key(name) {
        document.insert(name.to_owned(), Value::Object(ProfileValues::new()));
    }
    document.get_mut(name)
        .and_then(Value::as_object_mut)
        .expect("ProfileStore only contains validated profile objects")
}

fn set_saved_value(profile: &mut ProfileValues, key: &str, saved: Value) {
    if let Some(value) = profile.get_mut(key) {
        *value = saved;
    } else {
        profile.insert(key.to_owned(), saved);
    }
}

/// Matches the legacy checkbox conversion, notably `"0"` being true.
pub fn legacy_checkbox(value: &Value) -> bool {
    python_truth(value)
}

/// Python's `str` spelling for JSON scalars, without validating numeric-entry
/// text or rejecting nonempty combo selections absent from the literal choices.
pub fn legacy_text(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(number) => python_number_text(number),
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => {
            let mut result = String::new();
            append_python_repr(&mut result, value);
            result
        }
    }
}

fn append_python_repr(result: &mut String, value: &Value) {
    match value {
        Value::Null => result.push_str("None"),
        Value::Bool(true) => result.push_str("True"),
        Value::Bool(false) => result.push_str("False"),
        Value::Number(number) => result.push_str(&python_number_text(number)),
        Value::String(text) => append_python_string_repr(result, text),
        Value::Array(values) => {
            result.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    result.push_str(", ");
                }
                append_python_repr(result, value);
            }
            result.push(']');
        }
        Value::Object(values) => {
            result.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    result.push_str(", ");
                }
                append_python_string_repr(result, key);
                result.push_str(": ");
                append_python_repr(result, value);
            }
            result.push('}');
        }
    }
}

fn append_python_string_repr(result: &mut String, text: &str) {
    let quote = if text.contains('\'') && !text.contains('"') { '"' } else { '\'' };
    result.push(quote);
    for character in text.chars() {
        match character {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            character if character == quote => {
                result.push('\\');
                result.push(character);
            }
            character if character.is_control() => {
                use std::fmt::Write;
                let code = character as u32;
                if code <= 0xff {
                    write!(result, "\\x{code:02x}").expect("writing to a String cannot fail");
                } else {
                    write!(result, "\\u{code:04x}").expect("writing to a String cannot fail");
                }
            }
            character => result.push(character),
        }
    }
    result.push(quote);
}

fn python_number_text(number: &Number) -> String {
    if !number.is_f64() {
        return number.to_string();
    }

    // JSON's shortest-round-trip float digits are also the Python repr digits,
    // but scientific-notation thresholds and exponent spelling are different.
    let encoded = number.to_string();
    let (sign, unsigned) = match encoded.strip_prefix('-') {
        Some(unsigned) => ("-", unsigned),
        None => ("", encoded.as_str()),
    };
    let (coefficient, scale) = unsigned.split_once('e')
        .or_else(|| unsigned.split_once('E'))
        .map_or((unsigned, 0), |(coefficient, exponent)| {
            (coefficient, exponent.parse::<i32>().expect("JSON float exponent is an integer"))
        });
    let decimal = coefficient.find('.').unwrap_or(coefficient.len());
    let digits: String = coefficient.chars().filter(|character| *character != '.').collect();
    let leading_zeroes = digits.len() - digits.trim_start_matches('0').len();
    let significant = digits.trim_start_matches('0').trim_end_matches('0');
    if significant.is_empty() {
        return format!("{sign}0.0");
    }
    let exponent = scale + decimal as i32 - leading_zeroes as i32 - 1;
    if !(-4..16).contains(&exponent) {
        let (first, remaining) = significant.split_at(1);
        let fraction = if remaining.is_empty() { String::new() } else { format!(".{remaining}") };
        return format!("{sign}{first}{fraction}e{exponent:+03}");
    }
    let decimal = exponent + 1;
    if decimal <= 0 {
        format!("{sign}0.{}{significant}", "0".repeat((-decimal) as usize))
    } else if decimal as usize >= significant.len() {
        format!("{sign}{significant}{}.0", "0".repeat(decimal as usize - significant.len()))
    } else {
        let (integer, fraction) = significant.split_at(decimal as usize);
        format!("{sign}{integer}.{fraction}")
    }
}
