use std::fs;

use serde_json::{Map, Value, json};
use tempfile::TempDir;
use wsdist_app::profiles::{ProfileError, ProfileStore, legacy_text};

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn saving_retains_independent_unknowns_other_jobs_and_missing_inputs() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("defaults.json");
    let mut store = ProfileStore::from_document(object(json!({
        "default": {
            "future_option": { "source": "default", "enabled": true },
            "defaults_tp_entry": "1000",
            "defaults_mainjob_combobox": "Ninja"
        },
        "Scholar": {
            "future_option": ["Scholar", 2],
            "defaults_tp_entry": "1900",
            "defaults_mainjob_combobox": "Scholar"
        },
        "Red Mage": {
            "future_option": "unchanged",
            "defaults_tp_entry": "3000"
        }
    }))).unwrap();
    let red_mage = store.profile("Red Mage").unwrap().clone();
    let effective = object(json!({
        "defaults_mainjob_combobox": "Scholar",
        "defaults_whm_checkbox": "0",
        "defaults_geo_checkbox": 0,
        "defaults_shell5_checkbox": "",
        "defaults_brd_checkbox": null,
        "defaults_cor_checkbox": -2,
        "defaults_bubble_bonus_checkbox": "A future geomancy bonus",
        "defaults_optimizer_seed_entry": "not validated by storage",
        "defaults_enemy_level_location_label": "computed visible label",
        "quicklook_main_item": "Quicklook Main +3",
        "tp_quicklook_main_item": "TP Main R30",
        "ws_quicklook_main_item": "WS Main Path B",
        "quicklook_feet_item": "Quicklook Feet",
        "tp_quicklook_feet_item": "TP Feet",
        "ws_quicklook_feet_item": "WS Feet"
    }));

    store.save(&path, "Scholar", &effective).unwrap();
    let reloaded = ProfileStore::load(&path).unwrap();
    assert_eq!(reloaded.document(), store.document());
    assert_eq!(reloaded.profile("Red Mage"), Some(&red_mage));
    assert_eq!(reloaded.profile("default").unwrap()["future_option"],
        json!({ "source": "default", "enabled": true }));
    assert_eq!(reloaded.profile("Scholar").unwrap()["future_option"], json!(["Scholar", 2]));
    assert_eq!(reloaded.profile("default").unwrap()["defaults_tp_entry"], "1000");
    assert_eq!(reloaded.profile("Scholar").unwrap()["defaults_tp_entry"], "1900");
    for name in ["default", "Scholar"] {
        let profile = reloaded.profile(name).unwrap();
        assert_eq!(profile["defaults_whm_checkbox"], json!(true));
        assert_eq!(profile["defaults_geo_checkbox"], json!(false));
        assert_eq!(profile["defaults_shell5_checkbox"], json!(false));
        assert_eq!(profile["defaults_brd_checkbox"], json!(false));
        assert_eq!(profile["defaults_cor_checkbox"], json!(true));
        // The checkbox-looking name is an actual text combo, not a checkbox.
        assert_eq!(profile["defaults_bubble_bonus_checkbox"], "A future geomancy bonus");
        assert_eq!(profile["defaults_optimizer_seed_entry"], "not validated by storage");
        assert_eq!(profile["defaults_enemy_level_location_label"], "computed visible label");
        assert_eq!(profile["quicklook_main_item"], "Quicklook Main +3");
        assert_eq!(profile["tp_quicklook_main_item"], "TP Main R30");
        assert_eq!(profile["ws_quicklook_main_item"], "WS Main Path B");
        assert_eq!(profile["quicklook_feet_item"], "Quicklook Feet");
        assert_eq!(profile["tp_quicklook_feet_item"], "TP Feet");
        assert_eq!(profile["ws_quicklook_feet_item"], "WS Feet");
    }
}

