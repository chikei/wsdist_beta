use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Map, Value, json};
use tempfile::TempDir;
use wsdist_app::exchange::{
    BulkAction, BulkFilters, GearSource, ImportOutcome, InventoryError,
    import_inventory_file, select_bulk,
};
use wsdist_app::profile_registry::{INPUT_REGISTRY, InputKind, input_spec};
use wsdist_app::profiles::{GEAR_PROFILE_KEYS, ProfileError, ProfileStore, legacy_checkbox};
use wsdist_app::state::UiState;
use wsdist_core::model::{CalcError, GearId, Gearset, Job, Slot};
use wsdist_core::rng::RandomSources;

mod ui_support;
use ui_support::{
    assert_checkbox_model, assert_last_player, assert_number, assert_piece_state,
    assert_radio_model, assert_snapshot, case_entries, catalog, document, fixture,
};

type StepResult<E = Box<dyn Error>> = Result<(), E>;

struct NativeDriver {
    state: UiState,
    profiles: ProfileStore,
    directory: TempDir,
    saved_job: Option<String>,
}

fn source(value: &Value) -> GearSource {
    match value.as_str().unwrap() {
        "quicklook" => GearSource::Quicklook,
        "tp" => GearSource::Tp,
        "ws" => GearSource::Ws,
        name => panic!("unknown frozen gear source {name}"),
    }
}

fn slot(value: &Value) -> Slot {
    Slot::from_name(value.as_str().unwrap()).unwrap()
}

impl NativeDriver {
    fn new(fixture: &Value) -> Self {
        let input = &fixture["input"];
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("defaults.json");
        let profiles = if input["profile_document"].is_null() {
            let mut blank = Map::new();
            blank.insert("default".into(), json!({}));
            for job in Job::ALL { blank.insert(job.display_name().into(), json!({})); }
            ProfileStore::from_document(blank).unwrap()
        } else {
            fs::write(&path, serde_json::to_vec(&input["profile_document"]).unwrap()).unwrap();
            ProfileStore::load(&path).unwrap()
        };
        let restored = match input["bootstrap"].as_str().unwrap() {
            "shipped_profiles" => Some(&profiles),
            "initialized_without_profiles" => None,
            name => panic!("unknown frozen bootstrap {name}"),
        };
        let random = RandomSources::from_seed(input["host_seed"].as_u64().unwrap());
        Self { state: UiState::new(catalog(), random, restored), profiles, directory, saved_job: None }
    }

    fn profile_path(&self) -> PathBuf { self.directory.path().join("defaults.json") }

    fn piece(&self, record: &Value) -> GearId {
        // The actual Qt equipment callback receives name2, and all_gear is a
        // last-value-wins index. An original-ID lookup would select the wrong
        // duplicate record even though the fixture's input contains that ID.
        self.state.engine.catalog.by_name2(record["name2"].as_str().unwrap()).unwrap()
    }

    fn filters(&self) -> BulkFilters {
        BulkFilters {
            odyssey_rank: self.state.text("defaults_ody_rank_combobox").into(),
            nyame_r25_b: self.state.checked("defaults_nyame25_checkbox"),
            soa_ring: self.state.text("defaults_soa_ring_combobox").into(),
            tvr_ring: self.state.text("defaults_tvr_ring_combobox").into(),
            mastery_rank: self.state.text("defaults_mastery_rank_combobox").into(),
        }
    }

    fn load_profile(&mut self, path: &std::path::Path, reason: &str) -> StepResult {
        let previous = self.profiles.document().clone();
        let bytes = fs::read(self.profile_path()).ok();
        let parsed = ProfileStore::load(path);
        // Reading never rewrites either valid or malformed user JSON.
        assert_eq!(fs::read(self.profile_path()).ok(), bytes);
        match parsed {
            Ok(store) => {
                self.profiles = store;
                self.state.load_profile(&self.profiles, reason);
                Ok(())
            }
            Err(error) => {
                assert_eq!(self.profiles.document(), &previous, "failed resource load retains the previous in-memory profile document");
                Err(error.into())
            }
        }
    }

