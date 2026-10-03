use serde_json::{Value, json};
use wsdist_app::exchange::GearSource;
use wsdist_app::input_metadata::{ABILITY_SPECS, ENEMY_FIELDS};
use wsdist_app::profiles::ProfileStore;
use wsdist_core::model::Slot;

mod ui_support;
use ui_support::{assert_number, assert_snapshot, cases, names, seeded, support};

#[test]
fn native_initialized_state_matches_every_effective_source_input_and_selection_model() {
    let fixture = cases("ui.metadata").remove(0);
    let state = seeded(None);
    assert!(state.messages.is_empty(), "initialization errors: {:?}", state.messages);
    assert_snapshot(&state, &fixture["expected"]["value"]["initial_state"], "initialized controller");
}

#[test]
fn all_shipped_profiles_restore_in_source_order_with_exact_effective_players() {
    let cases = cases("ui.profile");
    assert_eq!(cases.len(), 23);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap();
        let store = ProfileStore::from_document(fixture["input"]["profile_document"].as_object().unwrap().clone()).unwrap();
        let mut state = seeded(Some(&store));
        if fixture["input"]["profile_name"] == "default" {
            assert_snapshot(&state, &fixture["expected"]["value"]["initial_state"], "shipped default startup");
        }
        let step = &fixture["input"]["steps"][0];
        match step["action"].as_str().unwrap() {
            "activate" => state.activate(step["key"].as_str().unwrap(), &step["value"], Some(&store)).unwrap(),
            "load_defaults" => state.load_profile(&store, step["type"].as_str().unwrap()),
            action => panic!("unexpected profile activation {action}"),
        }
        assert!(state.messages.is_empty(), "{label}: restore errors {:?}", state.messages);
        assert_snapshot(&state, &fixture["expected"]["value"]["steps"][0]["state"], label);
        let build = fixture["expected"]["value"]["player_builds"].as_array().unwrap().iter().rev().find(|build| build["phase"] == "step.0").unwrap();
        let player = state.last_player.as_ref().unwrap();
        support::assert_metadata(player, &build["final"], label);
        support::assert_sheet(&player.stats, &player.wsc, player.wyvern_bonus_attack, &build["final"], label);
    }
}

#[test]
fn complete_job_subjob_level_availability_keeps_unchecking_and_literal_menu_transitions() {
    let fixture = cases("ui.availability").remove(0);
    let input = &fixture["input"];
    let rows = fixture["expected"]["value"]["availability_rows"].as_array().unwrap();
    let mut state = seeded(None); let mut previous_main = String::new(); let mut previous_sub = String::new();
    assert_eq!(rows.len(), 2420);
    for (index, row) in rows.iter().enumerate() {
        let main = row["input"]["main_job_display"].as_str().unwrap();
        let sub = row["input"]["sub_job_display"].as_str().unwrap();
        if main != previous_main {
            state.set_raw("defaults_mainjob_combobox", &json!(main)).unwrap();
            state.update_job("main static", None).unwrap();
            previous_main = main.to_owned(); previous_sub.clear();
        }
        if sub != previous_sub {
            state.activate("defaults_subjob_combobox", &json!(sub), None).unwrap();
            previous_sub = sub.to_owned();
        }
        for name in input["checked_before_each_row"].as_array().unwrap() {
            let spec = ABILITY_SPECS.iter().find(|spec| spec.name == name.as_str().unwrap()).unwrap();
            state.set_raw(spec.key, &Value::Bool(true)).unwrap();
        }
        state.activate("defaults_ml_combobox", &json!(row["input"]["master_level"].as_i64().unwrap().to_string()), None).unwrap();
        assert_eq!(state.sub_job_level as i64, row["ui_sub_job_level"].as_i64().unwrap(), "row {index}: level");
        assert_eq!(state.enhancing_skill_label, row["enhancing_skill_label"], "row {index}: skill kind");
        assert_eq!(state.text("defaults_enhancing_skill_entry"), row["enhancing_skill"].as_str().unwrap(), "row {index}: skill reset");
        for spec in ABILITY_SPECS {
            let control = &state.controls[spec.key];
            assert_eq!(control.value, row["abilities"][spec.name]["checked"], "row {index}: {} checked", spec.name);
            assert_eq!(control.hidden, row["abilities"][spec.name]["explicit_hidden"].as_bool().unwrap(), "row {index}: {} hidden", spec.name);
        }
        for (key, expected) in [("defaults_spell_combobox", "spell_choices"), ("defaults_subjob_combobox", "subjob_choices"), ("defaults_boost_combobox", "boost_choices")] {
            assert_eq!(json!(state.controls[key].choices), row[expected], "row {index}: {key}");
        }
        assert_eq!(json!(names(&state, &state.gear.radios[0][Slot::Sub.index()].items.visible)), row["sub_radio_choices"], "row {index}: offhand choices");
        for (cache, key) in ["Dia", "Haste", "Boost", "Storm"].iter().enumerate() {
            assert_eq!(state.whm_selections[cache], row["cached_whm_selections"][key], "row {index}: source cache {key}");
        }
    }
}

