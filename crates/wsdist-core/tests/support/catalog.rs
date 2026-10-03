use std::collections::HashMap;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wsdist_core::catalog::{BuffTableKind, BuffValue, ROLL_NUMERALS};
use wsdist_core::{Catalog, Slot};

use crate::player_support;

fn exact(actual: f64, expected: &Value, path: &str) {
    crate::kernel_support::assert_value(&json!(actual), expected, true, path);
}

pub fn hash_file(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    format!("{:x}", Sha256::digest(&bytes))
}

fn ordered(actual: &Value, expected: &Value, path: &str) {
    assert_eq!(actual, expected, "{path}: complete source value");
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>(), "{path}: key order");
            for (key, actual) in actual { ordered(actual, &expected[key], &format!("{path}.{key}")); }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() { ordered(actual, expected, &format!("{path}[{index}]")); }
        }
        _ => {}
    }
}

fn buff(actual: BuffValue, expected: &Value, path: &str) {
    match actual {
        BuffValue::Constant(value) => exact(value, expected, path),
        BuffValue::Scaling(value) => { exact(value.base, &expected["base"], path); exact(value.per_point, &expected["per_point"], path); }
        BuffValue::CorRoll(value) => {
            for (index, numeral) in ROLL_NUMERALS.iter().enumerate() { exact(value.potency[index], &expected["potency"][numeral], path); }
            exact(value.per_roll_bonus, &expected["per_roll_bonus"], path);
            exact(value.job_bonus, &expected["job_bonus"], path);
        }
    }
}

