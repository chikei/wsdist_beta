#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use wsdist_fixtures::{Value, to_value};
use wsdist_app::exchange::GearSource;
use wsdist_app::input_metadata::ABILITY_SPECS;
use wsdist_app::profile_registry::{INPUT_REGISTRY, InputKind};
use wsdist_app::profiles::ProfileStore;
use wsdist_app::selections::{CheckboxModel, RadioModel};
use wsdist_app::state::UiState;
use wsdist_core::Catalog;
use wsdist_core::buffs::aggregate_buffs;
use wsdist_core::model::{Abilities, Ability, CalcError, GearId, Job, Slot};
use wsdist_core::rng::RandomSources;
use wsdist_core::stats::Stats;

#[path = "../../../wsdist-core/tests/legacy_player/support.rs"]
#[allow(dead_code)]
pub mod support;
pub use support::{assert_number, document_json};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

pub fn catalog() -> Arc<Catalog> {
    static CATALOG: LazyLock<Arc<Catalog>> = LazyLock::new(|| {
        Arc::new(Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap())
    });
    Arc::clone(&CATALOG)
}

pub fn case_entries(operation: &str) -> Vec<Value> {
    let index = document_json(&root().join("ui.json"));
    assert_eq!(index["schema_version"], 3, "unsupported UI fixture index schema");
    index["cases"].as_array().unwrap().iter()
        .filter(|case| case["operation"] == operation).cloned().collect()
}

pub fn fixture(entry: &Value) -> Value {
    support::fixture(&root(), entry)
}

pub fn cases(operation: &str) -> Vec<Value> {
    case_entries(operation).iter().map(fixture).collect()
}

pub fn seeded(profiles: Option<&ProfileStore>) -> UiState {
    UiState::new(catalog(), RandomSources::from_seed(11), profiles)
}

pub fn names(state: &UiState, ids: &[GearId]) -> Vec<String> {
    ids.iter().map(|&id| state.engine.catalog.piece(id).name2.to_string()).collect()
}

pub fn assert_stats(actual: &Stats, expected: &Value, label: &str) {
    let expected = expected.as_object().unwrap();
    assert_eq!(actual.len(), expected.len(), "{label}: stat presence");
    for ((stat, actual), (name, expected)) in actual.ordered().zip(expected) {
        assert_eq!(stat.name(), name, "{label}: ordered stat identity");
        assert_number(actual, expected, &format!("{label}.{name}"));
    }
}

pub fn assert_piece_state(state: &UiState, id: GearId, expected: &Value, label: &str) {
    let piece = state.engine.catalog.piece(id);
    support::assert_piece(piece, None, &expected["original"], &format!("{label}.original"));
    support::assert_piece(piece, Some(&state.engine.state), &expected["effective"], &format!("{label}.effective"));
}

pub fn assert_radio_model(state: &UiState, actual: &RadioModel, expected: &Value, label: &str) {
    assert_eq!(to_value(names(state, &actual.items.master)).unwrap(), expected["master_data"], "{label}: full master order");
    assert_eq!(to_value(names(state, &actual.items.visible)).unwrap(), expected["visible_data"], "{label}: visible order");
    assert_eq!(actual.selected.map(|id| state.engine.catalog.piece(id).name2.as_ref()).unwrap_or(""), expected["selected"].as_str().unwrap(), "{label}: retained selection");
    let visible = actual.visible_selection().map(|id| Value::from(state.engine.catalog.piece(id).name2.as_ref())).unwrap_or(Value::Null);
    assert_eq!(visible, expected["visible_current"], "{label}: visible current item");
}

pub fn assert_checkbox_model(state: &UiState, actual: &CheckboxModel, expected: &Value, label: &str) {
    assert_eq!(to_value(names(state, &actual.items.master)).unwrap(), expected["master_data"], "{label}: full master order");
    assert_eq!(to_value(names(state, &actual.items.visible)).unwrap(), expected["visible_data"], "{label}: visible order");
    assert_eq!(to_value(names(state, &actual.selected())).unwrap(), expected["selected"], "{label}: full checked pool including hidden entries");
    let source_checks = expected["selection_state"].as_object().unwrap();
    assert_eq!(actual.items.unique.len(), source_checks.len(), "{label}: checked-state domain");
    for &id in actual.items.unique.iter() {
        let name = state.engine.catalog.piece(id).name2.as_ref();
        assert_eq!(actual.is_checked(id), source_checks[name].as_bool().unwrap(), "{label}: hidden/full check {name}");
    }
    let visible_checks = actual.items.visible.iter().map(|&id| Value::Object([
        ("name".to_owned(), Value::from(state.engine.catalog.piece(id).name2.as_ref())),
        ("state".to_owned(), Value::from(if actual.is_checked(id) { "Checked" } else { "Unchecked" })),
    ].into_iter().collect())).collect();
    assert_eq!(Value::Array(visible_checks), expected["visible_checks"], "{label}: visible check states");
}

pub fn assert_legacy_error(actual: &CalcError, expected: &Value, label: &str) {
    match actual {
        CalcError::Legacy { category, .. } => assert_eq!(*category, expected["class"].as_str().unwrap(), "{label}: calculation error category"),
        error => panic!("{label}: expected legacy {} error, got {error:?}", expected["class"]),
    }
}