#[test]
fn failed_atomic_replacement_keeps_memory_and_the_existing_destination() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("defaults.json");
    // Replacing a directory with a tempfile deterministically fails on both
    // platforms, after serialization/write/flush/sync rather than before them.
    fs::create_dir(&path).unwrap();
    let existing = path.join("existing-profile.json");
    let original = b"{\"Scholar\":{\"defaults_tp_entry\":\"1900\"}}\n";
    fs::write(&existing, original).unwrap();
    let mut store = ProfileStore::from_document(object(json!({
        "default": { "defaults_tp_entry": "1900", "future": "keep default" },
        "Scholar": { "defaults_tp_entry": "1900", "future": "keep Scholar" }
    }))).unwrap();
    let before = store.document().clone();
    let effective = object(json!({ "defaults_tp_entry": "3000" }));

    let error = store.save(&path, "Scholar", &effective).unwrap_err();
    assert!(matches!(&error, ProfileError::Io { operation: "replace", .. }));
    assert_eq!(error.path(), Some(path.as_path()));
    assert_eq!(store.document(), &before);
    assert_eq!(fs::read(existing).unwrap(), original);
}

#[test]
fn failed_tempfile_creation_preserves_the_original_json_and_store() {
    let directory = TempDir::new().unwrap();
    let original_path = directory.path().join("defaults.json");
    let original = b"{\"Scholar\":{\"defaults_tp_entry\":\"1900\"}}\n";
    fs::write(&original_path, original).unwrap();
    let mut store = ProfileStore::load(&original_path).unwrap();
    let before = store.document().clone();
    let impossible_path = original_path.join("defaults.json");

    let error = store.save(&impossible_path, "Scholar", &object(json!({
        "defaults_tp_entry": "3000"
    }))).unwrap_err();
    assert_eq!(error.path(), Some(impossible_path.as_path()));
    assert!(matches!(error, ProfileError::Io { .. }));
    assert_eq!(store.document(), &before);
    assert_eq!(fs::read(original_path).unwrap(), original);
}

#[test]
fn loading_is_read_only_and_does_not_fall_back_to_default_for_missing_jobs() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("defaults.json");
    let bytes = b" { \"default\" : {\"defaults_tp_entry\" : \"3000\"}, \"Scholar\" : {} } \n";
    fs::write(&path, bytes).unwrap();

    let store = ProfileStore::load(&path).unwrap();
    assert_eq!(store.profile("Scholar"), Some(&Map::new()));
    assert_eq!(store.profile("Scholar").unwrap().get("defaults_tp_entry"), None);
    assert_eq!(store.profile("Ninja"), None);
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn malformed_and_wrong_shape_documents_report_the_path_without_rewriting() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("defaults.json");
    fs::write(&path, b"{\"Scholar\":").unwrap();
    let error = ProfileStore::load(&path).unwrap_err();
    assert!(matches!(&error, ProfileError::Json { .. }));
    assert_eq!(error.path(), Some(path.as_path()));
    assert_eq!(fs::read(&path).unwrap(), b"{\"Scholar\":");

    for (bytes, field) in [
        (b"[]".as_slice(), "$"),
        (b"{\"Scholar\":true}".as_slice(), "$[\"Scholar\"]"),
    ] {
        fs::write(&path, bytes).unwrap();
        let error = ProfileStore::load(&path).unwrap_err();
        assert_eq!(error.path(), Some(path.as_path()));
        match error {
            ProfileError::Shape { field: actual, .. } => assert_eq!(actual, field),
            error => panic!("expected a document shape error, got {error}"),
        }
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn unreadable_input_reports_its_path_and_does_not_recreate_it() {
    let directory = TempDir::new().unwrap();
    let blocking_file = directory.path().join("blocking-file");
    fs::write(&blocking_file, b"not a directory").unwrap();
    let path = blocking_file.join("defaults.json");

    let error = ProfileStore::load(&path).unwrap_err();
    assert_eq!(error.path(), Some(path.as_path()));
    assert!(matches!(error, ProfileError::Io { .. }));
    assert_eq!(fs::read(blocking_file).unwrap(), b"not a directory");
}

#[test]
fn scalar_restore_conversion_uses_python_spelling_at_float_boundaries() {
    for (value, expected) in [
        (json!(null), "None"),
        (json!(true), "True"),
        (json!(false), "False"),
        (json!(42), "42"),
        (json!(-0.0), "-0.0"),
        (json!(1.0), "1.0"),
        (json!(1e-4), "0.0001"),
        (json!(1e-5), "1e-05"),
        (json!(1e15), "1000000000000000.0"),
        (json!(1e16), "1e+16"),
        (json!(1.25e20), "1.25e+20"),
    ] {
        assert_eq!(legacy_text(&value), expected);
    }
}
