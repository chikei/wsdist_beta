use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use tempfile::TempDir;
use wsdist_core::catalog::{BuffTableKind, BuffValue, ROLL_NUMERALS};
use wsdist_core::model::ModelState;
use wsdist_core::{CalcError, Catalog, Job, Slot, Stat};

fn resource_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn document(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

fn copied_resources() -> TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join("data")).unwrap();
    for name in ["data/gear.json", "data/enemies.json", "data/buffs.json", "data/pup-attachments.json", "item_list.csv"] {
        fs::copy(resource_root().join(name), temporary.path().join(name)).unwrap();
    }
    temporary
}

fn alter(root: &Path, name: &str, change: impl FnOnce(&mut Value)) {
    let mut value = document(root, name);
    change(&mut value);
    fs::write(root.join(name), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn assert_number(actual: f64, expected: &Value) {
    assert_eq!(actual.to_bits(), expected.as_f64().unwrap().to_bits());
}

fn assert_ordered_value(actual: &Value, expected: &Value) {
    assert_eq!(actual, expected);
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
            for (key, value) in actual { assert_ordered_value(value, &expected[key]); }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            for (actual, expected) in actual.iter().zip(expected) { assert_ordered_value(actual, expected); }
        }
        _ => {}
    }
}

fn assert_data_error(root: &Path, file: &str, json_path: &str) -> String {
    match Catalog::load(root).unwrap_err() {
        CalcError::Data { path, json_path: actual, message } => {
            assert_eq!(path, root.join(file));
            assert_eq!(actual, json_path);
            message
        }
        error => panic!("expected a data error, got {error}"),
    }
}

fn append_piece(document: &mut Value, symbol: &str, id: u32, mut piece: Value) {
    piece["id"] = json!(id);
    document["pieces"].as_array_mut().unwrap().push(piece);
    document["symbols"][symbol] = json!(id);
}

#[test]
fn real_equipment_preserves_all_records_and_ordered_membership() {
    let root = resource_root();
    let source = document(&root, "data/gear.json");
    let catalog = Arc::new(Catalog::load(&root).unwrap());
    let source_pieces = source["pieces"].as_array().unwrap();
    assert_eq!(catalog.pieces().len(), source_pieces.len());
    for (index, expected) in source_pieces.iter().enumerate() {
        let original_id = expected["id"].as_u64().unwrap() as u32;
        let id = catalog.by_original_id(original_id).unwrap();
        assert_eq!(id.index(), index);
        let actual = catalog.piece(id);
        assert_eq!(actual.original_id, original_id);
        assert_eq!(actual.name.as_ref(), expected["name"].as_str().unwrap());
        assert_eq!(actual.name2.as_ref(), expected["name2"].as_str().unwrap());
        let jobs: Vec<_> = expected["jobs"].as_array().unwrap().iter().map(|job| job.as_str().unwrap()).collect();
        assert_eq!(actual.jobs.iter().map(|job| job.name()).collect::<Vec<_>>(), jobs);
        for &job in Job::ALL {
            assert_eq!(actual.eligible_for(job), jobs.contains(&job.name()));
        }
        assert_eq!(actual.kind.name(), expected["type"].as_str().unwrap());
        assert_eq!(actual.skill_type.name(), expected["skill_type"].as_str().unwrap());
        assert_number(actual.dmg, &expected["dmg"]);
        assert_number(actual.delay, &expected["delay"]);
        assert_eq!(actual.rank, expected["rank"].as_i64());
        if expected["wsc"].is_null() {
            assert_eq!(actual.wsc, None);
        } else {
            let wsc = actual.wsc.unwrap();
            assert_eq!(wsc.stat.name(), expected["wsc"][0].as_str().unwrap());
            assert_number(wsc.percent, &expected["wsc"][1]);
        }
        let stats = expected["stats"].as_object().unwrap();
        assert_eq!(actual.stats.len(), stats.len());
        for (actual, (name, expected)) in actual.stats.iter().zip(stats) {
            assert_eq!(actual.stat.name(), name);
            assert_number(actual.value, expected);
            assert_eq!(&actual.literal, expected.as_number().unwrap());
        }
    }

    let mut names = HashMap::new();
    for &slot in Slot::ALL {
        let expected: Vec<_> = source["slots"][slot.name()].as_array().unwrap().iter().map(|id| id.as_u64().unwrap() as u32).collect();
        let actual: Vec<_> = catalog.slot(slot).iter().map(|id| catalog.piece(*id).original_id).collect();
        assert_eq!(actual, expected, "{} membership", slot.name());
        for original_id in expected {
            let id = catalog.by_original_id(original_id).unwrap();
            names.insert(catalog.piece(id).name2.as_ref(), id);
        }
    }
    for piece in catalog.pieces() {
        assert_eq!(catalog.by_name2(&piece.name2), names.get(piece.name2.as_ref()).copied());
    }
    let source_symbols = source["symbols"].as_object().unwrap();
    assert_eq!(catalog.symbols().len(), source_symbols.len());
    for (actual, (name, expected)) in catalog.symbols().iter().zip(source_symbols) {
        assert_eq!(actual.name.as_ref(), name);
        assert_eq!(catalog.piece(actual.id).original_id, expected.as_u64().unwrap() as u32);
        assert_eq!(catalog.by_symbol(name), Some(actual.id));
    }
    let expected_foods: Vec<_> = source["foods"].as_array().unwrap().iter().map(|id| id.as_u64().unwrap() as u32).collect();
    assert_eq!(catalog.foods().iter().map(|id| catalog.piece(*id).original_id).collect::<Vec<_>>(), expected_foods);
    let mut food_names = HashMap::new();
    for &id in catalog.foods() { food_names.insert(catalog.piece(id).name.as_ref(), id); }
    for (name, id) in food_names { assert_eq!(catalog.food_by_name(name), Some(id)); }
    assert_eq!(catalog.piece(catalog.empty).name2.as_ref(), "Empty");
    for symbol in ["Duty_Flanchard0", "Duty_Flanchard"] {
        assert_eq!(catalog.piece(catalog.by_symbol(symbol).unwrap()).stat(Stat::Da), 8.0);
    }
}