    fn apply(&mut self, step: &Value) -> StepResult {
        match step["action"].as_str().unwrap() {
            "restore_widget" => self.state.set_raw(step["key"].as_str().unwrap(), &step["value"] )?,
            "activate" => {
                let key = step["key"].as_str().unwrap();
                if key == "defaults_mainjob_combobox" && self.profile_path().is_file() {
                    // A job activation reads the current disk document, unlike
                    // a raw widget restore or writing a profile file alone.
                    self.profiles = ProfileStore::load(&self.profile_path())?;
                }
                self.state.activate(key, &step["value"], Some(&self.profiles))?;
            }
            "check" => {
                let key = step["key"].as_str().unwrap();
                let checked = step["value"].as_bool().unwrap();
                // The oracle clicks only when the requested state differs.
                // Re-activating an already-checked mutually exclusive source
                // would incorrectly clear another restored checkbox.
                if self.state.checked(key) != checked {
                    self.state.activate(key, &step["value"], Some(&self.profiles))?;
                }
            }
            "equipment_callback" => {
                let item = self.piece(&step["piece"]);
                self.state.select_gear(source(&step["source"]), slot(&step["slot"]), item)?;
            }
            "copy_button" => {
                let (from, to) = match step["route"].as_str().unwrap() {
                    "quicklook to tp" => (GearSource::Quicklook, GearSource::Tp),
                    "quicklook to ws" => (GearSource::Quicklook, GearSource::Ws),
                    "tp to quicklook" => (GearSource::Tp, GearSource::Quicklook),
                    "ws to quicklook" => (GearSource::Ws, GearSource::Quicklook),
                    route => panic!("unknown frozen copy route {route}"),
                };
                self.state.copy_set(from, to);
            }
            "equip_best_set_signal" => {
                let set: Gearset = std::array::from_fn(|index| self.piece(&step["gearset"][Slot::ALL[index].name()]));
                self.state.equip_best_set(set);
            }
            "profile_file" => {
                let path = self.profile_path();
                if step["remove"].as_bool().unwrap_or(false) {
                    fs::remove_file(&path).unwrap();
                } else {
                    let bytes = match step.get("raw") {
                        Some(raw) => raw.as_str().unwrap().as_bytes().to_vec(),
                        None => serde_json::to_vec(&step["document"]).unwrap(),
                    };
                    fs::write(&path, bytes).unwrap();
                }
                if let Some(reason) = step["load_type"].as_str() {
                    // The frozen permission-0 failure is transport, explicitly
                    // replaced by visible inert-resource errors. A file ancestor
                    // produces a deterministic unreadable path on Windows,
                    // Unix and privileged CI, without changing user permissions.
                    let read_path = if step.get("permissions").is_some() {
                        path.join("unreadable").join("defaults.json")
                    } else { path };
                    self.load_profile(&read_path, reason)?;
                }
            }
            "load_defaults" => self.load_profile(&self.profile_path(), step["type"].as_str().unwrap())?,
            "save_defaults" => {
                let path = self.profile_path();
                let job = self.state.text("defaults_mainjob_combobox").to_owned();
                self.profiles.save(&path, &job, &self.state.effective_profile())?;
                self.saved_job = Some(job);
                assert_eq!(ProfileStore::load(&path).unwrap().document(), self.profiles.document(), "saved bytes and committed store agree");
            }
            "tp_edit_finished" => {
                self.state.set_raw("defaults_tp_entry", &step["value"])?;
                self.state.commit_tp_entry();
            }
            "tp_wheel" => {
                self.state.set_raw("defaults_tp_entry", &step["value"])?;
                self.state.wheel_tp(step["delta"].as_i64().unwrap().signum() as i32);
            }
            "optimizer_bulk" => {
                let action = match step["event"].as_str().unwrap() {
                    "select all" => BulkAction::SelectAll,
                    event => panic!("unknown frozen bulk action {event}"),
                };
                let selected = select_bulk(&self.state.engine.catalog, &self.state.gear.visible_candidates(), &self.state.gear.candidate_pools(), &self.filters(), action);
                self.state.gear.replace_candidates(&selected);
            }
            "inventory_dialog" => {
                let path = match step["mode"].as_str().unwrap() {
                    "file" => {
                        let path = self.directory.path().join("inventory-input.lua");
                        fs::write(&path, step["contents"].as_str().unwrap()).unwrap();
                        Some(path)
                    }
                    "cancel" => None,
                    "missing" => Some(self.directory.path().join("does-not-exist.lua")),
                    mode => panic!("unknown frozen inventory mode {mode}"),
                };
                let mut selected = self.state.gear.candidate_pools();
                let outcome = import_inventory_file(&self.state.engine.catalog, &self.state.gear.visible_candidates(), &self.filters(), &mut selected, path.as_deref())?;
                assert_eq!(outcome, if path.is_none() { ImportOutcome::Cancelled } else { ImportOutcome::Completed });
                self.state.gear.replace_candidates(&selected);
            }
            "menu_flag" => match step["attribute"].as_str().unwrap() {
                "damage_limit99999" => self.state.damage_limit = step["value"].as_bool().unwrap(),
                "verbose_swaps" => self.state.verbose_swaps = step["value"].as_bool().unwrap(),
                attribute => panic!("unknown frozen menu flag {attribute}"),
            },
            action => panic!("unimplemented frozen action {action}: {step}"),
        }
        Ok(())
    }
}

