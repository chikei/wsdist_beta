#[path = "support/fixture_json.rs"]
mod fixture_json;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Number, Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const ORIGINAL: &str = r#"{"z":"é雪🦀\\quoted\"","nested":{"second":2,"first":1},"numbers":[18446744073709551615,-9223372036854775808,1,1.0,-0,-0.0,1.2345678901234567,2.55,1e-20],"array":["last","first",{"b":false,"a":true}]}"#;

struct StoredFixture {
    _directory: TempDir,
    path: PathBuf,
    descriptor: Value,
}

impl StoredFixture {
    fn part_path(&self, index: usize) -> PathBuf {
        self.path.parent().unwrap().join(self.descriptor["parts"][index]["path"].as_str().unwrap())
    }

    fn save_descriptor(&self) {
        fs::write(&self.path, serde_json::to_vec(&self.descriptor).unwrap()).unwrap();
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn multipart(source: &[u8], width: usize) -> StoredFixture {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("nested");
    fs::create_dir_all(parent.join("case.parts")).unwrap();
    let parts = source.chunks(width).enumerate().map(|(index, chunk)| {
        let relative = format!("case.parts/{index:06}.part");
        fs::write(parent.join(&relative), chunk).unwrap();
        json!({"path": relative, "byte_length": chunk.len()})
    }).collect::<Vec<_>>();
    let stored = StoredFixture {
        path: parent.join("case.json"),
        _directory: directory,
        descriptor: json!({
            "fixture_format": "byte-parts-v1",
            "sha256": sha256(source),
            "byte_length": source.len(),
            "parts": parts,
        }),
    };
    stored.save_descriptor();
    stored
}

fn assert_original(value: &Value) {
    assert_eq!(value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
        ["z", "nested", "numbers", "array"]);
    assert_eq!(value["z"], "é雪🦀\\quoted\"");
    assert_eq!(value["nested"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
        ["second", "first"]);
    let numbers = value["numbers"].as_array().unwrap();
    assert_eq!(numbers.len(), 9);
    assert_eq!(numbers[0].as_u64(), Some(u64::MAX));
    assert!(!numbers[0].is_f64());
    assert_eq!(numbers[1].as_i64(), Some(i64::MIN));
    assert!(!numbers[1].is_f64());
    assert_eq!(numbers[2].as_u64(), Some(1));
    assert!(!numbers[2].is_f64());
    assert!(numbers[3].is_f64());
    assert_eq!(numbers[3].as_f64(), Some(1.0));
    for number in &numbers[4..6] {
        assert!(number.is_f64());
        assert_eq!(number.as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
    }
    for (number, expected) in numbers[6..].iter().zip([1.2345678901234567_f64, 2.55, 1e-20]) {
        assert!(number.is_f64());
        assert_eq!(number.as_f64().unwrap().to_bits(), expected.to_bits());
    }
    let array = value["array"].as_array().unwrap();
    assert_eq!(array.len(), 3);
    assert_eq!(array[0], "last");
    assert_eq!(array[1], "first");
    assert_eq!(array[2].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["b", "a"]);
    assert_eq!(array[2]["b"], false);
    assert_eq!(array[2]["a"], true);
}

fn assert_document_error(stored: &StoredFixture) -> String {
    let message = fixture_json::read::<Value>(&stored.path, None).unwrap_err().to_string();
    assert!(message.contains(stored.path.to_str().unwrap()), "{message}");
    message
}

#[test]
fn ordinary_json_keeps_exact_values_and_order_and_hashes_original_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ordinary.json");
    let source = format!("\n {ORIGINAL} \r\n");
    fs::write(&path, source.as_bytes()).unwrap();
    assert_original(&fixture_json::read::<Value>(&path, None).unwrap());
    assert_original(&fixture_json::read::<Value>(&path, Some(&sha256(source.as_bytes()))).unwrap());
    let error = fixture_json::read::<Value>(&path, Some(&sha256(ORIGINAL.as_bytes()))).unwrap_err().to_string();
    assert!(error.contains(path.to_str().unwrap()), "{error}");
}

#[test]
fn multipart_reconstructs_across_every_token_escape_and_utf8_byte_boundary() {
    let stored = multipart(ORIGINAL.as_bytes(), 1);
    assert_original(&fixture_json::read::<Value>(&stored.path, None).unwrap());
    assert_original(&fixture_json::read::<Value>(&stored.path, Some(&sha256(ORIGINAL.as_bytes()))).unwrap());
    let physical_hash = sha256(&fs::read(&stored.path).unwrap());
    let error = fixture_json::read::<Value>(&stored.path, Some(&physical_hash)).unwrap_err().to_string();
    assert!(error.contains(stored.path.to_str().unwrap()), "{error}");
}

#[derive(Deserialize)]
struct TypedCase {
    id: String,
    numbers: Vec<Number>,
}

#[test]
fn typed_documents_deserialize_original_bytes_for_ordinary_and_multipart_storage() {
    let source = r#"{"id":"é🦀","numbers":[18446744073709551615,1,1.0,-0.0]}"#.as_bytes();
    let stored = multipart(source, 2);
    let ordinary = stored.path.parent().unwrap().join("ordinary.json");
    fs::write(&ordinary, source).unwrap();
    for path in [&ordinary, &stored.path] {
        let value: TypedCase = fixture_json::read(path, None).unwrap();
        assert_eq!(value.id, "é🦀");
        assert_eq!(value.numbers.len(), 4);
        assert_eq!(value.numbers[0].as_u64(), Some(u64::MAX));
        assert_eq!(value.numbers[1].as_u64(), Some(1));
        assert!(!value.numbers[1].is_f64());
        assert!(value.numbers[2].is_f64());
        assert_eq!(value.numbers[2].as_f64(), Some(1.0));
        assert_eq!(value.numbers[3].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
    }
}

#[test]
fn canonical_descriptor_prefix_allows_leading_json_whitespace() {
    let stored = multipart(ORIGINAL.as_bytes(), 7);
    let descriptor = serde_json::to_string_pretty(&stored.descriptor).unwrap();
    fs::write(&stored.path, format!(" \r\n\t{descriptor}\n")).unwrap();
    assert_original(&fixture_json::read::<Value>(&stored.path, None).unwrap());
}

#[test]
fn nested_format_keys_are_ordinary_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ordinary.json");
    let source = br#"[{"fixture_format":"not-a-storage-descriptor"},{"text":"fixture_format"}]"#;
    fs::write(&path, source).unwrap();
    let value = fixture_json::read::<Value>(&path, None).unwrap();
    assert_eq!(value[0]["fixture_format"], "not-a-storage-descriptor");
    assert_eq!(value[1]["text"], "fixture_format");
}

#[test]
fn missing_parts_report_descriptor_and_part_paths() {
    let stored = multipart(ORIGINAL.as_bytes(), 8);
    let missing = stored.part_path(1);
    fs::remove_file(&missing).unwrap();
    let error = assert_document_error(&stored);
    assert!(error.contains(missing.to_str().unwrap()), "{error}");
}

#[test]
fn truncated_parts_are_rejected_at_the_declared_part_length() {
    let stored = multipart(ORIGINAL.as_bytes(), 8);
    OpenOptions::new().write(true).open(stored.part_path(0)).unwrap().set_len(7).unwrap();
    let error = assert_document_error(&stored);
    assert!(error.contains(stored.part_path(0).to_str().unwrap()), "{error}");
}

#[test]
fn extra_part_bytes_are_not_silently_ignored() {
    let stored = multipart(ORIGINAL.as_bytes(), 8);
    OpenOptions::new().append(true).open(stored.part_path(0)).unwrap().write_all(b"x").unwrap();
    let error = assert_document_error(&stored);
    assert!(error.contains(stored.part_path(0).to_str().unwrap()), "{error}");
}

#[test]
fn reordered_equal_length_parts_fail_source_hash_verification() {
    let mut stored = multipart(b"[1,2]", 1);
    stored.descriptor["parts"].as_array_mut().unwrap().swap(1, 3);
    stored.save_descriptor();
    assert_document_error(&stored);
}

#[test]
fn same_length_valid_json_corruption_fails_source_hash_verification() {
    let stored = multipart(br#"{"x":1}"#, 8);
    fs::write(stored.part_path(0), br#"{"x":2}"#).unwrap();
    assert_document_error(&stored);
}

#[test]
fn aggregate_and_declared_original_lengths_must_agree() {
    let mut stored = multipart(ORIGINAL.as_bytes(), 8);
    stored.descriptor["byte_length"] = json!(ORIGINAL.len() + 1);
    stored.save_descriptor();
    assert_document_error(&stored);
}

#[test]
fn individual_part_length_cannot_exceed_sixteen_mib() {
    let mut stored = multipart(ORIGINAL.as_bytes(), 8);
    stored.descriptor["parts"][0]["byte_length"] = json!(16 * 1024 * 1024 + 1);
    stored.save_descriptor();
    assert_document_error(&stored);
}

#[test]
fn descriptor_part_paths_must_stay_relative_to_the_descriptor() {
    for invalid in ["", "../outside.part"] {
        let mut stored = multipart(ORIGINAL.as_bytes(), 8);
        stored.descriptor["parts"][0]["path"] = json!(invalid);
        stored.save_descriptor();
        assert_document_error(&stored);
    }
    let mut stored = multipart(ORIGINAL.as_bytes(), 8);
    stored.descriptor["parts"][0]["path"] = json!(stored.part_path(0).to_str().unwrap());
    stored.save_descriptor();
    assert_document_error(&stored);
}

#[test]
fn unknown_versions_and_malformed_descriptors_are_not_returned_as_case_values() {
    let mut stored = multipart(ORIGINAL.as_bytes(), 8);
    stored.descriptor["fixture_format"] = json!("byte-parts-v2");
    stored.save_descriptor();
    assert_document_error(&stored);
    stored.descriptor["fixture_format"] = json!("byte-parts-v1");
    stored.descriptor.as_object_mut().unwrap().remove("parts");
    stored.save_descriptor();
    assert_document_error(&stored);
}

#[test]
fn a_valid_descriptor_hash_does_not_hide_invalid_reconstructed_json() {
    let stored = multipart(br#"{"unfinished":[1,"#, 3);
    assert_document_error(&stored);
}

#[test]
fn edited_descriptor_and_parts_cannot_bypass_the_independent_index_hash() {
    let original = br#"{"id":"original","value":-0.0}"#;
    let replacement = br#"{"id":"modified","value":-0.0}"#;
    assert_eq!(original.len(), replacement.len());
    let index_sha256 = sha256(original);
    let mut stored = multipart(original, 5);
    for (index, chunk) in replacement.chunks(5).enumerate() {
        fs::write(stored.part_path(index), chunk).unwrap();
    }
    stored.descriptor["sha256"] = json!(sha256(replacement));
    stored.save_descriptor();
    assert_eq!(fixture_json::read::<Value>(&stored.path, None).unwrap()["id"], "modified");
    let error = fixture_json::read::<Value>(&stored.path, Some(&index_sha256)).unwrap_err().to_string();
    assert!(error.contains(stored.path.to_str().unwrap()), "{error}");
}

fn check_fixture_sizes(directory: &Path) {
    let mut paths = fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let metadata = fs::metadata(&path).unwrap();
        if metadata.is_dir() {
            check_fixture_sizes(&path);
        } else if metadata.is_file() {
            assert!(metadata.len() < 50_000_000,
                "{}: repository fixture file has {} bytes; every stored fixture file must be below 50,000,000 bytes",
                path.display(), metadata.len());
        }
    }
}

#[test]
fn repository_fixture_files_stay_below_fifty_million_bytes() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    for root in [core.join("../wsdist-app/tests/fixtures"), core.join("tests/fixtures")] {
        check_fixture_sizes(&root);
    }
}