#[test]
fn real_enemies_and_attachments_preserve_records_and_menu_order() {
    let root = resource_root();
    let catalog = Catalog::load(&root).unwrap();
    let enemies = document(&root, "data/enemies.json");
    let records = enemies["enemies"].as_array().unwrap();
    assert_eq!(catalog.enemies().len(), 11);
    assert_eq!(catalog.enemies().len(), records.len());
    for (actual, expected) in catalog.enemies().iter().zip(records) {
        assert_eq!(actual.name.as_ref(), expected["Name"].as_str().unwrap());
        assert_eq!(actual.location.as_ref(), expected["Location"].as_str().unwrap());
        assert_number(actual.level, &expected["Level"]);
        let numeric: Vec<_> = expected.as_object().unwrap().iter().filter(|(name, _)| name.as_str() != "Name" && name.as_str() != "Location").collect();
        assert_eq!(actual.stats.len(), numeric.len());
        for (actual, (name, expected)) in actual.stats.iter().zip(&numeric) {
            assert_eq!(actual.stat.name(), name.as_str());
            assert_number(actual.value, expected);
            assert_eq!(&actual.literal, expected.as_number().unwrap());
        }
        let input = actual.to_input();
        assert_eq!(input.name, actual.name);
        assert_eq!(input.location, actual.location);
        assert_eq!(input.stats.ordered().map(|(stat, _)| stat.name()).collect::<Vec<_>>(), numeric.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>());
        for contribution in &actual.stats {
            assert_eq!(actual.stat(contribution.stat), Some(contribution.value));
            assert_eq!(input.stats.get(contribution.stat).to_bits(), contribution.value.to_bits());
        }
    }
    let menu: Vec<_> = catalog.enemy_menu().iter().map(|id| catalog.enemy(*id).name.as_ref()).collect();
    assert_eq!(menu, [
        "Ozma", "Octorok", "BG Wiki sets", "Apex Archaic Cogs", "Apex Archaic Cog",
        "Apex Idle Drifter", "Apex Knight Lugcrawler", "Apex Lugcrawler Hunter",
        "Apex Bat", "Apex Toad", "Apex Bats",
    ]);
    for &id in catalog.enemy_menu() { assert_eq!(catalog.enemy_by_name(&catalog.enemy(id).name), Some(id)); }
    let attachments = document(&root, "data/pup-attachments.json");
    let source = attachments["attachments"].as_array().unwrap();
    assert_eq!(catalog.attachments().len(), 113);
    assert_eq!(catalog.attachments().len(), source.len());
    for (actual, expected) in catalog.attachments().iter().zip(source) {
        assert_ordered_value(actual, expected);
        let name = expected["Name"].as_str().unwrap();
        assert_ordered_value(catalog.attachment_by_name(name).unwrap(), expected);
    }
}