pub fn verify(root: &Path, catalog: &Catalog, manifest: &Value) -> Value {
    let mut hashes = serde_json::Map::new();
    for (name, expected) in manifest["catalog_hashes"].as_object().unwrap() {
        let hash = hash_file(&root.join("data").join(name));
        assert_eq!(&hash, expected.as_str().unwrap(), "data/{name}: immutable exported catalog");
        hashes.insert(format!("data/{name}"), json!(hash));
    }
    let gear = player_support::document(&root.join("data/gear.json"));
    let pieces = gear["pieces"].as_array().unwrap();
    assert_eq!(pieces.len(), catalog.pieces().len(), "every exported original gear identity");
    for (index, (actual, expected)) in catalog.pieces().iter().zip(pieces).enumerate() {
        assert_eq!(actual.id.index(), index, "catalog index/order");
        player_support::assert_piece(actual, None, expected, &format!("catalog.pieces[{index}]"));
        assert_eq!(catalog.by_original_id(actual.original_id), Some(actual.id), "original identity lookup");
    }
    let mut names = HashMap::new();
    let mut memberships = 0;
    for &slot in Slot::ALL {
        let expected = gear["slots"][slot.name()].as_array().unwrap().iter().map(|id| id.as_u64().unwrap() as u32).collect::<Vec<_>>();
        let actual = catalog.slot(slot).iter().map(|id| catalog.piece(*id).original_id).collect::<Vec<_>>();
        assert_eq!(actual, expected, "catalog.{}: ordered repeated membership", slot.name());
        memberships += actual.len();
        for &id in catalog.slot(slot) { names.insert(catalog.piece(id).name2.as_ref(), id); }
    }
    for piece in catalog.pieces() { assert_eq!(catalog.by_name2(&piece.name2), names.get(piece.name2.as_ref()).copied(), "{}: last-value-wins name2 index", piece.name2); }
    assert_eq!(catalog.symbols().len(), gear["symbols"].as_object().unwrap().len());
    for (actual, (name, expected)) in catalog.symbols().iter().zip(gear["symbols"].as_object().unwrap()) {
        assert_eq!(actual.name.as_ref(), name);
        assert_eq!(catalog.piece(actual.id).original_id as u64, expected.as_u64().unwrap());
        assert_eq!(catalog.by_symbol(name), Some(actual.id));
    }
    assert_eq!(catalog.foods().iter().map(|id| catalog.piece(*id).original_id as u64).collect::<Vec<_>>(),
        gear["foods"].as_array().unwrap().iter().map(|id| id.as_u64().unwrap()).collect::<Vec<_>>(), "food identity/order/repeats");

    let enemies = player_support::document(&root.join("data/enemies.json"));
    let records = enemies["enemies"].as_array().unwrap();
    assert_eq!(catalog.enemies().len(), records.len());
    for (index, (actual, expected)) in catalog.enemies().iter().zip(records).enumerate() {
        assert_eq!(actual.name.as_ref(), expected["Name"].as_str().unwrap());
        assert_eq!(actual.location.as_ref(), expected["Location"].as_str().unwrap());
        exact(actual.level, &expected["Level"], &format!("catalog.enemies[{index}].Level"));
        let stats = expected.as_object().unwrap().iter().filter(|(name, _)| !matches!(name.as_str(), "Name" | "Location")).collect::<Vec<_>>();
        assert_eq!(actual.stats.len(), stats.len());
        for (actual, (name, expected)) in actual.stats.iter().zip(stats) {
            assert_eq!(actual.stat.name(), name);
            exact(actual.value, expected, &format!("catalog.enemies[{index}].{name}"));
            assert_eq!(&actual.literal, expected.as_number().unwrap());
        }
    }
    assert_eq!(json!(catalog.enemy_menu().iter().map(|id| catalog.enemy(*id).name.as_ref()).collect::<Vec<_>>()), manifest["export"]["enemy_menu"], "stable descending level/name menu");

    let buffs = player_support::document(&root.join("data/buffs.json"));
    let mut buff_rows = 0;
    for kind in BuffTableKind::ALL {
        let table = catalog.buffs().table(kind);
        let expected = buffs[kind.name()].as_object().unwrap();
        assert_eq!(table.entries().len(), expected.len());
        for (index, (actual, (name, expected))) in table.entries().iter().zip(expected).enumerate() {
            assert_eq!(actual.name.as_ref(), name);
            assert_eq!(table.index_of(name), Some(index));
            assert_eq!(actual.level_requirement, expected.get("level requirement").and_then(Value::as_f64));
            assert_eq!(actual.job_requirement.as_ref().map(|jobs| jobs.iter().map(|job| job.name()).collect::<Vec<_>>()),
                expected.get("job requirement").map(|jobs| jobs.as_array().unwrap().iter().map(|job| job.as_str().unwrap()).collect::<Vec<_>>()));
            let stats = expected.as_object().unwrap().iter().filter(|(name, _)| !matches!(name.as_str(), "level requirement" | "job requirement")).collect::<Vec<_>>();
            assert_eq!(actual.stats.len(), stats.len());
            for (actual, (stat, expected)) in actual.stats.iter().zip(stats) {
                assert_eq!(actual.stat.name(), stat);
                buff(actual.value, expected, &format!("catalog.buffs.{}.{name}.{stat}", kind.name()));
            }
            assert_eq!(actual.song_bonus_cap, if kind == BuffTableKind::Brd { buffs["brd_song_limits"][name].as_f64() } else { None });
            buff_rows += 1;
        }
    }
    let limits = buffs["brd_song_limits"].as_object().unwrap();
    assert_eq!(catalog.buffs().song_limits().len(), limits.len());
    for (actual, (name, expected)) in catalog.buffs().song_limits().iter().zip(limits) {
        assert_eq!(catalog.buffs().table(BuffTableKind::Brd).entries()[actual.song_index].name.as_ref(), name);
        exact(actual.cap, expected, &format!("catalog.song_limits.{name}"));
    }
    let attachments = player_support::document(&root.join("data/pup-attachments.json"));
    let records = attachments["attachments"].as_array().unwrap();
    assert_eq!(catalog.attachments().len(), records.len());
    for (index, (actual, expected)) in catalog.attachments().iter().zip(records).enumerate() {
        ordered(actual, expected, &format!("catalog.attachments[{index}]"));
        ordered(catalog.attachment_by_name(expected["Name"].as_str().unwrap()).unwrap(), expected, &format!("catalog.attachments[{index}].lookup"));
    }
    let frozen_csv = manifest["export"]["csv_rows"].as_array().unwrap();
    assert_eq!(frozen_csv.first().unwrap(), &json!(["id", "name", "name2"]), "legacy CSV header");
    let csv_rows = &frozen_csv[1..];
    assert_eq!(catalog.aliases().len(), csv_rows.len());
    let mut full = HashMap::new();
    let mut abbreviations = HashMap::new();
    for (index, (actual, expected)) in catalog.aliases().iter().zip(csv_rows).enumerate() {
        assert_eq!(json!([actual.id.as_ref(), actual.name.as_ref(), actual.name2.as_ref()]), *expected, "catalog.csv[{index}]");
        full.entry(actual.name.to_lowercase()).or_insert(index);
        abbreviations.entry(actual.name2.to_lowercase()).or_insert(index);
    }
    for (name, index) in full { assert!(std::ptr::eq(catalog.alias_by_full_name(&name).unwrap(), &catalog.aliases()[index]), "first full-name match: {name}"); }
    for (name, index) in abbreviations { assert!(std::ptr::eq(catalog.alias_by_abbreviation(&name).unwrap(), &catalog.aliases()[index]), "first abbreviation match: {name}"); }
    let mut assets = 0;
    for (name, expected) in manifest["export"]["asset_hashes"].as_object().unwrap() {
        assert_eq!(hash_file(&root.join(name)), expected.as_str().unwrap(), "asset {name}");
        assets += 1;
    }
    json!({"hashes": hashes, "pieces": pieces.len(), "slot_memberships": memberships,
        "foods": catalog.foods().len(), "symbols": catalog.symbols().len(), "enemies": catalog.enemies().len(),
        "buff_rows": buff_rows, "attachments": catalog.attachments().len(), "csv_rows": csv_rows.len(), "asset_hashes_checked": assets})
}