fn assert_gathered_abilities(abilities: &Abilities, expected: &Value, label: &str) {
    for (name, value) in expected["special_toggles"].as_object().unwrap() {
        match name.as_str() {
            "Enhancing Skill" => assert_number(abilities.enhancing_skill, value, &format!("{label}.skill")),
            "Aftermath" => assert_number(abilities.aftermath(), value, &format!("{label}.aftermath")),
            "Storm spell" => assert_eq!(abilities.storm_name.as_deref(), value.as_str(), "{label}: cached storm"),
            "Enemy Resist Rank" => assert_eq!(abilities.resist_rank_name.as_ref(), value.as_str().unwrap(), "{label}: resist rank"),
            name => {
                let ability = Ability::from_name(name).unwrap_or_else(|| panic!("{label}: unknown frozen ability {name}"));
                assert_eq!(abilities.enabled(ability), value.as_bool().unwrap(), "{label}: ability {name}");
            }
        }
    }
}

fn assert_gathered_inputs(state: &UiState, expected: &Value, label: &str) {
    assert_eq!(state.main_job().unwrap().name(), expected["main_job"].as_str().unwrap(), "{label}: main job");
    assert_eq!(state.sub_job().map_or("None", Job::name), expected["sub_job"].as_str().unwrap(), "{label}: sub job");
    assert_number(state.integer("defaults_ml_combobox").unwrap(), &expected["master_level"], &format!("{label}.master level"));
    assert_eq!(state.text("defaults_ws_combobox"), expected["ws_name"].as_str().unwrap(), "{label}: selected WS");
    assert_eq!(state.text("defaults_spell_combobox"), expected["spell_name"].as_str().unwrap(), "{label}: selected spell");
    assert_number(state.integer("defaults_tp_entry").unwrap(), &expected["tp_value"], &format!("{label}.TP"));
    assert_gathered_abilities(&state.abilities().unwrap(), expected, label);
    assert_stats(&state.enemy_input().unwrap().stats, &expected["enemy_stats"], &format!("{label}.gathered enemy"));
}