fn assert_buff_value(actual: BuffValue, expected: &Value) {
    match actual {
        BuffValue::Constant(value) => assert_number(value, expected),
        BuffValue::Scaling(value) => {
            assert_number(value.base, &expected["base"]);
            assert_number(value.per_point, &expected["per_point"]);
        }
        BuffValue::CorRoll(value) => {
            for (index, numeral) in ROLL_NUMERALS.iter().enumerate() {
                assert_number(value.potency[index], &expected["potency"][numeral]);
            }
            assert_number(value.per_roll_bonus, &expected["per_roll_bonus"]);
            assert_number(value.job_bonus, &expected["job_bonus"]);
        }
    }
}

#[test]
fn real_buffs_retain_numeric_units_requirements_and_source_order() {
    let root = resource_root();
    let catalog = Catalog::load(&root).unwrap();
    let source = document(&root, "data/buffs.json");
    for kind in BuffTableKind::ALL {
        let table = catalog.buffs().table(kind);
        let expected_table = source[kind.name()].as_object().unwrap();
        assert_eq!(table.entries().len(), expected_table.len());
        for (index, (actual, (name, expected))) in table.entries().iter().zip(expected_table).enumerate() {
            assert_eq!(actual.name.as_ref(), name);
            assert_eq!(table.index_of(name), Some(index));
            let expected_level = expected.get("level requirement").map(|value| value.as_f64().unwrap());
            assert_eq!(actual.level_requirement, expected_level);
            let expected_jobs = expected.get("job requirement").map(|value| value.as_array().unwrap().iter().map(|job| job.as_str().unwrap()).collect::<Vec<_>>());
            assert_eq!(actual.job_requirement.as_ref().map(|jobs| jobs.iter().map(|job| job.name()).collect::<Vec<_>>()), expected_jobs);
            let expected_stats: Vec<_> = expected.as_object().unwrap().iter().filter(|(name, _)| name.as_str() != "level requirement" && name.as_str() != "job requirement").collect();
            assert_eq!(actual.stats.len(), expected_stats.len());
            for (actual, (name, expected)) in actual.stats.iter().zip(expected_stats) {
                assert_eq!(actual.stat.name(), name);
                assert_buff_value(actual.value, expected);
            }
            assert_eq!(actual.song_bonus_cap, if kind == BuffTableKind::Brd { source["brd_song_limits"][name].as_f64() } else { None });
        }
    }
    let brd = catalog.buffs().table(BuffTableKind::Brd);
    let limits = source["brd_song_limits"].as_object().unwrap();
    assert_eq!(catalog.buffs().song_limits().len(), limits.len());
    for (actual, (name, expected)) in catalog.buffs().song_limits().iter().zip(limits) {
        assert_eq!(brd.entries()[actual.song_index].name.as_ref(), name);
        assert_number(actual.cap, expected);
    }
    assert_eq!(catalog.buffs().table(BuffTableKind::Whm).get("Haste").unwrap().stat(Stat::MagicHaste), Some(BuffValue::Constant(0.146484375)));
    let box_step = catalog.buffs().table(BuffTableKind::MiscDebuffs).get("Box Step (sub)").unwrap();
    assert_eq!(box_step.level_requirement, Some(30.0));
    assert_eq!(box_step.job_requirement.as_deref(), Some([Job::Dnc].as_slice()));
}

#[test]
fn real_csv_retains_all_rows_and_first_matching_aliases() {
    let root = resource_root();
    let catalog = Catalog::load(&root).unwrap();
    let mut reader = csv::ReaderBuilder::new().delimiter(b';').from_path(root.join("item_list.csv")).unwrap();
    let records: Vec<_> = reader.records().map(Result::unwrap).collect();
    assert_eq!(catalog.aliases().len(), records.len());
    let mut full_names = HashMap::new();
    let mut abbreviations = HashMap::new();
    for (index, (actual, expected)) in catalog.aliases().iter().zip(&records).enumerate() {
        assert_eq!(actual.id.as_ref(), &expected[0]);
        assert_eq!(actual.name.as_ref(), &expected[1]);
        assert_eq!(actual.name2.as_ref(), &expected[2]);
        full_names.entry(expected[1].to_lowercase()).or_insert(index);
        abbreviations.entry(expected[2].to_lowercase()).or_insert(index);
    }
    for (name, index) in full_names {
        assert!(std::ptr::eq(catalog.alias_by_full_name(&name).unwrap(), &catalog.aliases()[index]));
    }
    for (name, index) in abbreviations {
        assert!(std::ptr::eq(catalog.alias_by_abbreviation(&name).unwrap(), &catalog.aliases()[index]));
    }
    let stormwaker = catalog.alias_by_abbreviation("STORMWAKER FRAME").unwrap();
    assert_eq!(stormwaker.id.as_ref(), "65407");
    assert_eq!(stormwaker.name.as_ref(), "stormwaker head");
}