#[test]
fn missing_profile_inputs_and_gear_keep_values_and_report_only_missing_names() {
    let mut state = seeded(None);
    let old_food = state.cached_food;
    state.set_raw("defaults_tp_entry", &json!("1777")).unwrap();
    let gear = state.engine.catalog.by_name2("Heishi Shorinken").unwrap();
    state.select_gear(GearSource::Tp, Slot::Main, gear).unwrap();
    let store = ProfileStore::from_document(json!({"Scholar": {"defaults_bubble_bonus_checkbox": "Geomancy +17", "tp_quicklook_main_item": "Removed weapon"}, "default": {}}).as_object().unwrap().clone()).unwrap();
    state.load_profile(&store, "main");
    assert_eq!(state.text("defaults_tp_entry"), "1777");
    assert_eq!(state.gear.equipped[1][Slot::Main.index()], gear);
    assert_eq!(state.text("defaults_bubble_bonus_checkbox"), "Geomancy +17");
    assert_eq!(state.cached_food, old_food);
    assert_eq!(state.messages, ["Missing gear in tp_quicklook_main_item: Removed weapon"]);
    let before = state.effective_profile();
    state.load_profile(&store, "default");
    assert_eq!(state.effective_profile(), before, "empty profile is not a default reset");
}

#[test]
fn raw_restore_retains_stale_cache_but_direct_activation_keeps_calculation_inputs_coherent() {
    let mut state = seeded(None);
    state.set_raw("defaults_dia_combobox", &json!("Dia III")).unwrap();
    assert_eq!(state.whm_selections[0], "Dia II", "restoring a combo is not an activation");
    state.activate("defaults_dia_combobox", &json!("Dia III"), None).unwrap();
    assert_eq!(state.whm_selections[0], "Dia III");
    state.set_raw("defaults_ml_combobox", &json!("50")).unwrap();
    state.update_job("master_level", None).unwrap();
    assert_eq!(state.sub_job_level, 55, "legacy underscore callback does not refresh the cache");
    state.activate("defaults_ml_combobox", &json!("50"), None).unwrap();
    assert_eq!(state.sub_job_level, 59);
    state.set_raw("defaults_tp_entry", &json!("  +１_９００ ")).unwrap();
    assert_number(state.calculation_inputs(GearSource::Quicklook).unwrap().tp, &json!(1900), "Python integer entry");
    state.set_raw(ENEMY_FIELDS[0].1, &json!("-0")).unwrap();
    assert_eq!(state.integer(ENEMY_FIELDS[0].1).unwrap().to_bits(), 0.0f64.to_bits(), "integer negative zero becomes positive zero");
}
