use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{Value, json};
use wsdist_fixtures::{Value as FixtureValue, to_value};
use sha2::{Digest, Sha256};
use wsdist_core::catalog::{BuffTableKind, BuffValue, ROLL_NUMERALS};
use wsdist_core::{Catalog, Slot};

use crate::player_support;

fn exact(actual: f64, expected: &FixtureValue, path: &str) {
    crate::kernel_support::assert_value(&to_value(actual).unwrap(), expected, true, path);
}

pub fn hash_file(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    format!("{:x}", Sha256::digest(&bytes))
}

fn ordered(actual: &Value, expected: &FixtureValue, path: &str) {
    assert_eq!(actual, expected, "{path}: complete source value");
    match (actual, expected) {
        (Value::Object(actual), FixtureValue::Object(expected)) => {
            assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>(), "{path}: key order");
            for (key, actual) in actual { ordered(actual, &expected[key], &format!("{path}.{key}")); }
        }
        (Value::Array(actual), FixtureValue::Array(expected)) => {
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() { ordered(actual, expected, &format!("{path}[{index}]")); }
        }
        _ => {}
    }
}

fn buff(actual: BuffValue, expected: &FixtureValue, path: &str) {
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

pub fn verify(root: &Path, catalog: &Catalog, manifest: &FixtureValue, frozen: &FixtureValue) -> Value {
    let mut hashes = serde_json::Map::new();
    for (name, expected) in manifest["catalog_hashes"].as_object().unwrap() {
        if name == "gear.json" { continue; }
        let name = Path::new(name).with_extension("json5");
        let source = player_support::document_catalog(&root.join("data").join(&name));
        // Reconstruct the frozen export's ordered JSON, including numeric kinds.
        // JSON5 comments, key quoting and trailing commas are not catalog changes.
        let mut exported_json = serde_json::to_vec_pretty(&source).unwrap();
        exported_json.push(b'\n');
        let hash = format!("{:x}", Sha256::digest(&exported_json));
        assert_eq!(&hash, expected.as_str().unwrap(), "data/{}: immutable exported catalog values/order", name.display());
        hashes.insert(format!("data/{}", name.display()), json!(hash));
    }
    // The frozen export is value evidence, not an authored-ID or source-layout
    // contract. Match the full record multiset, including dormant duplicates.
    let gear = &frozen["documents"]["gear.json"];
    let pieces = gear["pieces"].as_array().unwrap();
    assert_eq!(pieces.len(), catalog.pieces().len(), "all captured gear records, including duplicate declarations");
    let resolved = player_support::resolve_piece_occurrences(catalog, pieces, "catalog.gear");
    let mut stat_literals = 0;
    for (index, (&id, expected)) in resolved.iter().zip(pieces).enumerate() {
        player_support::assert_piece(catalog.piece(id), None, expected, &format!("catalog.gear[{index}]"));
        stat_literals += catalog.piece(id).stats.len();
    }
    let captured_membership = |values: &FixtureValue| {
        values.as_array().unwrap().iter().map(|captured| {
            // This relation exists entirely inside the immutable captured
            // document; production lookup uses the already matched snapshot.
            let index = pieces.iter().position(|piece| piece["id"] == *captured).unwrap();
            resolved[index]
        }).collect::<Vec<_>>()
    };
    let mut memberships = 0;
    let mut listed = HashSet::new();
    for &slot in Slot::ALL {
        let mut expected = captured_membership(&gear["slots"][slot.name()]);
        let mut actual = catalog.slot(slot).to_vec();
        expected.sort_by_key(|id| id.index());
        actual.sort_by_key(|id| id.index());
        assert_eq!(actual, expected, "catalog.{}: captured membership without incidental source order", slot.name());
        memberships += actual.len();
        listed.extend(actual);
    }
    let mut expected_foods = captured_membership(&gear["foods"]);
    let mut actual_foods = catalog.foods().to_vec();
    expected_foods.sort_by_key(|id| id.index());
    actual_foods.sort_by_key(|id| id.index());
    assert_eq!(actual_foods, expected_foods, "captured food membership");
    listed.extend(actual_foods);
    let declarations = catalog.pieces().len() - listed.len();

    let enemies = player_support::document_catalog(&root.join("data/enemies.json5"));
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
            assert_eq!(to_value(&actual.literal).unwrap(), *expected);
        }
    }
    assert_eq!(to_value(catalog.enemy_menu().iter().map(|id| catalog.enemy(*id).name.as_ref()).collect::<Vec<_>>()).unwrap(), manifest["export"]["enemy_menu"], "stable descending level/name menu");

    let buffs = player_support::document_catalog(&root.join("data/buffs.json5"));
    let mut buff_rows = 0;
    for kind in BuffTableKind::ALL {
        let table = catalog.buffs().table(kind);
        let expected = buffs[kind.name()].as_object().unwrap();
        assert_eq!(table.entries().len(), expected.len());
        for (index, (actual, (name, expected))) in table.entries().iter().zip(expected).enumerate() {
            assert_eq!(actual.name.as_ref(), name);
            assert_eq!(table.index_of(name), Some(index));
            assert_eq!(actual.level_requirement, expected.get("level requirement").and_then(FixtureValue::as_f64));
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
    let attachments = player_support::document_catalog(&root.join("data/pup-attachments.json5"));
    let records = attachments["attachments"].as_array().unwrap();
    assert_eq!(catalog.attachments().len(), records.len());
    for (index, (actual, expected)) in catalog.attachments().iter().zip(records).enumerate() {
        ordered(actual, expected, &format!("catalog.attachments[{index}]"));
        ordered(catalog.attachment_by_name(expected["Name"].as_str().unwrap()).unwrap(), expected, &format!("catalog.attachments[{index}].lookup"));
    }
    let frozen_csv = manifest["export"]["csv_rows"].as_array().unwrap();
    assert_eq!(frozen_csv.first().unwrap(), &to_value(["id", "name", "name2"]).unwrap(), "legacy CSV header");
    let csv_rows = &frozen_csv[1..];
    assert_eq!(catalog.aliases().len(), csv_rows.len());
    let mut full = HashMap::new();
    let mut abbreviations = HashMap::new();
    for (index, (actual, expected)) in catalog.aliases().iter().zip(csv_rows).enumerate() {
        assert_eq!(to_value([actual.id.as_ref(), actual.name.as_ref(), actual.name2.as_ref()]).unwrap(), *expected, "catalog.csv[{index}]");
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
    json!({"canonical_json_sha256": hashes, "pieces": pieces.len(), "slot_memberships": memberships,
        "foods": catalog.foods().len(), "gear_values": {"record_snapshots_checked": pieces.len(),
            "ordered_stat_literals_checked": stat_literals, "declaration_only_records_checked": declarations,
            "identity_contract": "complete captured snapshot multiset and selectable membership; no legacy-ID, file-layout or record-order assertion"},
        "enemies": catalog.enemies().len(), "buff_rows": buff_rows,
        "attachments": catalog.attachments().len(), "csv_rows": csv_rows.len(), "asset_hashes_checked": assets})
}