#[test]
fn sparse_original_ids_collisions_and_repeated_members_preserve_identity() {
    let temporary = copied_resources();
    let original_count = document(temporary.path(), "data/gear.json")["pieces"].as_array().unwrap().len();
    alter(temporary.path(), "data/gear.json", |source| {
        let sparse = |id: u64| 1000 + id * 3;
        for piece in source["pieces"].as_array_mut().unwrap() {
            piece["id"] = json!(sparse(piece["id"].as_u64().unwrap()));
        }
        for value in source["symbols"].as_object_mut().unwrap().values_mut() {
            *value = json!(sparse(value.as_u64().unwrap()));
        }
        for members in source["slots"].as_object_mut().unwrap().values_mut() {
            for value in members.as_array_mut().unwrap() { *value = json!(sparse(value.as_u64().unwrap())); }
        }
        for value in source["foods"].as_array_mut().unwrap() { *value = json!(sparse(value.as_u64().unwrap())); }
        source["pieces"][0]["stats"] = json!({"Attack": 17});
        let mut first = source["pieces"][15].clone();
        first["name2"] = json!("Collision");
        let mut last = first.clone();
        last["stats"]["Attack"] = json!(23);
        let mut unlisted = first.clone();
        unlisted["stats"]["Attack"] = json!(99);
        append_piece(source, "CollisionFirst", 3_000_000_000, first);
        append_piece(source, "CollisionLast", 3_000_000_001, last);
        append_piece(source, "CollisionUnlisted", u32::MAX, unlisted);
        source["slots"]["main"].as_array_mut().unwrap().extend([json!(3_000_000_000u32), json!(3_000_000_000u32)]);
        source["slots"]["feet"].as_array_mut().unwrap().push(json!(3_000_000_001u32));
        let food = source["foods"][0].clone();
        source["foods"].as_array_mut().unwrap().extend([food.clone(), food]);
        // Object key order is not the legacy traversal order.
        let old_slots = source["slots"].as_object().unwrap();
        let mut reordered = Map::new();
        reordered.insert("feet".to_owned(), old_slots["feet"].clone());
        for &slot in Slot::ALL {
            if slot != Slot::Feet { reordered.insert(slot.name().to_owned(), old_slots[slot.name()].clone()); }
        }
        source["slots"] = Value::Object(reordered);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    for index in 0..original_count {
        let original_id = 1000 + index as u32 * 3;
        assert_eq!(catalog.by_original_id(original_id).unwrap().index(), index);
    }
    let first = catalog.by_symbol("CollisionFirst").unwrap();
    let last = catalog.by_symbol("CollisionLast").unwrap();
    let unlisted = catalog.by_symbol("CollisionUnlisted").unwrap();
    assert_eq!([first.index(), last.index(), unlisted.index()], [original_count, original_count + 1, original_count + 2]);
    assert_eq!(catalog.by_original_id(u32::MAX), Some(unlisted));
    assert_eq!(catalog.by_name2("Collision"), Some(last));
    assert_eq!(catalog.piece(last).stat(Stat::Attack), 23.0);
    let main = catalog.slot(Slot::Main);
    assert_eq!(&main[main.len() - 2..], [first, first]);
    let foods = catalog.foods();
    assert_eq!(&foods[foods.len() - 2..], [foods[0], foods[0]]);
    assert_eq!(catalog.piece(catalog.empty).original_id, 1000);
    assert_eq!(catalog.piece(catalog.empty).stat(Stat::Attack), 17.0);
    assert_eq!(catalog.symbols().iter().rev().take(3).map(|symbol| symbol.name.as_ref()).collect::<Vec<_>>(), ["CollisionUnlisted", "CollisionLast", "CollisionFirst"]);
}

#[test]
fn gear_equality_preserves_presence_job_order_and_exact_numeric_literals() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear.json", |source| {
        let mut piece = source["pieces"][15].clone();
        piece["name"] = json!("Equality weapon");
        piece["name2"] = json!("Equality weapon");
        piece["jobs"] = json!(["war", "mnk"]);
        piece["stats"] = json!({"STR": 0, "Accuracy": 1});
        append_piece(source, "OrderedStats", 100_000, piece.clone());
        let mut reordered = Map::new();
        reordered.insert("Accuracy".to_owned(), json!(1.0));
        reordered.insert("STR".to_owned(), json!(-0.0));
        piece["stats"] = Value::Object(reordered);
        append_piece(source, "ReorderedStats", 100_001, piece.clone());
        piece["stats"] = json!({"Accuracy": 1});
        append_piece(source, "AbsentZero", 100_002, piece.clone());
        piece["stats"] = json!({"STR": 0, "Accuracy": 1});
        piece["jobs"] = json!(["mnk", "war"]);
        append_piece(source, "ReorderedJobs", 100_003, piece.clone());
        piece["jobs"] = json!(["war", "mnk"]);
        piece["stats"] = json!({"STR": 9_007_199_254_740_992u64});
        append_piece(source, "ExactInteger", 100_004, piece.clone());
        piece["stats"] = json!({"STR": 9_007_199_254_740_993u64});
        append_piece(source, "NeighborInteger", 100_005, piece.clone());
        piece["stats"] = json!({"STR": 9_007_199_254_740_992.0});
        append_piece(source, "EqualFloat", 100_006, piece);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    let state = ModelState::new(catalog.pieces().len());
    let piece = |name| catalog.piece(catalog.by_symbol(name).unwrap());
    let ordered = piece("OrderedStats");
    let reordered = piece("ReorderedStats");
    assert_eq!(ordered.stats.iter().map(|entry| entry.stat).collect::<Vec<_>>(), [Stat::Str, Stat::Accuracy]);
    assert_eq!(reordered.stats.iter().map(|entry| entry.stat).collect::<Vec<_>>(), [Stat::Accuracy, Stat::Str]);
    assert!(ordered.value_equal(reordered, &state));
    assert!(reordered.stats[1].literal.as_f64().unwrap().is_sign_negative());
    let absent = piece("AbsentZero");
    assert_eq!(ordered.stat(Stat::Str), absent.stat(Stat::Str));
    assert!(!ordered.value_equal(absent, &state));
    assert!(!ordered.value_equal(piece("ReorderedJobs"), &state));
    let integer = piece("ExactInteger");
    let neighbor = piece("NeighborInteger");
    assert_eq!(integer.stat(Stat::Str), neighbor.stat(Stat::Str));
    assert!(!integer.value_equal(neighbor, &state));
    assert!(integer.value_equal(piece("EqualFloat"), &state));
}

#[test]
fn optional_rank_and_wsc_are_not_inferred_or_normalized() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear.json", |source| {
        let mut piece = source["pieces"][15].clone();
        piece["name2"] = json!("Optional weapon R30");
        piece["rank"] = Value::Null;
        piece["wsc"] = Value::Null;
        append_piece(source, "NullOptions", 100_000, piece.clone());
        piece.as_object_mut().unwrap().remove("rank");
        piece.as_object_mut().unwrap().remove("wsc");
        append_piece(source, "AbsentOptions", 100_001, piece.clone());
        piece["rank"] = json!(-5);
        piece["wsc"] = json!(["CHR", 3.75]);
        piece["dmg"] = json!(12.5);
        piece["delay"] = json!(-0.0);
        append_piece(source, "ExplicitOptions", 100_002, piece);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    for symbol in ["NullOptions", "AbsentOptions"] {
        let piece = catalog.piece(catalog.by_symbol(symbol).unwrap());
        assert_eq!(piece.rank, None);
        assert_eq!(piece.wsc, None);
    }
    let piece = catalog.piece(catalog.by_symbol("ExplicitOptions").unwrap());
    assert_eq!(piece.rank, Some(-5));
    assert_eq!(piece.wsc.unwrap().stat, Stat::Chr);
    assert_eq!(piece.wsc.unwrap().percent, 3.75);
    assert_eq!(piece.dmg, 12.5);
    assert_eq!(piece.delay.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn unknown_and_internal_only_gear_stats_report_exact_spelling_and_path() {
    for (name, path) in [
        ("unknown stat", "$.pieces[0].stats[\"unknown stat\"]"),
        ("Attack1", "$.pieces[0].stats.Attack1"),
        ("main Sword Skill", "$.pieces[0].stats[\"main Sword Skill\"]"),
    ] {
        let temporary = copied_resources();
        alter(temporary.path(), "data/gear.json", |source| { source["pieces"][0]["stats"][name] = json!(1); });
        let message = assert_data_error(temporary.path(), "data/gear.json", path);
        assert!(message.contains(name), "missing exact spelling in {message}");
    }
}

#[test]
fn declared_only_internal_stats_are_preserved_but_rejected_in_slot_lists() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear.json", |source| {
        let mut piece = source["pieces"][0].clone();
        piece["name2"] = json!("Declared internal stat");
        piece["stats"] = json!({"Dark Magic Skill": 20});
        append_piece(source, "DeclaredInternal", u32::MAX, piece);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    let id = catalog.by_symbol("DeclaredInternal").unwrap();
    assert_eq!(catalog.piece(id).stat(Stat::DarkMagicSkill), 20.0);
    assert!(!catalog.slot(Slot::Main).contains(&id));

    let index = catalog.pieces().len() - 1;
    alter(temporary.path(), "data/gear.json", |source| {
        source["slots"]["main"].as_array_mut().unwrap().push(json!(u32::MAX));
    });
    assert_data_error(temporary.path(), "data/gear.json", &format!("$.pieces[{index}].stats[\"Dark Magic Skill\"]"));
}

#[test]
fn malformed_references_and_typed_values_report_the_specific_field() {
    type Change = fn(&mut Value);
    let cases: &[(&str, &str, Change)] = &[
        ("data/gear.json", "$.pieces[1].id", |source| { source["pieces"][1]["id"] = source["pieces"][0]["id"].clone(); }),
        ("data/gear.json", "$.symbols.Dangling", |source| { source["symbols"]["Dangling"] = json!(u32::MAX); }),
        ("data/gear.json", "$.slots.main[0]", |source| { source["slots"]["main"][0] = json!(u32::MAX); }),
        ("data/gear.json", "$.foods[0]", |source| { source["foods"][0] = json!(u32::MAX); }),
        ("data/gear.json", "$.pieces[0].jobs[0]", |source| { source["pieces"][0]["jobs"][0] = json!("War"); }),
        ("data/gear.json", "$.pieces[0].wsc[1]", |source| { source["pieces"][0]["wsc"] = json!(["STR", "not a number"]); }),
        ("data/gear.json", "$.symbols.Empty", |source| { source["symbols"]["Empty"] = source["pieces"][1]["id"].clone(); }),
        ("data/enemies.json", "$.enemies[0][\"Magic Defense\"]", |source| { source["enemies"][0].as_object_mut().unwrap().remove("Magic Defense"); }),
        ("data/buffs.json", "$.cor.Chaos[\"Attack%\"].potency.XI", |source| { source["cor"]["Chaos"]["Attack%"]["potency"].as_object_mut().unwrap().remove("XI"); }),
        ("data/buffs.json", "$.brd_song_limits[\"Honor March\"]", |source| { source["brd_song_limits"].as_object_mut().unwrap().remove("Honor March"); }),
        ("data/buffs.json", "$.misc_debuffs[\"Box Step (sub)\"][\"job requirement\"][0]", |source| { source["misc_debuffs"]["Box Step (sub)"]["job requirement"][0] = json!("dancer"); }),
        ("data/pup-attachments.json", "$.attachments[0].Capacity", |source| { source["attachments"][0]["Capacity"] = json!("two"); }),
    ];
    for &(file, path, change) in cases {
        let temporary = copied_resources();
        alter(temporary.path(), file, change);
        assert_data_error(temporary.path(), file, path);
    }
}

#[test]
fn an_empty_piece_is_required_but_need_not_be_listed_or_have_a_symbol() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear.json", |source| {
        let empty_id = source["symbols"]["Empty"].clone();
        source["symbols"].as_object_mut().unwrap().remove("Empty");
        for members in source["slots"].as_object_mut().unwrap().values_mut() {
            members.as_array_mut().unwrap().retain(|id| id != &empty_id);
        }
        source["pieces"][0]["stats"] = json!({"STR": 5});
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    assert_eq!(catalog.piece(catalog.empty).name2.as_ref(), "Empty");
    assert_eq!(catalog.piece(catalog.empty).stat(Stat::Str), 5.0);
    assert_eq!(catalog.by_name2("Empty"), None);
    alter(temporary.path(), "data/gear.json", |source| { source["pieces"][0]["name2"] = json!("No empty piece"); });
    assert_data_error(temporary.path(), "data/gear.json", "$.pieces");
}

