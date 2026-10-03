mod ws_support;

use std::sync::Arc;

use serde_json::Value;
use wsdist_core::catalog::Catalog;
use wsdist_core::model::{CalcError, EvaluationContext, WscContribution};
use wsdist_core::stats::Stat;
use wsdist_core::weaponskills::{CritScaling, DStatFormula, TpScaling, WS_TABLE, WsHook, interpolate_tp, resolve_name, weaponskill_info};

use ws_support::{NoSetupDraws, assert_number, boundary_enemy, boundary_player, document, evaluate_case, fixture_root, helper_event, index, number, resource_root, verify_case};

fn exact_number(label: &str, actual: f64, expected: &Value) {
    assert_eq!(actual.to_bits(), number(expected).to_bits(), "{label}: immutable static value");
}

fn exact_points(label: &str, actual: [f64; 3], expected: &Value) {
    let expected = expected.as_array().expect("three TP anchors");
    assert_eq!(expected.len(), 3, "{label}: anchor count");
    for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
        exact_number(&format!("{label}[{index}]"), actual, expected);
    }
}

fn verify_hook(label: &str, actual: WsHook, expected: &Value) {
    assert_eq!(actual.source_name(), expected["name"].as_str().unwrap(), "{label}: ordered hook identity");
    let parameters = &expected["parameters"];
    match actual {
        WsHook::Accuracy { points, both_hands } => {
            exact_points(&format!("{label}.acc_boost"), points, &parameters["acc_boost"]);
            assert_eq!(both_hands, parameters["both_hands"].as_bool().unwrap(), "{label}: both_hands");
        }
        WsHook::RangedAccuracy { points } => exact_points(&format!("{label}.acc_boost"), points, &parameters["acc_boost"]),
        WsHook::Attack { modifier, both_hands } => {
            exact_number(&format!("{label}.modifier"), modifier, &parameters["modifier"]);
            assert_eq!(both_hands, parameters["both_hands"].as_bool().unwrap(), "{label}: both_hands");
        }
        WsHook::AttackUnguarded { modifier } | WsHook::RangedAttack { modifier } => exact_number(&format!("{label}.modifier"), modifier, &parameters["modifier"]),
        WsHook::InterpolatedAttack { points, both_hands } => {
            exact_points(&format!("{label}.atk_boost"), points, &parameters["atk_boost"]);
            assert_eq!(both_hands, parameters["both_hands"].as_bool().unwrap(), "{label}: both_hands");
        }
        WsHook::KickAttack => assert_eq!(parameters["_recalc"]["qualname"], "kick_atk_hook.<locals>._recalc", "{label}: kick recalculation"),
        WsHook::DefenseDown { points } => exact_points(&format!("{label}.scaling"), points, &parameters["scaling"]),
    }
}

