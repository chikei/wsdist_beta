use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use wsdist_fixtures::{Map, Number, Value};

const ORIGINAL: &str = r#"{"z":"é雪🦀\\quoted\"","nested":{"second":2,"first":1},"numbers":[18446744073709551615,-9223372036854775808,1,1.0,-0,-0.0,1.2345678901234567,2.55,1e-20],"array":["last","first",{"b":false,"a":true}]}"#;
const HEADER_BYTES: usize = 32;

struct StoredFixture {
    _directory: TempDir,
    path: PathBuf,
    hash: String,
}

fn native(source: &str) -> Value { serde_json::from_str(source).unwrap() }

fn stored(value: &Value) -> StoredFixture {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("case.bitcode.zst");
    let hash = wsdist_fixtures::write_value(&path, value).unwrap();
    StoredFixture { _directory: directory, path, hash }
}

fn compressed(document: &[u8], checksum: bool, content_size: bool) -> Vec<u8> {
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 3).unwrap();
    encoder.include_checksum(checksum).unwrap();
    encoder.include_contentsize(content_size).unwrap();
    encoder.set_pledged_src_size(Some(document.len() as u64)).unwrap();
    encoder.write_all(document).unwrap();
    encoder.finish().unwrap()
}

fn document(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_BYTES + payload.len());
    bytes.extend_from_slice(b"WSFIX\0\x01\0");
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&wsdist_fixtures::content_hash(payload).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn document_for(value: &Value) -> Vec<u8> { document(&bitcode::serialize(value).unwrap()) }

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

#[test]
fn native_json_and_binary_roundtrips_preserve_order_scalar_kinds_and_f64_bits() {
    let directory = tempfile::tempdir().unwrap();
    let metadata = directory.path().join("metadata.json");
    fs::write(&metadata, format!("\n {ORIGINAL} \r\n")).unwrap();
    let value = wsdist_fixtures::read_json_value(&metadata).unwrap();
    assert_original(&value);
    let stored = stored(&value);
    assert_original(&wsdist_fixtures::read_value(&stored.path, Some(&stored.hash)).unwrap());
    let (bytes, hash) = wsdist_fixtures::encode_value(&value).unwrap();
    assert_eq!(hash, stored.hash);
    assert_original(&wsdist_fixtures::decode_value(&bytes, Some(&hash)).unwrap());
    let ordinary_json: serde_json::Value = serde_json::from_str(ORIGINAL).unwrap();
    assert_eq!(value, ordinary_json);
    assert_eq!(ordinary_json, value);
    let emitted = serde_json::to_string(&value).unwrap();
    assert_original(&native(&emitted));
}