#[test]
fn enemy_name_collisions_keep_first_menu_position_and_last_sorted_value() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/enemies.json", |source| {
        let mut high = source["enemies"][0].clone();
        high["Name"] = json!("Repeated enemy");
        high["Level"] = json!(2000);
        high["Defense"] = json!(200);
        let mut tied_first = high.clone();
        tied_first["Name"] = json!("First tied enemy");
        tied_first["Level"] = json!(1500);
        let mut tied_second = tied_first.clone();
        tied_second["Name"] = json!("Second tied enemy");
        let mut low = high.clone();
        low["Level"] = json!(1);
        low["Defense"] = json!(1);
        source["enemies"].as_array_mut().unwrap().extend([high, tied_first, tied_second, low]);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    let menu = catalog.enemy_menu();
    assert_eq!(catalog.enemy(menu[0]).name.as_ref(), "Repeated enemy");
    assert_eq!(catalog.enemy(menu[0]).level, 1.0);
    assert_eq!(catalog.enemy(menu[0]).stat(Stat::Defense), Some(1.0));
    assert_eq!(catalog.enemy_by_name("Repeated enemy"), Some(menu[0]));
    assert_eq!(catalog.enemy(menu[1]).name.as_ref(), "First tied enemy");
    assert_eq!(catalog.enemy(menu[2]).name.as_ref(), "Second tied enemy");
    assert_eq!(catalog.enemies()[11].level, 2000.0);
    assert_eq!(catalog.enemies()[14].level, 1.0);
}

