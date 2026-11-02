use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use tempfile::TempDir;
use wsdist_core::catalog::{BuffTableKind, BuffValue, ROLL_NUMERALS};
use wsdist_core::model::ModelState;
use wsdist_core::{CalcError, Catalog, Job, Slot, Stat};

fn resource_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn document(root: &Path, name: &str) -> Value {
    json5::from_str(&fs::read_to_string(root.join(name)).unwrap()).unwrap()
}

fn copied_resources() -> TempDir {
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir_all(temporary.path().join("data/gear")).unwrap();
    for name in ["data/enemies.json5", "data/buffs.json5", "data/pup-attachments.json5", "item_list.csv"] {
        fs::copy(resource_root().join(name), temporary.path().join(name)).unwrap();
    }
    let slots = Slot::ALL.iter().map(|slot| slot.name()).collect::<Vec<_>>();
    write_fragment(temporary.path(), "test.json5", vec![
        test_piece("Empty", &slots),
        test_piece("Example weapon", &["main", "sub"]),
        test_piece("Example food", &["food"]),
    ]);
    temporary
}

fn test_piece(name: &str, slots: &[&str]) -> Value {
    json!({
        "name": name, "name2": name, "jobs": ["war", "mnk"], "slots": slots,
        "type": "None", "skill_type": "None", "dmg": 0, "delay": 0,
        "rank": null, "wsc": null, "stats": {},
    })
}

fn write_fragment(root: &Path, relative: &str, pieces: Vec<Value>) {
    let path = root.join("data/gear").join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&json!({"schema_version": 2, "pieces": pieces})).unwrap()).unwrap();
}