#[test]
fn binary_numbers_keep_explicit_signed_kind_and_every_binary64_bit() {
    let bit_patterns = [
        0, 0x8000_0000_0000_0000, 1, 0x000f_ffff_ffff_ffff,
        0x7fef_ffff_ffff_ffff, 0x7ff0_0000_0000_0000, 0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_1234, 0x7ff0_0000_0000_4321,
    ];
    let mut values = vec![Value::Number(Number::U64(2)), Value::Number(Number::I64(2)), Value::Number(Number::I64(-2))];
    values.extend(bit_patterns.into_iter().map(|bits| Value::Number(Number::F64(f64::from_bits(bits)))));
    let (bytes, hash) = wsdist_fixtures::encode_value(&Value::Array(values)).unwrap();
    let decoded = wsdist_fixtures::decode_value(&bytes, Some(&hash)).unwrap();
    let numbers = decoded.as_array().unwrap();
    assert!(matches!(numbers[0], Value::Number(Number::U64(2))));
    assert!(matches!(numbers[1], Value::Number(Number::I64(2))));
    assert!(matches!(numbers[2], Value::Number(Number::I64(-2))));
    for (number, bits) in numbers[3..].iter().zip(bit_patterns) {
        assert!(number.is_f64());
        assert_eq!(number.as_f64().unwrap().to_bits(), bits);
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct TypedCase {
    id: String,
    numbers: Vec<Number>,
    selected: Option<u32>,
}

#[test]
fn typed_binary_and_metadata_reads_borrow_the_native_number_semantics() {
    let source = r#"{"id":"é🦀","numbers":[18446744073709551615,-9223372036854775808,1,1.0,-0.0],"selected":7,"ignored":{"deep":[true,null]}}"#;
    let value = native(source);
    let stored = stored(&value);
    let binary: TypedCase = wsdist_fixtures::read(&stored.path, Some(&stored.hash)).unwrap();
    let borrowed: TypedCase = wsdist_fixtures::from_value(&value).unwrap();
    let metadata = stored.path.with_file_name("index.json");
    fs::write(&metadata, source).unwrap();
    let readable: TypedCase = wsdist_fixtures::read_json(&metadata).unwrap();
    assert_eq!(binary, borrowed);
    assert_eq!(binary, readable);
    assert_eq!(binary.id, "é🦀");
    assert_eq!(binary.selected, Some(7));
    assert!(matches!(binary.numbers[0], Number::U64(u64::MAX)));
    assert!(matches!(binary.numbers[1], Number::I64(i64::MIN)));
    assert!(matches!(binary.numbers[2], Number::U64(1)));
    assert!(matches!(binary.numbers[3], Number::F64(_)));
    assert_eq!(binary.numbers[3].as_f64().unwrap().to_bits(), 1.0_f64.to_bits());
    assert_eq!(binary.numbers[4].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
}

#[derive(Debug, Deserialize)]
struct Borrowed<'a> { label: &'a str }

#[test]
fn public_native_deserializer_really_borrows_strings_and_rejects_unconsumed_tuples() {
    let value = native(r#"{"label":"é雪🦀"}"#);
    let result = Borrowed::deserialize((&value).into_deserializer()).unwrap();
    assert_eq!(result.label, "é雪🦀");
    assert_eq!(result.label.as_ptr(), value["label"].as_str().unwrap().as_ptr());
    assert_eq!(wsdist_fixtures::from_value::<(u32, String)>(&native(r#"[7,"seven"]"#)).unwrap(), (7, "seven".to_owned()));
    assert!(wsdist_fixtures::from_value::<(u32,)>(&native("[1,2]")).is_err());
    assert!(wsdist_fixtures::from_value::<u32>(&native("1.0")).is_err());
    assert!(wsdist_fixtures::from_value::<bool>(&native("1")).is_err());
    assert_eq!(wsdist_fixtures::from_value::<Option<String>>(&Value::Null).unwrap(), None);
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
enum Observation {
    Empty,
    One(u64),
    Pair(f64, String),
    Fields { active: bool, note: Option<String> },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Wrapper(String);

#[test]
fn direct_native_serializer_handles_typed_observations_enums_maps_and_json_scalars() {
    for observation in [
        Observation::Empty, Observation::One(u64::MAX),
        Observation::Pair(-0.0, "é\0雪".to_owned()),
        Observation::Fields { active: true, note: None },
    ] {
        let native = wsdist_fixtures::to_value(&observation).unwrap();
        assert_eq!(native, serde_json::to_value(&observation).unwrap());
        assert_eq!(wsdist_fixtures::from_value::<Observation>(&native).unwrap(), observation);
    }
    let map = BTreeMap::from([(2_u32, Wrapper("second".to_owned())), (1, Wrapper("first".to_owned()))]);
    let value = wsdist_fixtures::to_value(&map).unwrap();
    assert_eq!(value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["1", "2"]);
    assert_eq!(wsdist_fixtures::from_value::<BTreeMap<u32, Wrapper>>(&value).unwrap(), map);
    assert_eq!(wsdist_fixtures::to_value((true, '雪', Some(7), None::<u8>)).unwrap(), native(r#"[true,"雪",7,null]"#));
    let observation = TypedCase {
        id: "generated".to_owned(),
        numbers: vec![Number::U64(u64::MAX), Number::I64(i64::MIN), Number::F64(-0.0)],
        selected: None,
    };
    let generated = wsdist_fixtures::to_value(&observation).unwrap();
    assert_eq!(generated.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["id", "numbers", "selected"]);
    assert_eq!(wsdist_fixtures::from_value::<TypedCase>(&generated).unwrap(), observation);
    assert_eq!(generated["numbers"][2].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(wsdist_fixtures::to_value(nonfinite).unwrap().is_null());
    }
    assert_eq!(wsdist_fixtures::to_value(-0.0_f64).unwrap().as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
    assert!(wsdist_fixtures::to_value(u128::MAX).is_err());
    assert!(wsdist_fixtures::to_value(BTreeMap::from([(vec![1_u8], 2)])).is_err());
}

#[test]
fn typed_numeric_keys_do_not_collapse_noncanonical_json_spellings() {
    let value = native(r#"{"01":11,"1":22}"#);
    let result = wsdist_fixtures::from_value::<BTreeMap<u32, u32>>(&value);
    assert!(result.is_err(), "noncanonical keys must not silently merge: {result:?}");
}

#[test]
fn object_mutation_and_duplicate_json_keys_preserve_the_surviving_key_order() {
    let mut value = native(r#"{"z":1,"a":2,"z":3,"last":4}"#);
    assert_eq!(value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["z", "a", "last"]);
    assert_eq!(value["z"], 3);
    assert!(value["missing"].is_null());
    assert!(value.get("missing").is_none());
    value["a"] = Value::from(7);
    value["new"] = Value::Array(vec![Value::from("before")]);
    value["new"][0] = Value::from("after");
    value.get_mut("new").unwrap().as_array_mut().unwrap().push(Value::Bool(true));
    assert_eq!(value["new"][0], "after");
    assert_eq!(value["new"][1], true);
    assert_eq!(value.as_object_mut().unwrap().shift_remove("z"), Some(Value::from(3)));
    assert_eq!(value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["a", "last", "new"]);
    assert_eq!(value["a"].take(), Value::from(7));
    assert!(value["a"].is_null());
    let mut empty = Value::default();
    empty["created"] = Value::Bool(true);
    assert_eq!(empty.as_object().unwrap().len(), 1);
    let (bytes, hash) = wsdist_fixtures::encode_value(&value).unwrap();
    let decoded = wsdist_fixtures::decode_value(&bytes, Some(&hash)).unwrap();
    assert_eq!(decoded.as_object().unwrap().keys().collect::<Vec<_>>(), value.as_object().unwrap().keys().collect::<Vec<_>>());
}

#[derive(Debug, Deserialize)]
struct Projection { id: String }

#[test]
fn readable_metadata_fully_validates_discarded_fields_and_complete_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("index.json");
    for source in [
        r#"{"id":"ok","discard":1e400}"#,
        r#"{"id":"ok","discard":"\uD800"}"#,
        r#"{"id":"ok","discard":[1,]}"#,
        r#"{"id":"ok","discard":{"unfinished":true}"#,
        r#"{"id":"ok"} null"#,
    ] {
        fs::write(&path, source).unwrap();
        let error = wsdist_fixtures::read_json::<Projection>(&path).unwrap_err().to_string();
        assert!(error.contains(path.to_str().unwrap()), "{error}");
        assert!(wsdist_fixtures::read_json_value(&path).is_err());
    }
    fs::write(&path, b"{\"id\":\"ok\",\"discard\":\"\xff\"}").unwrap();
    assert!(wsdist_fixtures::read_json::<Projection>(&path).is_err());
    fs::write(&path, r#"{"id":"ok","discard":{"array":[null,false,-0.0,"雪"]}}"#).unwrap();
    assert_eq!(wsdist_fixtures::read_json::<Projection>(&path).unwrap().id, "ok");
}

#[test]
fn fixture_reader_never_sniffs_json_or_old_multipart_descriptors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("case.bitcode.zst");
    for source in [
        ORIGINAL.as_bytes(), b"null",
        br#"{"fixture_format":"byte-parts-v2","content_xxh3_128":"dc32537e27486e2c652e7b35ccc22eda","byte_length":4,"parts":[]}"#,
    ] {
        fs::write(&path, source).unwrap();
        let error = wsdist_fixtures::read_value(&path, None).unwrap_err().to_string();
        assert!(error.contains(path.to_str().unwrap()), "{error}");
        assert!(wsdist_fixtures::read::<Projection>(&path, None).is_err());
        assert!(wsdist_fixtures::decode_value(source, None).is_err());
    }
    let binary = stored(&native(ORIGINAL));
    assert!(wsdist_fixtures::read_json_value(&binary.path).is_err());
}

#[test]
fn writer_emits_one_standard_content_sized_checksummed_frame_with_independent_payload() {
    let value = native(ORIGINAL);
    let (bytes, hash) = wsdist_fixtures::encode_value(&value).unwrap();
    let decoded = zstd::stream::decode_all(bytes.as_slice()).unwrap();
    assert_eq!(&decoded[..8], b"WSFIX\0\x01\0");
    let length = u64::from_le_bytes(decoded[8..16].try_into().unwrap());
    assert_eq!(length, (decoded.len() - HEADER_BYTES) as u64);
    assert_eq!(zstd::zstd_safe::get_frame_content_size(&bytes).unwrap(), Some(decoded.len() as u64));
    assert_eq!(zstd::zstd_safe::find_frame_compressed_size(&bytes).unwrap(), bytes.len());
    let payload = &decoded[HEADER_BYTES..];
    assert_eq!(format!("{:032x}", wsdist_fixtures::content_hash(payload)), hash);
    assert_eq!(u128::from_le_bytes(decoded[16..32].try_into().unwrap()), wsdist_fixtures::content_hash(payload));
    assert_original(&bitcode::deserialize::<Value>(payload).unwrap());
    // Corrupt only the frame checksum: the independent payload hash is untouched.
    let mut corrupted_checksum = bytes;
    *corrupted_checksum.last_mut().unwrap() ^= 1;
    assert!(wsdist_fixtures::decode_value(&corrupted_checksum, Some(&hash)).is_err());
}

#[test]
fn raw_xxh3_and_index_digest_require_the_full_canonical_128_bits() {
    assert_eq!(format!("{:032x}", wsdist_fixtures::content_hash(b"null")), "dc32537e27486e2c652e7b35ccc22eda");
    let (bytes, hash) = wsdist_fixtures::encode_value(&Value::Null).unwrap();
    let high_half_changed = u128::from_str_radix(&hash, 16).unwrap() ^ (1_u128 << 64);
    for (label, invalid) in [
        ("high half", format!("{high_half_changed:032x}")),
        ("uppercase", hash.to_ascii_uppercase()),
        ("short", hash[..31].to_owned()),
        ("padded", format!("{:064x}", u128::from_str_radix(&hash, 16).unwrap())),
        ("nonhex", format!("g{}", &hash[1..])),
    ] {
        assert!(wsdist_fixtures::decode_value(&bytes, Some(&invalid)).is_err(), "{label}");
    }
    assert!(wsdist_fixtures::decode_value(&bytes, Some(&hash)).is_ok());
}

#[test]
fn header_and_index_hashes_are_checked_independently_even_after_valid_recompression() {
    let original = native(r#"{"id":"original","value":-0.0}"#);
    let replacement = native(r#"{"id":"modified","value":-0.0}"#);
    let (_, original_hash) = wsdist_fixtures::encode_value(&original).unwrap();
    let (replacement_bytes, replacement_hash) = wsdist_fixtures::encode_value(&replacement).unwrap();
    assert_eq!(wsdist_fixtures::decode_value(&replacement_bytes, Some(&replacement_hash)).unwrap()["id"], "modified");
    assert!(wsdist_fixtures::decode_value(&replacement_bytes, Some(&original_hash)).is_err());
    let mut wrong_header = document_for(&original);
    wrong_header[16] ^= 1;
    let valid_frame = compressed(&wrong_header, true, true);
    assert!(wsdist_fixtures::decode_value(&valid_frame, Some(&original_hash)).is_err());
    assert!(wsdist_fixtures::decode_value(&valid_frame, None).is_err());
}

#[test]
fn unknown_versions_declared_lengths_invalid_bitcode_and_bitcode_trailing_bytes_reject() {
    let value = native(ORIGINAL);
    let valid = document_for(&value);
    for version in [0, 2, 255] {
        let mut changed = valid.clone();
        changed[6] = version;
        assert!(wsdist_fixtures::decode_value(&compressed(&changed, true, true), None).is_err());
    }
    let length = (valid.len() - HEADER_BYTES) as u64;
    for declared in [0, length - 1, length + 1, wsdist_fixtures::MAX_PAYLOAD_BYTES + 1, u64::MAX] {
        let mut changed = valid.clone();
        changed[8..16].copy_from_slice(&declared.to_le_bytes());
        assert!(wsdist_fixtures::decode_value(&compressed(&changed, true, true), None).is_err(), "declared length {declared}");
    }
    let mut missing_payload = valid.clone();
    missing_payload.pop();
    assert!(wsdist_fixtures::decode_value(&compressed(&missing_payload, true, true), None).is_err());
    let mut extra_payload = valid;
    extra_payload.push(0);
    assert!(wsdist_fixtures::decode_value(&compressed(&extra_payload, true, true), None).is_err());
    // These frames have correct length and payload hash, but are not one bitcode value.
    for payload in [Vec::new(), vec![255], {
        let mut payload = bitcode::serialize(&value).unwrap();
        payload.push(0);
        payload
    }] {
        assert!(wsdist_fixtures::decode_value(&compressed(&document(&payload), true, true), None).is_err());
    }
}

#[test]
fn truncation_missing_frame_flags_and_every_kind_of_trailing_data_reject() {
    let value = native(r#"{"text":"a short fixture","array":[1,false,null]}"#);
    let (bytes, _) = wsdist_fixtures::encode_value(&value).unwrap();
    for end in 0..bytes.len() {
        assert!(wsdist_fixtures::decode_value(&bytes[..end], None).is_err(), "truncated at {end}");
    }
    for suffix in [vec![0], b"trailing".to_vec(), bytes.clone(), b"\x50\x2a\x4d\x18\0\0\0\0".to_vec()] {
        let mut changed = bytes.clone();
        changed.extend_from_slice(&suffix);
        assert!(wsdist_fixtures::decode_value(&changed, None).is_err());
    }
    let document = document_for(&value);
    assert!(wsdist_fixtures::decode_value(&compressed(&document, false, true), None).is_err());
    assert!(wsdist_fixtures::decode_value(&compressed(&document, true, false), None).is_err());
    let mut skippable_prefix = b"\x50\x2a\x4d\x18\0\0\0\0".to_vec();
    skippable_prefix.extend_from_slice(&bytes);
    assert!(wsdist_fixtures::decode_value(&skippable_prefix, None).is_err());
}

#[test]
fn payload_limit_and_depth_limit_reject_before_large_or_unbounded_materialization() {
    assert_eq!(wsdist_fixtures::MAX_PAYLOAD_BYTES, 512 * 1024 * 1024);
    // Standard single-segment zstd header: 8-byte content size and checksum flag.
    // No large body is needed: the advertised size alone must fail the limit.
    let mut oversized = b"\x28\xb5\x2f\xfd\xe4".to_vec();
    oversized.extend_from_slice(&(wsdist_fixtures::MAX_PAYLOAD_BYTES + HEADER_BYTES as u64 + 1).to_le_bytes());
    let error = wsdist_fixtures::decode_value(&oversized, None).unwrap_err().to_string();
    assert!(error.contains("512 MiB"), "{error}");
    let mut deep = Value::Null;
    for _ in 0..129 { deep = Value::Array(vec![deep]); }
    assert!(wsdist_fixtures::encode_value(&deep).is_err());
    assert!(wsdist_fixtures::to_value(&deep).is_err());
    let bytes = compressed(&document_for(&deep), true, true);
    assert!(wsdist_fixtures::decode_value(&bytes, None).is_err());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deep.json");
    fs::write(&path, format!("{{\"id\":\"ok\",\"discard\":{}null{}}}", "[".repeat(129), "]".repeat(129))).unwrap();
    assert!(wsdist_fixtures::read_json::<Projection>(&path).is_err());
}

#[test]
fn empty_arrays_objects_and_nul_strings_are_valid_native_storage_values() {
    for value in [Value::Null, Value::Array(Vec::new()), Value::Object(Map::new()), Value::String("a\0雪".to_owned())] {
        let (bytes, hash) = wsdist_fixtures::encode_value(&value).unwrap();
        assert_eq!(wsdist_fixtures::decode_value(&bytes, Some(&hash)).unwrap(), value);
    }
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Float32Key(u32);

impl<'de> Deserialize<'de> for Float32Key {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        f32::deserialize(deserializer).map(|value| Self(value.to_bits()))
    }
}

#[test]
fn typed_float_keys_reject_finite_values_outside_the_requested_float_range() {
    let value = native(r#"{"340282366920938463463374607431768211455":1}"#);
    assert!(wsdist_fixtures::from_value::<BTreeMap<Float32Key, u8>>(&value).is_err());
}