#[test]
fn conflicting_case_insensitive_csv_aliases_keep_first_row_and_quoted_fields() {
    let temporary = copied_resources();
    fs::write(temporary.path().join("item_list.csv"), concat!(
        "id;name;name2\n",
        "001;Shared full;shared abbr\n",
        "2;shared full;other abbr\n",
        "3;Other full;SHARED ABBR\n",
        "4;\"Name; quoted \"\"blade\"\"\";\"Abbr; \"\"blade\"\"\"\n",
    )).unwrap();
    let catalog = Catalog::load(temporary.path()).unwrap();
    let rows: Vec<_> = catalog.aliases().iter().map(|alias| (alias.id.as_ref(), alias.name.as_ref(), alias.name2.as_ref())).collect();
    assert_eq!(rows, [
        ("001", "Shared full", "shared abbr"),
        ("2", "shared full", "other abbr"),
        ("3", "Other full", "SHARED ABBR"),
        ("4", "Name; quoted \"blade\"", "Abbr; \"blade\""),
    ]);
    assert_eq!(catalog.alias_by_full_name("SHARED FULL").unwrap().id.as_ref(), "001");
    assert_eq!(catalog.alias_by_abbreviation("SHARED ABBR").unwrap().id.as_ref(), "001");
    assert_eq!(catalog.alias_by_abbreviation("OTHER ABBR").unwrap().id.as_ref(), "2");
    assert_eq!(catalog.alias_by_full_name("other full").unwrap().id.as_ref(), "3");
    assert_eq!(catalog.alias_by_full_name("NAME; QUOTED \"BLADE\"").unwrap().name2.as_ref(), "Abbr; \"blade\"");
}

#[test]
fn invalid_csv_columns_and_ids_identify_the_source_row() {
    for (content, field) in [
        ("id;name;name2\n1;one column missing\n", false),
        ("id;name;name2\nnot-an-id;full;abbreviated\n", true),
    ] {
        let temporary = copied_resources();
        fs::write(temporary.path().join("item_list.csv"), content).unwrap();
        match Catalog::load(temporary.path()).unwrap_err() {
            CalcError::Data { path, json_path, .. } => {
                assert_eq!(path, temporary.path().join("item_list.csv"));
                if field { assert_eq!(json_path, "line 2.id"); }
                else { assert!(json_path.starts_with("line 2 "), "unexpected row location {json_path}"); }
            }
            error => panic!("expected CSV data error, got {error}"),
        }
    }
}