fn require_replacement(expected: &Value, name: &str) {
    assert_eq!(expected["intentional_replacements"][name]["not_a_golden_for_changed_transport"], true, "native transport projection must be explicitly marked by the immutable fixture");
}

fn native_profile_document(driver: &NativeDriver, fixture: &Value, expected: &Value) -> Map<String, Value> {
    let valid_shape = expected.as_object().is_some_and(|document| document.values().all(Value::is_object));
    let mut document = if valid_shape { expected.as_object().unwrap().clone() } else {
        require_replacement(&fixture["expected"]["value"], "profile_error_and_unknown_key_handling");
        // Python accepted a list/null into window.states. The native resource
        // boundary rejects that shape and retains the last valid document.
        fixture["input"]["profile_document"].as_object().unwrap().clone()
    };
    if let Some(job) = &driver.saved_job {
        require_replacement(&fixture["expected"]["value"], "profile_error_and_unknown_key_handling");
        let originals = fixture["input"]["save_profile_document"].as_object().unwrap();
        for name in ["default", job.as_str()] {
            let profile = document.get_mut(name).unwrap().as_object_mut().unwrap();
            for spec in INPUT_REGISTRY {
                if spec.kind == InputKind::Checkbox && let Some(value) = profile.get_mut(spec.key) {
                    *value = Value::Bool(legacy_checkbox(value));
                }
            }
            // The legacy default/current objects alias and lose independent
            // unknown keys. Preserve each original unknown under its own job;
            // every registered control and all 48 gear references remain golden.
            profile.retain(|key, _| input_spec(key).is_some() || GEAR_PROFILE_KEYS.contains(&key.as_str()));
            if let Some(original) = originals.get(name).and_then(Value::as_object) {
                for (key, value) in original {
                    if input_spec(key).is_none() && !GEAR_PROFILE_KEYS.contains(&key.as_str()) {
                        profile.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }
    document
}

fn assert_profile_row(driver: &NativeDriver, fixture: &Value, row: &Value, label: &str) {
    if let Some(present) = row.get("profile_file_present") {
        assert_eq!(driver.profile_path().is_file(), present.as_bool().unwrap(), "{label}: profile file presence");
        assert_eq!(driver.profiles.document(), &native_profile_document(driver, fixture, &row["in_memory_profile_document"]), "{label}: effective profile document");
        if let Some(expected_file) = row.get("profile_file_document") {
            let actual = document(&driver.profile_path());
            let expected_file = if driver.saved_job.is_some() {
                Value::Object(native_profile_document(driver, fixture, expected_file))
            } else { expected_file.clone() };
            assert_eq!(actual, expected_file, "{label}: actual persisted document");
        }
    }
}

fn assert_step_outcome(driver: &NativeDriver, fixture: &Value, step: &Value, row: &Value, phase: &str, result: StepResult, first_message: usize) {
    let expected = &fixture["expected"]["value"];
    let events = expected["events"].as_array().unwrap();
    let start = usize::try_from(row["event_start"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(row["event_end"].as_u64().unwrap()).unwrap();
    let errors = events[start..end].iter().filter(|event|
        event["phase"] == phase && event["kind"] == "callback_error")
        .map(|event| &event["error"]).collect::<Vec<_>>();
    let action = step["action"].as_str().unwrap();
    if let Err(error) = result {
        if let Some(error) = error.downcast_ref::<ProfileError>() {
            require_replacement(expected, "profile_error_and_unknown_key_handling");
            let read_path = if step.get("permissions").is_some() {
                driver.profile_path().join("unreadable").join("defaults.json")
            } else { driver.profile_path() };
            assert_eq!(error.path(), Some(read_path.as_path()), "{phase}: resource error retains its actual path");
            match fixture["id"].as_str().unwrap() {
                "ui.profile-fault.malformed-json" => assert!(matches!(error, ProfileError::Json { .. })),
                "ui.profile-fault.wrong-root-type" => assert!(matches!(error, ProfileError::Shape { field, .. } if field == "$")),
                "ui.profile-fault.wrong-profile-type" => assert!(matches!(error, ProfileError::Shape { field, .. } if field == "$[\"default\"]")),
                "ui.profile-fault.unreadable-json" | "ui.profile-fault.missing-json-no-pickle" => assert!(matches!(error, ProfileError::Io { .. })),
                id => panic!("{id}/{phase}: unexpected native profile failure {error}"),
            }
        } else if let Some(error) = error.downcast_ref::<InventoryError>() {
            require_replacement(expected, "inventory_cancel_and_read_error");
            assert_eq!(action, "inventory_dialog");
            assert_eq!(step["mode"], "missing");
            assert_eq!(error.path, driver.directory.path().join("does-not-exist.lua"));
            assert_eq!(error.source.kind(), std::io::ErrorKind::NotFound);
        } else if let Some(error) = error.downcast_ref::<CalcError>() {
            let CalcError::Legacy { category, field, .. } = error else { panic!("{phase}: unexpected calculation error {error:?}") };
            let expected_field = match step["key"].as_str().unwrap() {
                "defaults_food_combobox" => "food",
                key => key,
            };
            assert_eq!(*field, expected_field, "{phase}: field-specific callback error");
            assert!(errors.iter().any(|expected| expected["class"] == *category), "{phase}: {error} has no captured callback error");
        } else {
            panic!("{phase}: unexpected native failure {error:?}");
        }
    } else if !errors.is_empty() {
        // Profile restoration deliberately turns callback exceptions into an
        // accessible action/field diagnostic, but preserves their partial state.
        for error in &errors {
            let category = error["class"].as_str().unwrap();
            assert!(driver.state.messages[first_message..].iter().any(|message| message.contains(category)), "{phase}: missing native {category} restore diagnostic: {:?}", &driver.state.messages[first_message..]);
        }
    }
    if let Some(error) = row.get("error") {
        assert!(matches!(error["class"].as_str().unwrap(), "JSONDecodeError" | "PermissionError" | "FileNotFoundError"), "{phase}: uncategorized frozen resource exception");
    }
}

fn drive_fixture(fixture: &Value) {
    let expected = &fixture["expected"]["value"];
    let label = fixture["id"].as_str().unwrap();
    let mut driver = NativeDriver::new(fixture);
    assert_snapshot(&driver.state, &expected["initial_state"], &format!("{label}.initial"));
    assert_last_player(&driver.state, expected, "startup", label);
    let mut checked_reference = None;
    for (key, phase) in [("setup", "setup"), ("steps", "step")] {
        let Some(steps) = fixture["input"][key].as_array() else { continue; };
        let rows = expected[key].as_array().unwrap();
        assert_eq!(steps.len(), rows.len(), "{label}: complete {key} capture");
        for (index, (step, row)) in steps.iter().zip(rows).enumerate() {
            let phase = format!("{phase}.{index}");
            let label = format!("{label}.{phase}");
            let first_message = driver.state.messages.len();
            let retained_inventory = step["action"] == "inventory_dialog" &&
                matches!(step["mode"].as_str(), Some("cancel" | "missing"));
            let before_candidates = retained_inventory.then(|| driver.state.gear.candidate_pools());
            let result = driver.apply(step);
            assert_step_outcome(&driver, fixture, step, row, &phase, result, first_message);
            if let Some(before) = before_candidates {
                require_replacement(expected, "inventory_cancel_and_read_error");
                assert_eq!(driver.state.gear.candidate_pools(), before, "{label}: cancelled/failed import retains the complete prior selection");
            }
            if key == "steps" {
                assert_snapshot(&driver.state, &row["state"], &label);
            }
            assert_last_player(&driver.state, expected, &phase, &label);
            if let Some(models) = row.get("optimizer_selection") {
                let models = if retained_inventory {
                    checked_reference.expect("cancel/read failure has a preceding frozen selected model")
                } else {
                    checked_reference = Some(models);
                    models
                };
                for slot in Slot::ALL {
                    assert_checkbox_model(&driver.state, &driver.state.gear.candidates[slot.index()], &models[slot.name()], &format!("{label}.{}", slot.name()));
                }
            }
            assert_profile_row(&driver, fixture, row, &label);
        }
        if key == "setup" && let Some(snapshot) = expected.get("state_after_setup") {
            assert_snapshot(&driver.state, snapshot, &format!("{label}.after setup"));
        }
    }
    assert_snapshot(&driver.state, &expected["final_state"], &format!("{label}.final"));
    assert_eq!(driver.profiles.document(), &native_profile_document(&driver, fixture, &expected["in_memory_profile_document"]), "{label}: final profile document");
    if fixture["input"]["relaunch_after_save"].as_bool().unwrap_or(false) {
        let reloaded = ProfileStore::load(&driver.profile_path()).unwrap();
        let relaunched = UiState::new(catalog(), RandomSources::from_seed(11), Some(&reloaded));
        assert_snapshot(&relaunched, &expected["relaunch_state"], &format!("{label}.actual file relaunch"));
        assert_last_player(&relaunched, expected, "relaunch", label);
        assert_eq!(reloaded.document(), driver.profiles.document(), "{label}: relaunch reads the saved file, not an in-memory snapshot");
    }
}

#[test]
fn every_frozen_controller_transition_preserves_intermediate_effective_inputs_and_hidden_models() {
    let entries = case_entries("ui.transitions");
    assert_eq!(entries.len(), 215);
    for entry in entries { drive_fixture(&fixture(&entry)); }
}

#[test]
fn captured_save_relaunch_roundtrips_actual_files_and_independent_unknown_profile_keys() {
    let entries = case_entries("ui.save");
    assert_eq!(entries.len(), 1);
    drive_fixture(&fixture(&entries[0]));
}

#[test]
fn complete_virtual_model_transitions_retain_hidden_radio_and_full_checked_state() {
    let entries = case_entries("ui.virtual");
    assert_eq!(entries.len(), 1);
    let fixture = fixture(&entries[0]);
    let expected = &fixture["expected"]["value"];
    let mut driver = NativeDriver::new(&fixture);
    assert_snapshot(&driver.state, &expected["initial_state"], "virtual.initial");
    let slot = slot(&fixture["input"]["slot"]);
    let pieces = fixture["input"]["pieces"].as_array().unwrap();
    let first = driver.piece(&pieces[0]);
    let second = driver.piece(&pieces[1]);
    let radio_index = GearSource::Quicklook.index();
    let outside = driver.state.engine.catalog.pieces().iter().find(|piece|
        !driver.state.gear.radios[radio_index][slot.index()].items.contains(piece.id)).unwrap().id;
    let mut rows = expected["virtual_rows"].as_array().unwrap().iter();
    driver.state.gear.radios[radio_index][slot.index()].items.visible = Arc::from([first, second]);
    for operation in fixture["input"]["radio_operations"].as_array().unwrap() {
        let operation = operation.as_str().unwrap();
        let row = rows.next().unwrap();
        assert_eq!(row["kind"], "radio");
        assert_eq!(row["operation"], operation);
        match operation {
            "native-select-first" => driver.state.select_gear(GearSource::Quicklook, slot, first).unwrap(),
            "native-select-second" => driver.state.select_gear(GearSource::Quicklook, slot, second).unwrap(),
            "hide-first" => driver.state.gear.radios[radio_index][slot.index()].items.visible = Arc::from([second]),
            "programmatic-select-hidden" => driver.state.gear.radios[radio_index][slot.index()].set_selected(first),
            "restore-both" => driver.state.gear.radios[radio_index][slot.index()].items.visible = Arc::from([first, second]),
            // Native models have typed IDs, not arbitrary strings. An ID outside
            // this full model exercises the same rejected-selection boundary.
            "programmatic-unknown" => driver.state.gear.radios[radio_index][slot.index()].set_selected(outside),
            "hide-all" => driver.state.gear.radios[radio_index][slot.index()].items.visible = Arc::from([]),
            operation => panic!("unimplemented frozen radio transition {operation}"),
        }
        let label = format!("virtual.radio.{operation}");
        assert_radio_model(&driver.state, &driver.state.gear.radios[radio_index][slot.index()], &row["model"], &label);
        assert_piece_state(&driver.state, driver.state.gear.equipped[radio_index][slot.index()], &row["equipped"], &format!("{label}.equipped"));
    }
    let checks = &mut driver.state.gear.candidates[slot.index()];
    checks.items.visible = Arc::from([first, second]);
    checks.clear();
    for operation in fixture["input"]["checkbox_operations"].as_array().unwrap() {
        let operation = operation.as_str().unwrap();
        let row = rows.next().unwrap();
        assert_eq!(row["kind"], "checkbox");
        assert_eq!(row["operation"], operation);
        let checks = &mut driver.state.gear.candidates[slot.index()];
        match operation {
            "native-check-first" => checks.set_checked(first, true),
            "hide-first" => checks.items.visible = Arc::from([second]),
            "select-visible" | "deselect-visible" => {
                for id in checks.items.visible.to_vec() { checks.set_checked(id, operation == "select-visible"); }
            }
            "restore-both" => checks.items.visible = Arc::from([first, second]),
            "deselect-unknown" => checks.set_checked(outside, false),
            "select-all" => {
                for id in checks.items.unique.to_vec() { checks.set_checked(id, true); }
            }
            "deselect-first" => checks.set_checked(first, false),
            "deselect-all" => checks.clear(),
            operation => panic!("unimplemented frozen checkbox transition {operation}"),
        }
        assert_checkbox_model(&driver.state, &driver.state.gear.candidates[slot.index()], &row["model"], &format!("virtual.checkbox.{operation}"));
    }
    assert!(rows.next().is_none(), "all frozen virtual model states are consumed");
    assert_snapshot(&driver.state, &expected["final_state"], "virtual.final");
}

#[test]
fn tp_user_commit_preserves_unicode_integer_text_and_wheel_normalizes_by_direction() {
    let mut state = ui_support::seeded(None);
    for (text, committed, effective) in [
        ("", "1000", 1000), ("999", "1000", 1000), ("3001", "3000", 3000),
        ("malformed", "1000", 1000), ("  +１_９００ ", "  +１_９００ ", 1900),
        ("١٨٠٠", "١٨٠٠", 1800), ("２９９９", "２９９９", 2999),
        ("  +３００１ ", "3000", 3000), ("-٩", "1000", 1000),
    ] {
        state.set_raw("defaults_tp_entry", &json!(text)).unwrap();
        assert_eq!(state.text("defaults_tp_entry"), text, "raw restore must not normalize before the user commits");
        state.commit_tp_entry();
        assert_eq!(state.text("defaults_tp_entry"), committed, "focus loss for {text:?}");
        assert_number(state.calculation_inputs(GearSource::Quicklook).unwrap().tp, &json!(effective), "committed effective TP");
    }
    for (text, direction, expected) in [
        ("1000", -1, 1000), ("1000", 1, 1100), ("3000", 1, 3000),
        ("3000", -1, 2900), ("１８５０", 1, 1950), ("malformed", 1, 1000),
        ("1900", 7, 2000), ("1900", -7, 1800),
    ] {
        state.set_raw("defaults_tp_entry", &json!(text)).unwrap();
        state.wheel_tp(direction);
        assert_eq!(state.text("defaults_tp_entry"), expected.to_string(), "wheel direction {direction} for {text:?}");
        assert_number(state.calculation_inputs(GearSource::Quicklook).unwrap().tp, &json!(expected), "wheel effective TP");
    }
}
