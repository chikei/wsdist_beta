use serde_json::json;
use wsdist_core::model::{Abilities, Ability, ActionResult, LegacyF64, python_truth};
use wsdist_core::{Stat, Stats};

#[test]
fn sparse_zero_is_present_and_dictionary_equality_ignores_order() {
    let absent = Stats::default();
    let mut first = Stats::default();
    first.add(Stat::Str, 0.0);
    assert_ne!(absent, first);
    first.set(Stat::Dex, 25.0);
    let mut second = Stats::default();
    second.set(Stat::Dex, 25.0);
    second.set(Stat::Str, 0.0);
    assert!(first.contains(Stat::Str));
    assert_eq!(first, second);
    assert_eq!(first.ordered().collect::<Vec<_>>(), [(Stat::Str, 0.0), (Stat::Dex, 25.0)]);
    assert_eq!(second.ordered().collect::<Vec<_>>(), [(Stat::Dex, 25.0), (Stat::Str, 0.0)]);
}

#[test]
fn removed_then_reinserted_key_moves_to_end_and_clear_has_no_stale_values() {
    let mut stats = Stats::default();
    stats.set(Stat::Str, 99.0);
    stats.set(Stat::Dex, 20.0);
    assert_eq!(stats.remove(Stat::Str), Some(99.0));
    stats.add(Stat::Str, 1.0);
    assert_eq!(stats.ordered().collect::<Vec<_>>(), [(Stat::Dex, 20.0), (Stat::Str, 1.0)]);
    stats.clear();
    assert!(!stats.contains(Stat::Str));
    assert_eq!(stats.get_or(Stat::Str, -7.0), -7.0);
    stats.add(Stat::Str, 3.0);
    assert_eq!(stats.ordered().collect::<Vec<_>>(), [(Stat::Str, 3.0)]);
}

#[test]
fn legacy_spellings_remain_distinct_and_internal_stats_are_not_gear_stats() {
    let proper = Stat::from_name("Magic Damage").unwrap();
    let legacy = Stat::from_name("Magic damage").unwrap();
    assert_ne!(proper, legacy);
    let mut stats = Stats::default();
    stats.set(proper, 50.0);
    stats.set(legacy, 9.0);
    assert_eq!(stats.get(proper), 50.0);
    assert_eq!(stats.get(legacy), 9.0);
    assert!(proper.gear_allowed());
    assert!(!legacy.gear_allowed());
    assert!(!Stat::Attack1.gear_allowed());
}

#[test]
fn tagged_numbers_keep_nonfinite_classes_and_negative_zero() {
    for (text, expected) in [
        (r#"{"nonfinite":"NaN"}"#, f64::NAN),
        (r#"{"nonfinite":"+Infinity"}"#, f64::INFINITY),
        (r#"{"nonfinite":"-Infinity"}"#, f64::NEG_INFINITY),
        ("-0.0", -0.0),
        ("2.55", 2.55),
    ] {
        let parsed: LegacyF64 = serde_json::from_str(text).unwrap();
        if expected.is_nan() { assert!(parsed.0.is_nan()); }
        else { assert_eq!(parsed.0.to_bits(), expected.to_bits()); }
        let roundtrip: LegacyF64 = serde_json::from_str(&serde_json::to_string(&parsed).unwrap()).unwrap();
        if expected.is_nan() { assert!(roundtrip.0.is_nan()); }
        else { assert_eq!(roundtrip.0.to_bits(), expected.to_bits()); }
    }
    assert!(serde_json::from_str::<LegacyF64>(r#"{"nonfinite":"Infinity"}"#).is_err());
    assert!(serde_json::from_str::<LegacyF64>(r#"{"nonfinite":"NaN","other":0}"#).is_err());
}

#[test]
fn stat_and_action_json_never_turn_nonfinite_outputs_into_null() {
    let mut stats = Stats::default();
    stats.set(Stat::Str, f64::NAN);
    stats.set(Stat::Dex, -0.0);
    let parsed: Stats = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
    assert!(parsed.get(Stat::Str).is_nan());
    assert_eq!(parsed.get(Stat::Dex).to_bits(), (-0.0_f64).to_bits());
    let result = ActionResult { damage: f64::NAN, tp_return: -0.0, seconds_per_round: Some(f64::INFINITY),
        time_to_ws: None, metric: f64::NEG_INFINITY, invert: 1, physical_damage: 1.0, magical_damage: 2.0 };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["damage"], json!({"nonfinite":"NaN"}));
    assert_eq!(value["seconds_per_round"], json!({"nonfinite":"+Infinity"}));
    assert_eq!(value["metric"], json!({"nonfinite":"-Infinity"}));
    assert!(value["time_to_ws"].is_null());
    assert_eq!(value["tp_return"].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn legacy_checkbox_truthiness_does_not_parse_string_zero_as_false() {
    let cases = [(json!("0"), true), (json!(0), false), (json!(""), false), (json!(false), false),
        (json!(null), false), (json!([]), false), (json!({}), false), (json!([0]), true), (json!({"key":0}), true)];
    for (value, expected) in cases { assert_eq!(python_truth(&value), expected, "{value}"); }
    let abilities = Abilities::from_legacy(json!({"Footwork":"0", "Innin":0}).as_object().unwrap()).unwrap();
    assert!(abilities.enabled(Ability::Footwork));
    assert!(!abilities.enabled(Ability::Innin));
}
