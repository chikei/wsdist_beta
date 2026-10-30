use serde_json::json;
use wsdist_core::model::{Abilities, Ability, ActionResult, LegacyF64, Storm, python_truth};
use wsdist_core::{Stat, Stats};
use wsdist_fixtures::{Number as FixtureNumber, Value as FixtureValue, from_value as from_fixture_value};

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
    let abilities: Abilities = serde_json::from_value(json!({"Footwork":"0", "Innin":0})).unwrap();
    assert!(abilities.enabled(Ability::Footwork));
    assert!(!abilities.enabled(Ability::Innin));
}

fn abilities_from_json_and_native(text: &str) -> [Abilities; 2] {
    let native: FixtureValue = serde_json::from_str(text).unwrap();
    [serde_json::from_str(text).unwrap(), from_fixture_value(&native).unwrap()]
}

fn assert_abilities_rejected(text: &str) {
    assert!(serde_json::from_str::<Abilities>(text).is_err(), "accepted JSON {text}");
    let native: FixtureValue = serde_json::from_str(text).unwrap();
    assert!(from_fixture_value::<Abilities>(&native).is_err(), "accepted native {text}");
}

#[test]
fn ability_truthiness_consumes_collections_and_handles_numeric_kinds() {
    for (value, enabled) in [
        ("null", false), ("false", false), ("true", true),
        ("0", false), ("0.0", false), ("-0.0", false),
        ("-9223372036854775808", true), ("18446744073709551615", true), ("5e-324", true),
        (r#""""#, false), (r#""0""#, true),
        ("[]", false), ("{}", false), ("[null,false,0]", true),
        (r#"{"nested":[null,{"other":false}]}"#, true),
        (r#"{"nonfinite":"NaN"}"#, true),
    ] {
        let text = format!(r#"{{"Footwork":{value},"Innin":true}}"#);
        for abilities in abilities_from_json_and_native(&text) {
            assert!(abilities.present(Ability::Footwork), "{text}");
            assert_eq!(abilities.enabled(Ability::Footwork), enabled, "{text}");
            assert!(abilities.enabled(Ability::Innin), "did not consume Footwork fully: {text}");
            assert!(!abilities.present(Ability::Berserk), "{text}");
        }
    }
}

#[test]
fn aftermath_retains_integer_float_boolean_and_nonfinite_sources() {
    for (value, expected, is_integer) in [
        ("0", 0.0, true), ("-9223372036854775808", i64::MIN as f64, true),
        ("18446744073709551615", u64::MAX as f64, true),
        ("1.0", 1.0, false), ("-0.0", -0.0, false),
        ("true", 1.0, true), ("false", 0.0, true),
        (r#"{"nonfinite":"NaN"}"#, f64::NAN, false),
        (r#"{"nonfinite":"+Infinity"}"#, f64::INFINITY, false),
        (r#"{"nonfinite":"-Infinity"}"#, f64::NEG_INFINITY, false),
    ] {
        let text = format!(r#"{{"Aftermath":{value}}}"#);
        let source: serde_json::Value = serde_json::from_str(value).unwrap();
        for abilities in abilities_from_json_and_native(&text) {
            assert_eq!(abilities.aftermath_is_integer(), is_integer, "{text}");
            if expected.is_nan() { assert!(abilities.aftermath().is_nan()); }
            else { assert_eq!(abilities.aftermath().to_bits(), expected.to_bits(), "{text}"); }
            assert_eq!(serde_json::to_value(&abilities).unwrap()["Aftermath"], source, "{text}");
        }
    }
    for value in ["null", r#""1""#, "[]", "{}", r#"{"nonfinite":"Infinity"}"#,
        r#"{"nonfinite":"NaN","other":0}"#] {
        assert_abilities_rejected(&format!(r#"{{"Aftermath":{value}}}"#));
    }
}

#[test]
fn borrowed_native_positive_signed_numbers_keep_integer_ability_semantics() {
    let mut native = wsdist_fixtures::to_value(json!({"Aftermath":0,"Footwork":0})).unwrap();
    for value in [0_i64, 2_i64] {
        native["Aftermath"] = FixtureValue::Number(FixtureNumber::I64(value));
        native["Footwork"] = FixtureValue::Number(FixtureNumber::I64(value));
        let abilities = from_fixture_value::<Abilities>(&native).unwrap();
        assert!(abilities.aftermath_is_integer());
        assert_eq!(abilities.aftermath(), value as f64);
        assert_eq!(abilities.enabled(Ability::Footwork), value != 0);
    }
    for field in ["Storm spell", "Enemy Resist Rank"] {
        let mut malformed = native.clone();
        malformed[field] = FixtureValue::Number(FixtureNumber::I64(0));
        assert!(from_fixture_value::<Abilities>(&malformed).is_err(), "accepted {malformed}");
    }
}

#[test]
fn storm_and_resist_rank_keep_text_semantics_instead_of_checkbox_truthiness() {
    for (value, name, storm, contains_ii) in [
        ("false", None, None, false), ("null", None, None, false),
        (r#""""#, Some(""), None, false),
        (r#""None""#, Some("None"), Some(Storm::None), false),
        (r#""Firestorm II""#, Some("Firestorm II"), Some(Storm::FireII), true),
        (r#""Unknown II storm""#, Some("Unknown II storm"), None, true),
    ] {
        let text = format!(r#"{{"Storm spell":{value}}}"#);
        for abilities in abilities_from_json_and_native(&text) {
            assert_eq!(abilities.storm_name.as_deref(), name, "{text}");
            assert_eq!(abilities.storm, storm, "{text}");
            assert_eq!(abilities.storm_contains_ii, contains_ii, "{text}");
        }
    }
    for (name, index) in [("150%", Some(0)), ("100%", Some(3)), ("5%", Some(14)), ("", None), ("unknown", None)] {
        let text = format!(r#"{{"Enemy Resist Rank":"{name}"}}"#);
        for abilities in abilities_from_json_and_native(&text) {
            assert_eq!(abilities.resist_rank_name.as_ref(), name);
            assert_eq!(abilities.resist_rank_index, index);
        }
    }
    for value in ["true", "-1", "0", "18446744073709551615", "1.0", "[]", "{}",
        r#"{"nonfinite":"NaN"}"#] {
        assert_abilities_rejected(&format!(r#"{{"Storm spell":{value}}}"#));
        assert_abilities_rejected(&format!(r#"{{"Enemy Resist Rank":{value}}}"#));
    }
    for value in ["false", "null"] {
        assert_abilities_rejected(&format!(r#"{{"Enemy Resist Rank":{value}}}"#));
    }
}

#[test]
fn duplicate_ability_fields_validate_the_last_value_like_json_maps() {
    for abilities in abilities_from_json_and_native(
        r#"{"Footwork":false,"Footwork":[0],"capture":{"ignored":[null,false,1.0,{"nested":""}]}}"#
    ) {
        assert!(abilities.enabled(Ability::Footwork));
    }
    for (field, invalid, valid) in [
        ("Aftermath", r#""invalid""#, "1"),
        ("Enhancing Skill", r#""invalid""#, "500.0"),
        ("Storm spell", "true", r#""Firestorm II""#),
        ("Enemy Resist Rank", "50", r#""50%""#),
    ] {
        let text = format!(r#"{{"{field}":{invalid},"{field}":{valid}}}"#);
        let expected: serde_json::Value = serde_json::from_str(valid).unwrap();
        for abilities in abilities_from_json_and_native(&text) {
            assert_eq!(serde_json::to_value(&abilities).unwrap()[field], expected, "{text}");
        }
        assert_abilities_rejected(&format!(r#"{{"{field}":{valid},"{field}":{invalid}}}"#));
    }
}

#[test]
fn ignored_and_truthiness_values_still_require_fully_valid_json() {
    let malformed = [
        br#"[0,"\uD800"]"#.to_vec(),
        br#"{"first":0,"ignored":"\uDC00"}"#.to_vec(),
        br#"{"first":0,"\uD800":0}"#.to_vec(),
        b"[0,\"bad\xff\"]".to_vec(),
        b"{\"first\":0,\"bad\xff\":0}".to_vec(),
        b"1e400".to_vec(),
        b"[0,1e400]".to_vec(),
        br#"{"first":0,"ignored":1e400}"#.to_vec(),
        format!("{}0{}", "[".repeat(160), "]".repeat(160)).into_bytes(),
    ];
    for field in ["ignored capture metadata", "Footwork"] {
        for value in &malformed {
            let mut text = format!(r#"{{"{field}":"#).into_bytes();
            text.extend_from_slice(value);
            text.extend_from_slice(br#","Innin":true}"#);
            assert!(serde_json::from_slice::<Abilities>(&text).is_err(), "accepted {text:?}");
        }
    }
}