fn alter(root: &Path, name: &str, change: impl FnOnce(&mut Value)) {
    let mut value = document(root, name);
    change(&mut value);
    // JSON is a JSON5 subset; this writer retains integer/float kinds and -0.0.
    fs::write(root.join(name), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn alter_with_raw_number(root: &Path, name: &str, literal: &str, change: impl FnOnce(&mut Value, Value)) {
    let mut value = document(root, name);
    change(&mut value, json!("RAW_JSON5_NUMBER"));
    let source = serde_json::to_string(&value).unwrap().replace("\"RAW_JSON5_NUMBER\"", literal);
    fs::write(root.join(name), source).unwrap();
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

fn assert_data_error_file(root: &Path, file: &str) {
    match Catalog::load(root) {
        Err(CalcError::Data { path, .. }) => assert_eq!(path, root.join(file)),
        Err(error) => panic!("expected a data error, got {error}"),
        Ok(_) => panic!("expected {file} to be rejected"),
    }
}

fn append_piece(document: &mut Value, piece: Value) {
    document["pieces"].as_array_mut().unwrap().push(piece);
}

#[test]
fn json5_literals_preserve_attachment_order_and_exact_number_kinds() {
    let temporary = copied_resources();
    fs::write(temporary.path().join("data/pup-attachments.json5"), r#"{
  // Catalogs accept editable JSON5, including nested trailing commas.
  schema_version: 0x1,
  attachments: [
    {
      Name: 'Artificer\'s test',
      Element: 'Fire',
      Capacity: 0x2,
      values: {
        positive: +7,
        negative: -0x10,
        hex: 0x20000000000001,
        hex_above_u64: 0x10000000000000000,
        hex_below_i64: -0x8000000000000001,
        hex_negative_zero: -0x0,
        leading: .125,
        trailing: 12.,
        negative_zero: -0.0, /* Keep the floating-point sign bit. */
      },
      notes: ['first', 'second',],
    },
    {Name: 'Second attachment', Element: 'Ice', Capacity: 1,},
  ],
}"#).unwrap();
    let catalog = Catalog::load(temporary.path()).unwrap();
    assert_eq!(catalog.attachments().iter().map(|value| value["Name"].as_str().unwrap()).collect::<Vec<_>>(), [
        "Artificer's test", "Second attachment",
    ]);
    let attachment = catalog.attachment_by_name("Artificer's test").unwrap();
    assert_eq!(attachment.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), [
        "Name", "Element", "Capacity", "values", "notes",
    ]);
    assert_eq!(attachment["Capacity"].as_u64(), Some(2));
    assert_eq!(attachment["notes"], json!(["first", "second"]));
    let values = attachment["values"].as_object().unwrap();
    assert_eq!(values.keys().map(String::as_str).collect::<Vec<_>>(), [
        "positive", "negative", "hex", "hex_above_u64", "hex_below_i64", "hex_negative_zero",
        "leading", "trailing", "negative_zero",
    ]);
    assert_eq!(values["positive"].as_u64(), Some(7));
    assert_eq!(values["negative"].as_i64(), Some(-16));
    assert_eq!(values["hex"].as_u64(), Some(9_007_199_254_740_993));
    for (name, expected) in [
        ("leading", 0.125f64), ("trailing", 12.0), ("negative_zero", -0.0),
        ("hex_above_u64", 18_446_744_073_709_551_616.0),
        ("hex_below_i64", -9_223_372_036_854_775_808.0), ("hex_negative_zero", -0.0),
    ] {
        assert!(values[name].is_f64(), "{name} lost its floating-point kind");
        assert_eq!(values[name].as_f64().unwrap().to_bits(), expected.to_bits(), "{name}");
    }
}

#[test]
fn invalid_json5_identifies_the_catalog_and_source_position() {
    let temporary = copied_resources();
    fs::write(temporary.path().join("data/pup-attachments.json5"), concat!(
        "// editable JSON5\n",
        "{\n",
        "  schema_version: ?,\n",
        "}\n",
    )).unwrap();
    assert_data_error(temporary.path(), "data/pup-attachments.json5", "$ (line 3, column 19)");
}

#[test]
fn json5_non_finite_literals_in_required_fields_are_rejected() {
    for (file, pointer) in [
        ("data/gear/test.json5", "/pieces/0/dmg"),
        ("data/enemies.json5", "/enemies/0/Level"),
        ("data/buffs.json5", "/whm/Haste/Magic Haste"),
        ("data/pup-attachments.json5", "/attachments/0/Capacity"),
    ] {
        for literal in ["NaN", "Infinity", "-Infinity"] {
            let temporary = copied_resources();
            alter_with_raw_number(temporary.path(), file, literal, |source, number| {
                *source.pointer_mut(pointer).unwrap() = number;
            });
            assert_data_error_file(temporary.path(), file);
        }
    }
}

#[test]
fn json5_non_finite_literals_cannot_become_null_in_optional_or_extra_fields() {
    type Change = fn(&mut Value, Value);
    let cases: &[(&str, Change)] = &[
        ("data/gear/test.json5", |source, number| { source["pieces"][0]["rank"] = number; }),
        ("data/gear/test.json5", |source, number| { source["pieces"][0]["wsc"] = number; }),
        ("data/pup-attachments.json5", |source, number| {
            source["attachments"][0]["numeric_extra"] = json!({"values": [number]});
        }),
    ];
    for &(file, change) in cases {
        for literal in ["NaN", "Infinity", "-Infinity", "1e400"] {
            let temporary = copied_resources();
            alter_with_raw_number(temporary.path(), file, literal, change);
            assert_data_error_file(temporary.path(), file);
        }
    }
}

#[test]
fn json5_raw_negative_zero_preserves_float_semantics_and_is_not_an_integer() {
    let temporary = copied_resources();
    alter_with_raw_number(temporary.path(), "data/gear/test.json5", "-0", |source, number| {
        source["pieces"][0]["delay"] = number.clone();
        source["pieces"][0]["stats"] = json!({"STR": number});
    });
    alter_with_raw_number(temporary.path(), "data/pup-attachments.json5", "-0", |source, number| {
        source["attachments"][0]["numeric_extra"] = json!({
            "NaN": 7, "Infinity": number, "text": "-0 NaN Infinity 123", "values": [11, 13],
        });
    });
    let path = temporary.path().join("data/pup-attachments.json5");
    let source = fs::read_to_string(&path).unwrap()
        .replacen("\"NaN\":", "NaN:", 1).replacen("\"Infinity\":", "Infinity:", 1);
    fs::write(path, source).unwrap();
    let catalog = Catalog::load(temporary.path()).unwrap();
    let piece = &catalog.pieces()[0];
    let expected_bits = (-0.0f64).to_bits();
    assert_eq!(piece.delay.to_bits(), expected_bits);
    let stat = &piece.stats[0];
    assert_eq!(stat.stat, Stat::Str);
    assert_eq!(stat.value.to_bits(), expected_bits);
    assert!(stat.literal.is_f64(), "raw -0 lost its floating-point kind");
    assert_eq!(stat.literal.as_f64().unwrap().to_bits(), expected_bits);
    let metadata = &catalog.attachments()[0]["numeric_extra"];
    assert_eq!(metadata.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
        ["NaN", "Infinity", "text", "values"]);
    assert_eq!(metadata["NaN"], 7);
    assert_eq!(metadata["values"], json!([11, 13]));
    let extra = &metadata["Infinity"];
    assert!(extra.is_f64(), "raw -0 in extra metadata lost its floating-point kind");
    assert_eq!(extra.as_f64().unwrap().to_bits(), expected_bits);

    for (file, collection, field) in [
        ("data/gear/test.json5", "pieces", "rank"),
        ("data/pup-attachments.json5", "attachments", "Capacity"),
    ] {
        let temporary = copied_resources();
        alter_with_raw_number(temporary.path(), file, "-0", |source, number| {
            source[collection][0][field] = number;
        });
        assert_data_error_file(temporary.path(), file);
    }
}

#[test]
fn json5_out_of_range_decimal_integers_remain_finite_floating_numbers() {
    for (literal, expected_bits) in [
        ("18446744073709551616", 0x43f0_0000_0000_0000u64), // 2^64, just above u64.
        ("-9223372036854775809", 0xc3e0_0000_0000_0000), // Just below i64, rounded to -2^63.
        ("340282366920938463463374607431768211456", 0x47f0_0000_0000_0000), // 2^128, just above u128.
    ] {
        let temporary = copied_resources();
        alter_with_raw_number(temporary.path(), "data/gear/test.json5", literal, |source, number| {
            source["pieces"][0]["delay"] = number.clone();
            source["pieces"][0]["stats"] = json!({"STR": number});
        });
        alter_with_raw_number(temporary.path(), "data/pup-attachments.json5", literal, |source, number| {
            source["attachments"][0]["numeric_extra"] = json!({"values": [number]});
        });
        let catalog = Catalog::load(temporary.path()).unwrap();
        let piece = &catalog.pieces()[0];
        assert_eq!(piece.delay.to_bits(), expected_bits, "{literal}");
        let stat = &piece.stats[0];
        assert_eq!(stat.stat, Stat::Str);
        assert_eq!(stat.value.to_bits(), expected_bits, "{literal}");
        assert!(stat.literal.is_f64(), "{literal} lost its floating-point kind");
        assert_eq!(stat.literal.as_f64().unwrap().to_bits(), expected_bits, "{literal}");
        let extra = &catalog.attachments()[0]["numeric_extra"]["values"][0];
        assert!(extra.is_f64(), "{literal} in extra metadata lost its floating-point kind");
        assert_eq!(extra.as_f64().unwrap().to_bits(), expected_bits, "{literal}");
    }
}

#[test]
fn json5_documents_reject_trailing_values_even_when_unfinished() {
    let temporary = copied_resources();
    let name = "data/enemies.json5";
    let source = fs::read_to_string(temporary.path().join(name)).unwrap();
    for suffix in ["0", "[", "{", "null"] {
        fs::write(temporary.path().join(name), format!("{source}\n{suffix}")).unwrap();
        assert_data_error_file(temporary.path(), name);
    }
    fs::write(temporary.path().join(name), format!("{source}\n// trailing comment\n/* complete */\n")).unwrap();
    let expected = Catalog::load(&resource_root()).unwrap();
    let actual = Catalog::load(temporary.path()).unwrap();
    let menu = |catalog: &Catalog| catalog.enemy_menu().iter()
        .map(|id| {
            let enemy = catalog.enemy(*id);
            (enemy.name.to_string(), enemy.level.to_bits())
        }).collect::<Vec<_>>();
    assert_eq!(menu(&actual), menu(&expected));
}

#[test]
fn json5_strings_preserve_literal_unicode_line_and_paragraph_separators() {
    let temporary = copied_resources();
    let text = "left\u{2028}middle\u{2029}right";
    alter(temporary.path(), "data/pup-attachments.json5", |source| {
        source["attachments"][0]["text_extra"] = json!(text);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    assert_eq!(catalog.attachments()[0]["text_extra"].as_str(), Some(text));
}


#[test]
fn real_enemies_and_attachments_preserve_records_and_menu_order() {
    let root = resource_root();
    let catalog = Catalog::load(&root).unwrap();
    let enemies = document(&root, "data/enemies.json5");
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
    let attachments = document(&root, "data/pup-attachments.json5");
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
    let source = document(&root, "data/buffs.json5");
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
fn nested_fragments_keep_selection_order_independent_of_file_placement() {
    let temporary = copied_resources();
    let root = temporary.path();
    let mut alpha = test_piece("Alpha blade", &["main", "head"]);
    alpha["jobs"] = json!(["nin"]);
    alpha["type"] = json!("Food");
    alpha["stats"] = json!({"Attack": 23});
    let mut declared = alpha.clone();
    declared["slots"] = json!([]);
    declared["stats"] = json!({"Attack": 99});
    let mut other_declared = declared.clone();
    other_declared["stats"] = json!({"Attack": 101});
    let mut zulu = test_piece("Zulu blade", &["main", "sub"]);
    zulu["name"] = json!("Aardvark base weapon");
    let mut alpha_meal = test_piece("Alpha meal", &["food"]);
    alpha_meal["name"] = json!("Zulu meal base");
    let mut zulu_meal = test_piece("Zulu meal", &["food"]);
    zulu_meal["name"] = json!("Alpha meal base");
    write_fragment(root, "z/weapons.json5", vec![
        zulu,
        alpha_meal,
    ]);
    write_fragment(root, "a/armor.json5", vec![
        alpha, declared, other_declared, zulu_meal,
    ]);
    fs::write(root.join("data/gear/ignored.JSON5"), "not JSON5").unwrap();
    fs::write(root.join("data/gear/ignored.json"), "not JSON5").unwrap();
    fs::write(root.join("data/gear.json5"), "not a fallback catalog").unwrap();
    fs::create_dir(root.join("data/gear/directory.json5")).unwrap();
    let catalog = Catalog::load(root).unwrap();
    let alpha = catalog.by_name2("Alpha blade").unwrap();
    assert_eq!(catalog.piece(alpha).stat(Stat::Attack), 23.0);
    assert!(catalog.piece(alpha).eligible_for(Job::Nin));
    assert!(!catalog.piece(alpha).eligible_for(Job::War));
    assert!(catalog.slot(Slot::Main).contains(&alpha));
    assert!(catalog.slot(Slot::Head).contains(&alpha));
    assert!(!catalog.foods().contains(&alpha), "type and directory must not infer membership");
    let declarations = catalog.pieces().iter().filter(|piece| piece.name2.as_ref() == "Alpha blade").collect::<Vec<_>>();
    assert_eq!(declarations.len(), 3);
    for piece in declarations.iter().filter(|piece| piece.id != alpha) {
        assert!(!Slot::ALL.iter().any(|slot| catalog.slot(*slot).contains(&piece.id)));
        assert!(!catalog.foods().contains(&piece.id));
    }
    assert_eq!(catalog.by_name2("Alpha meal"), None);
    assert_eq!(catalog.piece(catalog.food_by_name("Zulu meal base").unwrap()).name2.as_ref(), "Alpha meal");
    assert_eq!(catalog.food_by_name("Alpha meal"), None);
    assert_eq!(catalog.slot(Slot::Main).iter().map(|id| catalog.piece(*id).name2.as_ref()).collect::<Vec<_>>(),
        ["Alpha blade", "Empty", "Example weapon", "Zulu blade"]);
    assert_eq!(catalog.foods().iter().map(|id| catalog.piece(*id).name2.as_ref()).collect::<Vec<_>>(),
        ["Alpha meal", "Example food", "Zulu meal"]);
    assert_eq!(catalog.pieces().iter().filter(|piece| piece.name2.as_ref() == "Empty").count(), 1);
    for &slot in Slot::ALL { assert!(catalog.slot(slot).contains(&catalog.empty)); }
    for (index, piece) in catalog.pieces().iter().enumerate() { assert_eq!(piece.id.index(), index); }
    let membership = |catalog: &Catalog| {
        let slots = Slot::ALL.iter().map(|slot| catalog.slot(*slot).iter()
            .map(|id| catalog.piece(*id).name2.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        let foods = catalog.foods().iter().map(|id| catalog.piece(*id).name2.to_string()).collect::<Vec<_>>();
        (slots, foods)
    };
    fs::rename(root.join("data/gear/a/armor.json5"), root.join("data/gear/z/armor.json5")).unwrap();
    fs::rename(root.join("data/gear/z/weapons.json5"), root.join("data/gear/a/weapons.json5")).unwrap();
    let moved = Catalog::load(root).unwrap();
    assert_eq!(membership(&moved), membership(&catalog));
    assert_eq!(moved.piece(moved.by_name2("Alpha blade").unwrap()).stat(Stat::Attack), 23.0);
}

#[test]
fn selectable_labels_cannot_conflict_across_fragments_or_selection_kinds() {
    for (first_slot, second_slot) in [("main", "feet"), ("food", "main"), ("food", "food")] {
        let temporary = copied_resources();
        write_fragment(temporary.path(), "a.json5", vec![test_piece("Conflicting selection", &[first_slot])]);
        let mut conflicting = test_piece("Conflicting selection", &[second_slot]);
        conflicting["stats"] = json!({"Attack": 99});
        write_fragment(temporary.path(), "nested/z.json5", vec![conflicting]);
        let message = assert_data_error(temporary.path(), "data/gear/nested/z.json5", "$.pieces[0].name2");
        assert!(message.contains("Conflicting selection"), "{message}");
        assert!(message.contains(&temporary.path().join("data/gear/a.json5").display().to_string()), "{message}");
        assert!(message.contains("$.pieces[0].name2"), "{message}");
    }
}

#[test]
fn food_base_name_ambiguity_reports_both_origins_instead_of_overriding() {
    let temporary = copied_resources();
    let mut first = test_piece("Shared meal", &["food"]);
    first["name2"] = json!("Shared meal R0");
    let mut second = first.clone();
    second["name2"] = json!("Shared meal R1");
    write_fragment(temporary.path(), "a.json5", vec![first]);
    write_fragment(temporary.path(), "nested/z.json5", vec![second]);
    let message = assert_data_error(temporary.path(), "data/gear/nested/z.json5", "$.pieces[0].name");
    assert!(message.contains("Shared meal"), "{message}");
    assert!(message.contains(&temporary.path().join("data/gear/a.json5").display().to_string()), "{message}");
    assert!(message.contains("$.pieces[0].name"), "{message}");
}

#[test]
fn missing_and_empty_fragment_directories_never_fall_back_to_a_monolith() {
    for missing in [false, true] {
        let temporary = copied_resources();
        fs::remove_dir_all(temporary.path().join("data/gear")).unwrap();
        if !missing { fs::create_dir(temporary.path().join("data/gear")).unwrap(); }
        fs::write(temporary.path().join("data/gear.json5"), r#"{"schema_version":2,"pieces":[]}"#).unwrap();
        assert_data_error(temporary.path(), "data/gear", "$");
    }
}

#[test]
fn nested_fragment_read_parse_and_record_errors_keep_the_fragment_path() {
    let temporary = copied_resources();
    let file = "data/gear/nested/broken.json5";
    write_fragment(temporary.path(), "nested/broken.json5", vec![]);
    fs::write(temporary.path().join(file), [0xff]).unwrap();
    assert_data_error(temporary.path(), file, "$");
    fs::write(temporary.path().join(file), "{\n schema_version: ?,\n}").unwrap();
    match Catalog::load(temporary.path()).unwrap_err() {
        CalcError::Data { path, json_path, .. } => {
            assert_eq!(path, temporary.path().join(file));
            assert!(json_path.starts_with("$ (line 2, column "), "{json_path}");
        }
        error => panic!("expected a fragment data error, got {error}"),
    }
    write_fragment(temporary.path(), "nested/broken.json5", vec![test_piece("Broken", &["feet"])]);
    alter(temporary.path(), file, |source| { source["schema_version"] = json!(1); });
    assert_data_error(temporary.path(), file, "$.schema_version");
    alter(temporary.path(), file, |source| {
        source["schema_version"] = json!(2);
        source["pieces"][0]["stats"]["Attack1"] = json!(7);
    });
    assert_data_error(temporary.path(), file, "$.pieces[0].stats.Attack1");
}

#[cfg(target_os = "linux")]
#[test]
fn case_only_fragment_paths_are_rejected_before_their_records_can_conflict() {
    let temporary = copied_resources();
    write_fragment(temporary.path(), "Sets/Armor.json5", vec![test_piece("Shared armor", &["head"])]);
    write_fragment(temporary.path(), "sets/armor.json5", vec![test_piece("Shared armor", &["head"])]);
    let message = assert_data_error(temporary.path(), "data/gear/sets/armor.json5", "$");
    assert!(message.contains("case-only"), "{message}");
    assert!(message.contains(&temporary.path().join("data/gear/Sets/Armor.json5").display().to_string()), "{message}");
}

#[cfg(unix)]
#[test]
fn symlinked_files_directories_and_roots_are_not_followed() {
    use std::os::unix::fs::symlink;

    let temporary = copied_resources();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("broken.json5"), "not JSON5").unwrap();
    let directory = temporary.path().join("data/gear");
    symlink(outside.path(), directory.join("linked-directory")).unwrap();
    symlink(outside.path().join("broken.json5"), directory.join("linked-file.json5")).unwrap();
    symlink(outside.path().join("missing.json5"), directory.join("dangling.json5")).unwrap();
    symlink(&directory, directory.join("cycle")).unwrap();
    let catalog = Catalog::load(temporary.path()).unwrap();
    assert!(catalog.by_name2("Example weapon").is_some());
    fs::rename(&directory, temporary.path().join("data/previous-gear")).unwrap();
    symlink(temporary.path().join("data/previous-gear"), &directory).unwrap();
    assert_data_error(temporary.path(), "data/gear", "$");
}

#[test]
fn gear_equality_preserves_presence_job_order_and_exact_numeric_literals() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear/test.json5", |source| {
        let mut piece = test_piece("Equality weapon", &[]);
        piece["stats"] = json!({"STR": 0, "Accuracy": 1});
        append_piece(source, piece.clone());
        let mut reordered = Map::new();
        reordered.insert("Accuracy".to_owned(), json!(1.0));
        reordered.insert("STR".to_owned(), json!(-0.0));
        piece["stats"] = Value::Object(reordered);
        append_piece(source, piece.clone());
        piece["stats"] = json!({"Accuracy": 1});
        append_piece(source, piece.clone());
        piece["stats"] = json!({"STR": 0, "Accuracy": 1});
        piece["jobs"] = json!(["mnk", "war"]);
        append_piece(source, piece.clone());
        piece["jobs"] = json!(["war", "mnk"]);
        piece["stats"] = json!({"STR": 9_007_199_254_740_992u64});
        append_piece(source, piece.clone());
        piece["stats"] = json!({"STR": 9_007_199_254_740_993u64});
        append_piece(source, piece.clone());
        piece["stats"] = json!({"STR": 9_007_199_254_740_992.0});
        append_piece(source, piece);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    let state = ModelState::new(catalog.pieces().len());
    let pieces = catalog.pieces().iter().filter(|piece| piece.name2.as_ref() == "Equality weapon").collect::<Vec<_>>();
    let ordered = pieces[0];
    let reordered = pieces[1];
    assert_eq!(ordered.stats.iter().map(|entry| entry.stat).collect::<Vec<_>>(), [Stat::Str, Stat::Accuracy]);
    assert_eq!(reordered.stats.iter().map(|entry| entry.stat).collect::<Vec<_>>(), [Stat::Accuracy, Stat::Str]);
    assert!(ordered.value_equal(reordered, &state));
    assert!(reordered.stats[1].literal.as_f64().unwrap().is_sign_negative());
    let absent = pieces[2];
    assert_eq!(ordered.stat(Stat::Str), absent.stat(Stat::Str));
    assert!(!ordered.value_equal(absent, &state));
    assert!(!ordered.value_equal(pieces[3], &state));
    let integer = pieces[4];
    let neighbor = pieces[5];
    assert_eq!(integer.stat(Stat::Str), neighbor.stat(Stat::Str));
    assert!(!integer.value_equal(neighbor, &state));
    assert!(integer.value_equal(pieces[6], &state));
}

#[test]
fn optional_rank_and_wsc_are_not_inferred_or_normalized() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear/test.json5", |source| {
        let mut piece = test_piece("Optional weapon R30", &[]);
        append_piece(source, piece.clone());
        piece.as_object_mut().unwrap().remove("rank");
        piece.as_object_mut().unwrap().remove("wsc");
        append_piece(source, piece.clone());
        piece["rank"] = json!(-5);
        piece["wsc"] = json!(["CHR", 3.75]);
        piece["dmg"] = json!(12.5);
        piece["delay"] = json!(-0.0);
        append_piece(source, piece);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    let pieces = catalog.pieces().iter().filter(|piece| piece.name2.as_ref() == "Optional weapon R30").collect::<Vec<_>>();
    for piece in &pieces[..2] {
        assert_eq!(piece.rank, None);
        assert_eq!(piece.wsc, None);
    }
    let piece = pieces[2];
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
        alter(temporary.path(), "data/gear/test.json5", |source| { source["pieces"][0]["stats"][name] = json!(1); });
        let message = assert_data_error(temporary.path(), "data/gear/test.json5", path);
        assert!(message.contains(name), "missing exact spelling in {message}");
    }
}

#[test]
fn declared_only_internal_stats_are_preserved_but_rejected_in_slot_lists() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear/test.json5", |source| {
        let mut piece = test_piece("Declared internal stat", &[]);
        piece["stats"] = json!({"Dark Magic Skill": 20});
        append_piece(source, piece);
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    let id = catalog.pieces().iter().find(|piece| piece.name2.as_ref() == "Declared internal stat").unwrap().id;
    assert_eq!(catalog.piece(id).stat(Stat::DarkMagicSkill), 20.0);
    assert!(!catalog.slot(Slot::Main).contains(&id));

    let index = catalog.pieces().len() - 1;
    alter(temporary.path(), "data/gear/test.json5", |source| {
        source["pieces"][index]["slots"] = json!(["main"]);
    });
    assert_data_error(temporary.path(), "data/gear/test.json5", &format!("$.pieces[{index}].stats[\"Dark Magic Skill\"]"));
}

#[test]
fn malformed_schema_and_typed_values_report_the_specific_field() {
    type Change = fn(&mut Value);
    let cases: &[(&str, &str, Change)] = &[
        ("data/gear/test.json5", "$.schema_version", |source| { source["schema_version"] = json!(1); }),
        ("data/gear/test.json5", "$.pieces[1].id", |source| { source["pieces"][1]["id"] = json!(7); }),
        ("data/gear/test.json5", "$.symbols", |source| { source["symbols"] = json!({}); }),
        ("data/gear/test.json5", "$.pieces[0].slots", |source| { source["pieces"][0].as_object_mut().unwrap().remove("slots"); }),
        ("data/gear/test.json5", "$.pieces[0].slots[0]", |source| { source["pieces"][0]["slots"] = json!(["ears"]); }),
        ("data/gear/test.json5", "$.pieces[0].slots[1]", |source| { source["pieces"][0]["slots"] = json!(["main", "main"]); }),
        ("data/gear/test.json5", "$.pieces[0].jobs[0]", |source| { source["pieces"][0]["jobs"][0] = json!("War"); }),
        ("data/gear/test.json5", "$.pieces[0].wsc[1]", |source| { source["pieces"][0]["wsc"] = json!(["STR", "not a number"]); }),
        ("data/enemies.json5", "$.schema_version", |source| { source["schema_version"] = json!(2); }),
        ("data/buffs.json5", "$.schema_version", |source| { source["schema_version"] = json!(2); }),
        ("data/pup-attachments.json5", "$.schema_version", |source| { source["schema_version"] = json!(2); }),
        ("data/enemies.json5", "$.enemies[0][\"Magic Defense\"]", |source| { source["enemies"][0].as_object_mut().unwrap().remove("Magic Defense"); }),
        ("data/buffs.json5", "$.cor.Chaos[\"Attack%\"].potency.XI", |source| { source["cor"]["Chaos"]["Attack%"]["potency"].as_object_mut().unwrap().remove("XI"); }),
        ("data/buffs.json5", "$.brd_song_limits[\"Honor March\"]", |source| { source["brd_song_limits"].as_object_mut().unwrap().remove("Honor March"); }),
        ("data/buffs.json5", "$.misc_debuffs[\"Box Step (sub)\"][\"job requirement\"][0]", |source| { source["misc_debuffs"]["Box Step (sub)"]["job requirement"][0] = json!("dancer"); }),
        ("data/pup-attachments.json5", "$.attachments[0].Capacity", |source| { source["attachments"][0]["Capacity"] = json!("two"); }),
    ];
    for &(file, path, change) in cases {
        let temporary = copied_resources();
        alter(temporary.path(), file, change);
        assert_data_error(temporary.path(), file, path);
    }
}

#[test]
fn one_shared_empty_piece_is_required_but_need_not_be_listed() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/gear/test.json5", |source| {
        source["pieces"][0]["slots"] = json!([]);
        source["pieces"][0]["stats"] = json!({"STR": 5});
    });
    let catalog = Catalog::load(temporary.path()).unwrap();
    assert_eq!(catalog.piece(catalog.empty).name2.as_ref(), "Empty");
    assert_eq!(catalog.piece(catalog.empty).stat(Stat::Str), 5.0);
    assert_eq!(catalog.by_name2("Empty"), None);
    write_fragment(temporary.path(), "z/duplicate-empty.json5", vec![test_piece("Empty", &[])]);
    let message = assert_data_error(temporary.path(), "data/gear/z/duplicate-empty.json5", "$.pieces[0].name2");
    assert!(message.contains("duplicate Empty"), "{message}");
    assert!(message.contains(&temporary.path().join("data/gear/test.json5").display().to_string()), "{message}");
    fs::remove_file(temporary.path().join("data/gear/z/duplicate-empty.json5")).unwrap();
    alter(temporary.path(), "data/gear/test.json5", |source| { source["pieces"][0]["name2"] = json!("No empty piece"); });
    assert_data_error(temporary.path(), "data/gear/test.json5", "$.pieces");
}

#[test]
fn enemy_name_collisions_keep_first_menu_position_and_last_sorted_value() {
    let temporary = copied_resources();
    alter(temporary.path(), "data/enemies.json5", |source| {
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