// Secondary static-coverage check. The full corpus below separately executes
// every formula and hook against independently captured numerical outputs.
#[test]
fn ordered_static_specs_cover_the_frozen_source_table() {
    let root = fixture_root();
    let metadata_case = index(&root).cases.into_iter().find(|case| case.operation == "combat.metadata").expect("reference table case");
    let metadata = document(&root.join(metadata_case.path));
    let expected = metadata["expected"]["value"]["actual_reference_tables"]["ws_table"].as_object().expect("frozen WS table");
    assert_eq!(WS_TABLE.len(), 184);
    assert_eq!(WS_TABLE.len(), expected.len());
    for (position, (actual, (name, expected))) in WS_TABLE.iter().zip(expected).enumerate() {
        let label = format!("WS_TABLE[{position}].{name}");
        assert_eq!(actual.name, name, "{label}: insertion order/name");
        assert_eq!(resolve_name(name).unwrap().index(), position, "{label}: resolved identity");
        assert_eq!(actual.skill_type.name(), expected["skill"].as_str().unwrap(), "{label}: weapon skill");
        assert_eq!(format!("weaponskill_data.py:{}-{}", actual.source_line, actual.source_line), expected["source_anchor"].as_str().unwrap(), "{label}: row anchor");
        let fields = &expected["fields"];
        assert_eq!(actual.nhits as u64, fields["nhits"].as_u64().unwrap(), "{label}: hits");
        match actual.ftp {
            TpScaling::Fixed(value) => {
                exact_number(&format!("{label}.ftp"), value, &fields["ftp"]);
                assert!(fields.get("base_ftp").is_none(), "{label}: fixed/interpolated classification");
            }
            TpScaling::Interpolated(points) => {
                exact_points(&format!("{label}.base_ftp"), points, &fields["base_ftp"]);
                assert!(fields.get("ftp").is_none(), "{label}: fixed/interpolated classification");
            }
        }
        let terms = fields["wsc"]["parameters"]["pairs"].as_array().unwrap();
        assert_eq!(actual.wsc.terms().len(), terms.len(), "{label}: WSC term count");
        for (position, ((coefficient, stat), expected)) in actual.wsc.terms().iter().zip(terms).enumerate() {
            exact_number(&format!("{label}.wsc[{position}].coefficient"), *coefficient, &expected[0]);
            let attribute = match stat {
                Stat::Str => "player_str", Stat::Dex => "player_dex", Stat::Vit => "player_vit",
                Stat::Agi => "player_agi", Stat::Int => "player_int", Stat::Mnd => "player_mnd", Stat::Chr => "player_chr",
                stat => panic!("{label}: non-primary WSC stat {stat:?}"),
            };
            assert_eq!(attribute, expected[1].as_str().unwrap(), "{label}: ordered WSC stat {position}");
        }
        assert_eq!(actual.ftp_rep, fields["ftp_rep"].as_bool().unwrap_or(false), "{label}: replication");
        assert_eq!(actual.magical, fields["magical"].as_bool().unwrap_or(false), "{label}: magical");
        assert_eq!(actual.hybrid, fields["hybrid"].as_bool().unwrap_or(false), "{label}: hybrid");
        assert_eq!(actual.element.name(), fields["element"].as_str().unwrap_or("None"), "{label}: element");
        match actual.dstat {
            DStatFormula::Zero => {
                if actual.magical { exact_number(&format!("{label}.dSTAT"), 0.0, &fields["dSTAT"]); }
                else { assert!(fields.get("dSTAT").is_none(), "{label}: physical dSTAT"); }
            }
            formula => assert_eq!(formula.source_name().unwrap(), fields["dSTAT"]["name"].as_str().unwrap(), "{label}: dSTAT formula"),
        }
        match actual.ftp_hybrid {
            Some(points) => exact_points(&format!("{label}.base_ftp_hybrid"), points, &fields["base_ftp_hybrid"]),
            None => assert!(fields.get("base_ftp_hybrid").is_none(), "{label}: hybrid anchors"),
        }
        match actual.crit {
            CritScaling::None => assert!(fields.get("crit").is_none() && fields.get("crit_rate").is_none(), "{label}: crit classification"),
            CritScaling::Fixed(rate) => exact_number(&format!("{label}.crit_rate"), rate, &fields["crit_rate"]),
            CritScaling::Melee(points) | CritScaling::Ranged(points) => {
                let source_name = if matches!(actual.crit, CritScaling::Melee(_)) { "crit_hook" } else { "ranged_crit_hook" };
                assert_eq!(source_name, fields["crit"]["name"].as_str().unwrap(), "{label}: crit hook");
                exact_points(&format!("{label}.crit_boost"), points, &fields["crit"]["parameters"]["crit_boost"]);
            }
        }
        let expected_hooks = fields["hooks"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        assert_eq!(actual.hooks.len(), expected_hooks.len(), "{label}: ordered hook count");
        for (position, (actual, expected)) in actual.hooks.iter().zip(expected_hooks).enumerate() {
            verify_hook(&format!("{label}.hooks[{position}]"), *actual, expected);
        }
    }
}

#[test]
fn all_captured_ws_setups_replay_from_resolved_numeric_boundaries() {
    let root = fixture_root();
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let ctx = EvaluationContext::new(catalog, NoSetupDraws);
    let mut coverage = [0_usize; WS_TABLE.len()];
    let mut successes = 0;
    let mut errors = 0;
    for row in index(&root).cases.into_iter().filter(|row| row.operation == "combat.ws") {
        let case = document(&root.join(&row.path));
        assert_eq!(case["id"].as_str().unwrap(), row.id, "{}: fixture identity", row.path);
        let actual = evaluate_case(&case, &ctx);
        verify_case(&case, &actual);
        match actual {
            Ok(_) => {
                let name = helper_event(&case)["arguments"]["ws_name"].as_str().unwrap();
                coverage[resolve_name(name).unwrap().index()] += 1;
                successes += 1;
            }
            Err(_) => errors += 1,
        }
    }
    assert_eq!(successes, 12_809, "successful direct helper corpus coverage");
    assert_eq!(errors, 3, "None/Atonement legacy helper error coverage");
    for (spec, cases) in WS_TABLE.iter().zip(coverage) {
        assert!(cases > 0, "{}: no production formula/hook replay", spec.name);
    }
}

fn require_legacy_error(error: CalcError, expected_field: &str, expected_category: &str) {
    match error {
        CalcError::Legacy { operation, field, category, .. } => {
            assert_eq!(operation, "weaponskill_info");
            assert_eq!(field, expected_field);
            assert_eq!(category, expected_category);
        }
        error => panic!("expected {expected_category} for {expected_field}, got {error}"),
    }
}

#[test]
fn required_sheet_values_and_zero_attack_denominators_keep_legacy_errors() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let ctx = EvaluationContext::new(catalog, NoSetupDraws);
    let case = document(&fixture_root().join("combat/008475.json")); // Naegling / Savage Blade
    let mut player = boundary_player(&case, &ctx.catalog);
    let mut enemy = boundary_enemy(&case);
    let skill = resolve_name("Savage Blade").unwrap();
    enemy.stats.remove(Stat::BaseDefense);
    enemy.stats.remove(Stat::Defense);
    require_legacy_error(weaponskill_info(skill, 2000.0, &player, &enemy, &[], false, &ctx).unwrap_err(), "Base Defense", "KeyError");
    enemy = boundary_enemy(&case);
    enemy.stats.remove(Stat::Vit); // Read by the context even though no hook uses it.
    require_legacy_error(weaponskill_info(skill, 2000.0, &player, &enemy, &[], false, &ctx).unwrap_err(), "VIT", "KeyError");
    enemy = boundary_enemy(&case);
    enemy.stats.remove(Stat::Chr); // enemy_chr actually reads MND, not CHR.
    let without_chr = weaponskill_info(skill, 2000.0, &player, &enemy, &[], false, &ctx).unwrap();
    let with_chr = weaponskill_info(skill, 2000.0, &player, &boundary_enemy(&case), &[], false, &ctx).unwrap();
    assert_eq!(without_chr, with_chr);
    player.stats.set(Stat::AttackPct, -1.0);
    require_legacy_error(weaponskill_info(skill, 2000.0, &player, &enemy, &[], false, &ctx).unwrap_err(), "Attack%", "ZeroDivisionError");
    player = boundary_player(&case, &ctx.catalog);
    player.stats.remove(Stat::Chr);
    require_legacy_error(weaponskill_info(skill, 2000.0, &player, &enemy,
        &[WscContribution { stat: Stat::Chr, percent: 3.0 }], false, &ctx).unwrap_err(), "CHR", "KeyError");
}

#[test]
fn tp_interpolation_retains_numpy_endpoints_and_nonfinite_classes() {
    let points = [1.0, 2.0, 4.0];
    for (tp, expected) in [
        (f64::NEG_INFINITY, 1.0), (999.0, 1.0), (1000.0, 1.0), (1500.0, 1.5),
        (2000.0, 2.0), (2500.0, 3.0), (3000.0, 4.0), (3001.0, 4.0), (f64::INFINITY, 4.0),
    ] {
        assert_number(&format!("interpolate_tp({tp:?})"), interpolate_tp(tp, points), &serde_json::json!(expected));
    }
    assert!(interpolate_tp(f64::NAN, points).is_nan());
    assert_eq!(interpolate_tp(1000.0, [-0.0, 1.0, 2.0]).to_bits(), (-0.0_f64).to_bits());
}
