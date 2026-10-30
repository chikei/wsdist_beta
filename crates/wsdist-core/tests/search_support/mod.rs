use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde::Deserialize;
use wsdist_fixtures::Value;
use wsdist_core::model::{Abilities, CalcError, Enemy, EvaluationContext, GearId, Gearset, ModelState, Player, PlayerInput, Slot};
use wsdist_core::rng::RandomSources;
use wsdist_core::Catalog;

#[path = "../legacy_player/support.rs"]
#[allow(dead_code)]
mod player_support;
pub use player_support::{assert_number, document_json, number};
pub use player_support::observation;

#[derive(Deserialize)]
pub struct IndexedCase { pub id: String, pub operation: String, pub path: String, pub source_path: String, pub source_sha256: String, pub content_xxh3_128: String }
#[derive(Deserialize)]
pub struct FixtureIndex { pub schema_version: u32, pub cases: Vec<IndexedCase> }

pub fn resource_root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
pub fn fixture_root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy") }
pub fn index() -> FixtureIndex {
    let index: FixtureIndex = wsdist_fixtures::read_json(&fixture_root().join("search_simulation.json")).unwrap();
    assert_eq!(index.schema_version, 3, "search fixture storage schema");
    index
}
pub fn fixture(root: &Path, entry: &IndexedCase) -> Value {
    wsdist_fixtures::read_value(&root.join(&entry.path), Some(&entry.content_xxh3_128))
        .unwrap_or_else(|error| panic!("{error}"))
}

/// Canonical and oracle-local records are loaded by the production JSON loader.
/// Only the local records' absent source IDs gain test-local inert catalog IDs.
/// They never enter the application's name/slot indices or alter the shipped data.
pub struct FrozenCatalog {
    pub catalog: Arc<Catalog>,
    locals: HashMap<String, GearId>,
}

