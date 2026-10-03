use std::path::{Path, PathBuf};
use std::sync::Arc;
use serde_json::Value;
use wsdist_core::model::{Abilities, Ability, BuffInputs, EnemyInput, RollInput};
use wsdist_core::Stats;

#[path = "../support/fixture_json.rs"]
mod fixture_json;

#[path = "../support/observation.rs"]
pub mod observation;

pub fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
pub fn ui_fixtures() -> PathBuf { root().join("crates/wsdist-app/tests/fixtures/legacy") }
pub fn read_json(path: &Path) -> Value {
    fixture_json::read::<Value>(path, None).unwrap_or_else(|error| panic!("{error}"))
}

pub fn controls(state: &Value) -> BuffInputs {
    let values = &state["controls"];
    let text = |key: &str| Arc::<str>::from(values[key]["state"].as_str().unwrap_or_else(|| panic!("missing text control {key}")));
    let flag = |key: &str| values[key]["state"].as_bool().unwrap_or_else(|| panic!("missing checkbox {key}"));
    let mut miscellaneous = Abilities::default();
    for &ability in Ability::ALL {
        let key = format!("defaults_{}_checkbox",ability.name());
        if let Some(value) = values[&key]["state"].as_bool() { miscellaneous.set(ability,value); }
    }
    BuffInputs {
        brd_enabled:flag("defaults_brd_checkbox"),
        songs:std::array::from_fn(|index| text(&format!("defaults_Song{}_combobox",index+1))),
        song_bonus:text("defaults_song_bonus_combobox"),soul_voice:flag("defaults_soul_voice_checkbox"),marcato:flag("defaults_marcato_checkbox"),
        cor_enabled:flag("defaults_cor_checkbox"),
        rolls:std::array::from_fn(|index| RollInput{name:text(&format!("defaults_Roll{}_name_combobox",index+1)),potency:text(&format!("defaults_Roll{}_potency_combobox",index+1))}),
        roll_bonus:text("defaults_roll_bonus_combobox"),crooked_cards:flag("defaults_crooked_cards_checkbox"),cor_job_bonus:flag("defaults_cor_job_bonus_checkbox"),light_shot:flag("defaults_light_shot_checkbox"),
        geo_enabled:flag("defaults_geo_checkbox"),bubbles:[text("defaults_Indi-_combobox"),text("defaults_Geo-_combobox"),text("defaults_Entrust-_combobox")],
        bubble_bonus:text("defaults_bubble_bonus_checkbox"),bubble_potency:text("defaults_bubble_potency_entry"),bolster:flag("defaults_bolster_checkbox"),blaze_of_glory:flag("defaults_bog_checkbox"),
        whm_enabled:flag("defaults_whm_checkbox"),whm_selections: ["Dia","Haste","Boost","Storm"].map(|key| Arc::from(state["cached_whm_selections"][key].as_str().unwrap())),
        shell_v:flag("defaults_shell5_checkbox"),food_name:text("defaults_food_combobox"),miscellaneous,
    }
}

pub fn enemy(state: &Value) -> EnemyInput {
    EnemyInput { name:Arc::from("captured GUI enemy"),location:Arc::from("captured UI"),
        stats:serde_json::from_value::<Stats>(state["enemy_input"].clone()).unwrap() }
}

pub fn states<'a>(value: &'a Value, output: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object) => {
            if object.contains_key("controls") && object.contains_key("aggregate_buffs") && object.contains_key("cached_whm_selections") {
                output.push(value);
            } else { for child in object.values() { states(child,output); } }
        }
        Value::Array(values) => { for child in values { states(child,output); } }
        _ => {}
    }
}

pub fn assert_stats(actual: &Stats, expected: &Value, at: &str) {
    let expected = expected.as_object().unwrap_or_else(|| panic!("expected stat map at {at}: {expected}"));
    assert_eq!(actual.ordered().map(|(stat,_)|stat.name()).collect::<Vec<_>>(),expected.keys().map(String::as_str).collect::<Vec<_>>(),"{at}: key order/presence");
    for (stat, value) in actual.ordered() {
        let wanted = expected[stat.name()].as_f64().unwrap();
        observation::record(value, wanted, expected[stat.name()].is_i64() || expected[stat.name()].is_u64(), &format!("{at}.{}", stat.name()));
        if expected[stat.name()].is_i64() || expected[stat.name()].is_u64() {
            assert_eq!(value,wanted,"{at}.{}: integer stage",stat.name());
        } else {
            assert!(value.is_finite() && (value-wanted).abs() <= 1e-9 + 1e-12*value.abs().max(wanted.abs()),"{at}.{}: {value} != {wanted}",stat.name());
        }
        if value == 0.0 { assert_eq!(value.is_sign_negative(),wanted.is_sign_negative(),"{at}.{}: signed zero",stat.name()); }
    }
}
