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

/// Canonical and oracle-local records use the production fragment loader.
/// Local identities belong only to this captured case; records remain unlisted
/// and retain their complete values without authored catalog IDs.
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
        static BASE: LazyLock<Arc<Catalog>> = LazyLock::new(|| Arc::new(Catalog::load(&resource_root()).unwrap()));
        static BASE_SNAPSHOTS: LazyLock<Vec<Value>> = LazyLock::new(|| BASE.pieces().iter().map(player_support::piece_snapshot).collect());
        let mut records = BTreeMap::new();
        collect(input, &mut records);
        let key = serde_json::to_string(&records).unwrap();
        let mut cache = CATALOGS.lock();
        let catalog = Arc::clone(cache.entry(key).or_insert_with(|| {
            if records.is_empty() { return Arc::clone(&BASE); }
            let root = tempfile::tempdir().unwrap();
            player_support::copy_resources(root.path());
            let pieces = records.values().map(|record| {
                let mut record = record.clone();
                let object = record.as_object_mut().unwrap();
                object.shift_remove("local_identity");
                object.shift_remove("id");
                object.insert("slots".to_owned(), Value::Array(Vec::new()));
                record
            }).collect::<Vec<_>>();
            // JSON is a JSON5 subset and preserves integer/float/signed-zero literals.
            let fragment = serde_json::json!({"schema_version": 2, "pieces": pieces});
            fs::write(root.path().join("data/gear/oracle-local.json5"), serde_json::to_vec(&fragment).unwrap()).unwrap();
            Arc::new(Catalog::load(root.path()).unwrap())
        }));
        // Reserve the complete canonical multiset first. A local record equal
        // to a dormant canonical declaration still requires a separate runtime
        // identity, just as it does when equal to a selectable record.
        let mut used = if records.is_empty() {
            std::collections::HashSet::new()
        } else {
            player_support::resolve_piece_occurrences(&catalog, &BASE_SNAPSHOTS, "canonical records").into_iter().collect()
        };
        let locals: HashMap<_, _> = records.iter().map(|(identity, record)| {
            let id = catalog.pieces().iter()
                .find(|piece| !used.contains(&piece.id) && player_support::piece_matches(piece, record))
                .map(|piece| piece.id)
                .unwrap_or_else(|| panic!("no distinct unlisted record for local identity {identity}"));
            used.insert(id);
            (identity.clone(), id)
        }).collect();
        Self { catalog, locals }
    }

    pub fn gear(&self, value: &Value) -> GearId {
        let value = value.get("original").unwrap_or(value);
        if let Some(identity) = value.get("local_identity").and_then(Value::as_str) {
            *self.locals.get(identity).unwrap_or_else(|| panic!("unregistered local identity {identity}"))
        } else {
            player_support::resolve_piece(&self.catalog, value, "frozen gear")
        }
    }

    pub fn gearset(&self, value: &Value) -> Gearset {
        std::array::from_fn(|index| self.gear(&value[Slot::ALL[index].name()]))
    }

    pub fn player(&self, value: &Value, state: &ModelState, label: &str) -> PlayerInput {
        player_support::parse_input_with_resolver(value, &self.catalog, state, label, |record, _| self.gear(record))
    }

    pub fn context(&self, seeds: &Value) -> EvaluationContext<RandomSources> {
        EvaluationContext::new(Arc::clone(&self.catalog), RandomSources::from_seed(seeds["numpy_host"].as_u64().unwrap()))
    }

    pub fn assert_piece(&self, actual: GearId, state: Option<&ModelState>, source: &Value, label: &str) {
        player_support::assert_piece(self.catalog.piece(actual), state, source, label);
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