impl FrozenCatalog {
    pub fn from_input(input: &Value) -> Self {
        fn collect(value: &Value, records: &mut BTreeMap<String, Value>) {
            match value {
                Value::Object(object) => {
                    if let Some(identity) = object.get("local_identity").and_then(Value::as_str) {
                        if object.contains_key("name") && object.contains_key("stats") && object.contains_key("jobs") {
                            if let Some(previous) = records.insert(identity.to_owned(), value.clone()) {
                                assert_eq!(previous, *value, "one local identity must retain one original record");
                            }
                        }
                    }
                    for child in object.values() { collect(child, records); }
                }
                Value::Array(values) => for child in values { collect(child, records); },
                _ => {}
            }
        }
        static CATALOGS: LazyLock<Mutex<HashMap<String, Arc<Catalog>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
        let mut records = BTreeMap::new();
        collect(input, &mut records);
        let key = serde_json::to_string(&records).unwrap();
        let mut cache = CATALOGS.lock();
        let catalog = Arc::clone(cache.entry(key).or_insert_with(|| {
            if records.is_empty() { return Arc::new(Catalog::load(&resource_root()).unwrap()); }
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("data")).unwrap();
            for name in ["enemies.json", "buffs.json", "pup-attachments.json"] {
                fs::copy(resource_root().join("data").join(name), root.path().join("data").join(name)).unwrap();
            }
            fs::copy(resource_root().join("item_list.csv"), root.path().join("item_list.csv")).unwrap();
            let mut gear = document_json(&resource_root().join("data/gear.json"));
            let pieces = gear["pieces"].as_array_mut().unwrap();
            for (index, record) in records.values().enumerate() {
                let mut record = record.clone();
                let object = record.as_object_mut().unwrap();
                object.shift_remove("local_identity");
                object.insert("id".to_owned(), Value::from(8_000_000_u32 + index as u32));
                pieces.push(record);
            }
            fs::write(root.path().join("data/gear.json"), serde_json::to_vec(&gear).unwrap()).unwrap();
            Arc::new(Catalog::load(root.path()).unwrap())
        }));
        let locals: HashMap<_, _> = records.keys().enumerate().map(|(index, identity)| {
            (identity.clone(), catalog.by_original_id(8_000_000_u32 + index as u32).unwrap())
        }).collect();
        Self { catalog, locals }
    }

    pub fn gear(&self, value: &Value) -> GearId {
        let value = value.get("original").unwrap_or(value);
        if let Some(identity) = value.get("local_identity").and_then(Value::as_str) {
            *self.locals.get(identity).unwrap_or_else(|| panic!("unregistered local identity {identity}"))
        } else {
            self.catalog.by_original_id(value["id"].as_u64().expect("canonical original ID") as u32).unwrap()
        }
    }

    pub fn gearset(&self, value: &Value) -> Gearset {
        std::array::from_fn(|index| self.gear(&value[Slot::ALL[index].name()]))
    }

    pub fn player(&self, value: &Value, state: &ModelState, label: &str) -> PlayerInput {
        let mut normalized = value.clone();
        for slot in Slot::ALL {
            let record = &mut normalized["gearset"][slot.name()];
            self.normalize_record(record);
        }
        player_support::parse_input(&normalized, &self.catalog, state, label)
    }

    fn normalize_record(&self, value: &mut Value) {
        if value.get("original").is_some() {
            self.normalize_record(&mut value["original"]);
            self.normalize_record(&mut value["effective"]);
        } else if let Some(identity) = value.get("local_identity").and_then(Value::as_str) {
            let id = self.locals[identity];
            let object = value.as_object_mut().unwrap();
            object.shift_remove("local_identity");
            object.insert("id".to_owned(), Value::from(self.catalog.piece(id).original_id));
        }
    }

    pub fn context(&self, seeds: &Value) -> EvaluationContext<RandomSources> {
        EvaluationContext::new(Arc::clone(&self.catalog), RandomSources::from_seed(seeds["numpy_host"].as_u64().unwrap()))
    }

    pub fn assert_piece(&self, actual: GearId, state: Option<&ModelState>, source: &Value, label: &str) {
        let piece = self.catalog.piece(actual);
        if source.get("id").is_none() {
            // Choice records and unpickled workers lose source identity. The
            // actual input/job already selected the piece; compare every field.
            let mut normalized = source.clone();
            normalized.as_object_mut().unwrap().shift_remove("local_identity");
            normalized["id"] = Value::from(piece.original_id);
            player_support::assert_piece(piece, state, &normalized, label);
        } else {
            player_support::assert_piece(piece, state, source, label);
        }
    }

    pub fn assert_gearset(&self, actual: &Gearset, state: &ModelState, expected: &Value, label: &str) {
        for slot in Slot::ALL {
            let id = actual[slot.index()];
            let source = &expected[slot.name()];
            let source = source.get("effective").unwrap_or(source);
            self.assert_piece(id, Some(state), source, &format!("{label}.{}", slot.name()));
        }
    }

    pub fn assert_player(&self, actual: &Player, state: &ModelState, expected: &Value, label: &str) {
        player_support::assert_metadata(actual, expected, label);
        player_support::assert_sheet(&actual.stats, &actual.wsc, actual.wyvern_bonus_attack, expected, label);
        if let Some(abilities) = expected.get("abilities") {
            let expected: Abilities = wsdist_fixtures::from_value(abilities).unwrap();
            assert_eq!(wsdist_fixtures::to_value(&actual.abilities).unwrap(), wsdist_fixtures::to_value(expected).unwrap(), "{label}: effective abilities");
        }
        self.assert_gearset(&actual.gearset, state, &expected["gearset"], label);
    }
}

pub fn enemy(value: &Value) -> Enemy {
    Enemy {
        name: Arc::from(value.get("name").and_then(Value::as_str).unwrap_or("captured enemy")),
        location: Arc::from(value.get("location").and_then(Value::as_str).unwrap_or("captured")),
        stats: wsdist_fixtures::from_value(&value["normalized_stats"]).unwrap(),
    }
}

pub fn assert_error(error: &CalcError, expected: &Value, label: &str) {
    let CalcError::Legacy { category, message, .. } = error else { panic!("{label}: expected source error, got {error}") };
    assert_eq!(*category, expected["class"].as_str().unwrap(), "{label}: exception category");
    assert_eq!(message, expected["message"].as_str().unwrap(), "{label}: exception message");
}