pub fn assert_snapshot(state: &UiState, expected: &Value, label: &str) {
    assert_eq!(state.controls.len(), expected["controls"].as_object().unwrap().len(), "{label}: registered control domain");
    for spec in INPUT_REGISTRY {
        let actual = &state.controls[spec.key];
        let row = &expected["controls"][spec.key];
        let kind = match actual.kind { InputKind::Checkbox => "checkbox", InputKind::Combo => "combo", InputKind::Entry => "entry", InputKind::Label => "label" };
        assert_eq!(kind, row["type"].as_str().unwrap(), "{label}.{}: control kind", spec.key);
        assert_eq!(actual.value, row["state"], "{label}.{}", spec.key);
        assert_eq!(actual.enabled, row["enabled"].as_bool().unwrap(), "{label}.{}.enabled", spec.key);
        assert_eq!(actual.hidden, row["explicit_hidden"].as_bool().unwrap(), "{label}.{}.hidden", spec.key);
    }
    for spec in ABILITY_SPECS {
        let actual = &state.controls[spec.key];
        let row = &expected["availability"][spec.name];
        assert_eq!(actual.value, row["checked"], "{label}: {} availability state", spec.name);
        assert_eq!(actual.hidden, row["explicit_hidden"].as_bool().unwrap(), "{label}: {} availability visibility", spec.name);
        assert_eq!(i64::from(spec.level), row["requirements"]["level requirement"].as_i64().unwrap(), "{label}: {} availability level", spec.name);
        assert_eq!(to_value(spec.jobs.iter().map(|job| job.name()).collect::<Vec<_>>()).unwrap(), row["requirements"]["job requirement"], "{label}: {} availability jobs", spec.name);
    }
    for (index, key) in ["Dia", "Haste", "Boost", "Storm"].iter().enumerate() {
        assert_eq!(state.whm_selections[index], expected["cached_whm_selections"][key], "{label}.cached {key}");
    }
    assert_piece_state(state, state.cached_food.expect("source retains a cached food piece"), &expected["cached_food"], &format!("{label}.cached food"));
    assert_eq!(state.sub_job_level as i64, expected["ui_sub_job_level"].as_i64().unwrap(), "{label}.cached UI sublevel");
    assert_eq!(state.main_weapon_skill.name(), expected["ui_main_weapon_skill"].as_str().unwrap(), "{label}.main skill cache");
    assert_eq!(state.ranged_weapon_skill.name(), expected["ui_ranged_weapon_skill"].as_str().unwrap(), "{label}.ranged skill cache");
    assert_eq!(state.active_tab as u64, expected["active_tab"].as_u64().unwrap(), "{label}.tab");
    assert_eq!(["Quicklook", "Optimize", "Simulate", "Player Stats"][state.active_tab], expected["active_tab_title"].as_str().unwrap(), "{label}.tab title");
    assert_eq!(state.damage_limit, expected["menu_flags"]["99999"].as_bool().unwrap(), "{label}: damage cap setting");
    assert_eq!(state.verbose_swaps, expected["menu_flags"]["Verbose Swaps"].as_bool().unwrap(), "{label}: verbose setting");
    for source in GearSource::ALL {
        for slot in Slot::ALL {
            let row = &expected["equipped_sets"][source.name()][slot.name()];
            assert_piece_state(state, state.gear.equipped[source.index()][slot.index()], &row["piece"], &format!("{label}.{}.{}", source.name(), slot.name()));
            let radio = &state.gear.radios[source.index()][slot.index()];
            assert_eq!(radio.selected.map(|id| state.engine.catalog.piece(id).name2.as_ref()).unwrap_or(""), row["radio_selection"].as_str().unwrap(), "{label}.{}.{}.radio", source.name(), slot.name());
            let visible = radio.visible_selection().map(|id| Value::from(state.engine.catalog.piece(id).name2.as_ref())).unwrap_or(Value::Null);
            assert_eq!(visible, row["visible_radio_selection"], "{label}.{}.{}.visible radio", source.name(), slot.name());
            if let Some(models) = expected.get("radio_models") {
                assert_radio_model(state, radio, &models[source.name()][slot.name()], &format!("{label}.{}.{}.model", source.name(), slot.name()));
            }
        }
    }
    if let Some(gathered) = expected.get("gather_player_inputs") {
        // Gathering remains observable even if a later buff parse rejects the action.
        assert_gathered_inputs(state, gathered, label);
    } else {
        let error = state.calculation_inputs(GearSource::Quicklook).unwrap_err();
        assert_legacy_error(&error, &expected["gather_player_inputs_error"], &format!("{label}.gather"));
    }
    let effects = aggregate_buffs(&state.buff_inputs(), &state.engine.catalog);
    if let Some(expected_effects) = expected.get("aggregate_buffs") {
        let effects = effects.unwrap_or_else(|error| panic!("{label}: aggregate buffs: {error}"));
        assert_eq!(effects.buckets.len(), expected_effects["active_buffs"].as_object().unwrap().len(), "{label}: buff bucket domain");
        for bucket in &effects.buckets {
            assert_stats(&bucket.stats, &expected_effects["active_buffs"][bucket.name.as_ref()], &format!("{label}.buffs.{}", bucket.name));
        }
        assert_stats(&effects.enemy_debuffs, &expected_effects["combined_debuffs"], &format!("{label}.debuffs"));
    } else {
        assert_legacy_error(&effects.unwrap_err(), &expected["aggregate_buffs_error"], &format!("{label}.buffs"));
    }
    if let Some(enemy) = expected.get("enemy_input") {
        assert_stats(&state.enemy_input().unwrap().stats, enemy, &format!("{label}.raw enemy"));
    }
    if let Some(enemy) = expected.get("normalized_enemy") {
        let actual = state.calculation_inputs(GearSource::Quicklook).unwrap_or_else(|error| panic!("{label}: calculation inputs: {error}"));
        assert_eq!(actual.ws_type.name(), expected["gather_player_inputs"]["ws_type"].as_str().unwrap(), "{label}: WS dispatch");
        assert_stats(&actual.enemy.stats, enemy, &format!("{label}.enemy"));
        assert_stats(&actual.player.effects.enemy_debuffs, &expected["enemy_debuff_input"], &format!("{label}.enemy debuff input"));
        // The controller adds the menu cap flag after raw gather, so this
        // frozen endpoint is distinct from the visible widget snapshot.
        assert_gathered_abilities(&actual.player.abilities, &expected["actual_enemy_preparation_inputs"]["gathered"], &format!("{label}.prepared player"));
    }
    assert_eq!(to_value(&state.controls["defaults_ws_combobox"].choices).unwrap(), expected["ws_choices"], "{label}: literal WS choices");
    assert_eq!(to_value(&state.controls["defaults_spell_combobox"].choices).unwrap(), expected["spell_choices"], "{label}: literal spell choices");
    if let Some(models) = expected.get("checkbox_models") {
        for slot in Slot::ALL {
            assert_checkbox_model(state, &state.gear.candidates[slot.index()], &models[slot.name()], &format!("{label}.{}.checkbox model", slot.name()));
        }
    }
}

pub fn assert_last_player(state: &UiState, expected: &Value, phase: &str, label: &str) {
    // Repeated source refreshes in one phase have identical captured final
    // sheets. Compare the consumer-visible endpoint once, as state.rs does.
    if let Some(build) = expected["player_builds"].as_array().unwrap().iter().rev().find(|build| build["phase"] == phase) {
        let player = state.last_player.as_ref().unwrap_or_else(|| panic!("{label}: {phase}: expected refreshed player"));
        support::assert_metadata(player, &build["final"], label);
        support::assert_sheet(&player.stats, &player.wsc, player.wyvern_bonus_attack, &build["final"], label);
        for slot in Slot::ALL {
            assert_piece_state(state, player.gearset[slot.index()], &build["final"]["gearset"][slot.name()], &format!("{label}.{phase}.player.{}", slot.name()));
        }
    }
}
