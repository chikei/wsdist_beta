#[path = "numerical_boundaries/support.rs"]
mod support;

use serde_json::Value;
use wsdist_core::math::*;
use wsdist_core::model::CalcError;

fn module(fixture: &Value, name: &str) -> bool {
    matches!(fixture["operation"].as_str(), Some("kernel_group" | "kernel_tapes"))
        && fixture["input"]["module"] == name
}

#[test]
fn tp_delay_truncation_and_physical_hit_caps_match_frozen_helpers() {
    support::replay_where(|fixture| module(fixture, "get_tp") || module(fixture, "get_hit_rate"));
}

#[test]
fn fractional_fstr_rank_and_ddex_piecewise_endpoints_match_frozen_helpers() {
    support::replay_where(|fixture| module(fixture, "get_fstr") || module(fixture, "get_dex_crit"));
}

#[test]
fn haste_caps_and_delay_floor_match_frozen_helpers() {
    support::replay_where(|fixture| module(fixture, "get_delay_timing"));
}

#[test]
fn analytical_pdif_keeps_legacy_midpoint_and_critical_correlation() {
    support::replay_where(|fixture| module(fixture, "get_pdif") && fixture["input"]["function"].as_str().unwrap().starts_with("get_avg_"));
}

#[test]
fn sampled_pdif_replays_recorded_compiled_draws_and_strict_endpoint_tapes() {
    support::replay_where(|fixture| module(fixture, "get_pdif") && !fixture["input"]["function"].as_str().unwrap().starts_with("get_avg_"));
}

#[test]
fn physical_damage_truncates_before_pdif_and_preserves_first_hit_bonuses() {
    support::replay_where(|fixture| module(fixture, "get_phys_damage"));
}

#[test]
fn multiattack_keeps_proc_priority_expected_hit_cap_and_float32_widening() {
    support::replay_where(|fixture| module(fixture, "get_ma_rate"));
}

#[test]
fn ninjutsu_and_elemental_coefficients_preserve_every_window_and_key_error() {
    support::replay_where(|fixture| module(fixture, "get_dint_m_v"));
}

#[test]
fn enspell_magic_accuracy_and_four_resist_states_match_frozen_helpers() {
    support::replay_where(|fixture| module(fixture, "nuking"));
}

#[test]
fn exact_weapon_spellings_and_both_weapon_contributions_match_frozen_helpers() {
    support::replay_where(|fixture| module(fixture, "weapon_bonus"));
}

#[test]
fn decimal_half_ties_use_distinct_python_and_numpy_algorithms() {
    for fixture in support::fixtures().iter().filter(|fixture| fixture["operation"] == "rounding") {
        for row in fixture["expected"]["value"]["rows"].as_array().unwrap() {
            let input = support::numeric(&row["input"]);
            let python = python_round1(input);
            let numpy = numpy_round1(input);
            assert_eq!(python.to_bits(), support::numeric(&row["python_round"]).to_bits(), "Python round({},1), {}", input, row["input_hex"]);
            assert_eq!(numpy.to_bits(), support::numeric(&row["numpy_round"]).to_bits(), "NumPy round({},1), {}", input, row["input_hex"]);
            assert_eq!(python.is_sign_negative(), support::numeric(&row["python_sign"]) < 0.0);
            assert_eq!(numpy.is_sign_negative(), support::numeric(&row["numpy_sign"]) < 0.0);
        }
    }
}

#[test]
fn complete_literal_coefficient_and_weapon_tables_have_the_captured_behavior() {
    let fixture = support::fixtures().iter().find(|fixture| fixture["operation"] == "kernel_tables").unwrap();
    for table in fixture["expected"]["value"]["tables"].as_array().unwrap() {
        match table["name"].as_str().unwrap() {
            "get_mv_blm.mv" => {
                for (tier_name, elements) in table["value"].as_object().unwrap() {
                    let tier = BlmTier::from_name(tier_name).unwrap();
                    for (element_name, windows) in elements.as_object().unwrap() {
                        let element = BlmElement::from_name(element_name).unwrap();
                        for (window_name, coefficients) in windows.as_object().unwrap() {
                            let window: f64 = window_name.parse().unwrap();
                            let (m, v, actual_window) = get_mv_blm(element, tier, window).unwrap();
                            assert_eq!(m.to_bits(), support::numeric(&coefficients[1]).to_bits(), "{tier_name}.{element_name}.{window_name}.M");
                            assert_eq!(v.to_bits(), support::numeric(&coefficients[0]).to_bits(), "{tier_name}.{element_name}.{window_name}.V");
                            assert_eq!(actual_window, window);
                        }
                    }
                }
            }
            "get_weapon_bonus.weapon_skill_bonuses" => {
                let empty = WeaponBonusWeapon::from_name("");
                for (weapon_name, skills) in table["value"].as_object().unwrap() {
                    let weapon = WeaponBonusWeapon::from_name(weapon_name);
                    for (skill_name, amount) in skills.as_object().unwrap() {
                        let skill = WeaponBonusSkill::from_name(skill_name);
                        assert_eq!(get_weapon_bonus(weapon, empty, skill).to_bits(), support::numeric(amount).to_bits(), "{weapon_name}.{skill_name}");
                    }
                }
            }
            name => panic!("unhandled frozen helper table {name}"),
        }
    }
}

#[test]
fn checked_integer_stages_reject_nonfinite_without_saturation_or_finite_clamps() {
    for (input, category) in [(f64::NAN, "ValueError"), (f64::INFINITY, "OverflowError"), (f64::NEG_INFINITY, "OverflowError")] {
        let error = python_int(input, "integer_boundary", "value").unwrap_err();
        match error {
            CalcError::Legacy { operation: "integer_boundary", field: "value", category: actual, .. } => assert_eq!(actual, category),
            other => panic!("wrong checked-int error {other:?}"),
        }
    }
    for (input, expected) in [(-0.0, 0.0_f64), (-0.9, 0.0), (-1.9, -1.0), (1.9, 1.0), (9_223_372_036_854_775_808.0, 9_223_372_036_854_775_808.0), (-9_223_372_036_854_777_856.0, -9_223_372_036_854_777_856.0), (f64::MAX, f64::MAX)] {
        let actual = python_int(input, "integer_boundary", "value").unwrap();
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
}

#[test]
fn nonfinite_outputs_are_not_replaced_with_success_shaped_zeroes() {
    assert!(get_fstr(90.0, f64::NAN, 100.0).is_nan());
    assert!(get_fstr2(90.0, f64::NAN, 100.0).is_nan());
    assert!(get_hit_rate(f64::NAN, 1000.0, 0.95).is_nan());
    assert!(get_avg_pdif_melee(f64::NAN, PdifSkill::from_name("Sword"), 0.0, 0.0, 1.0, 0.0).unwrap().is_nan());
    assert!(get_resist_state_average(f64::NAN).is_nan());
    assert!(python_round1(f64::NAN).is_nan());
    assert!(numpy_round1(f64::NAN).is_nan());
    for value in [f64::NEG_INFINITY, f64::INFINITY] {
        assert_eq!(python_round1(value).to_bits(), value.to_bits());
        assert_eq!(numpy_round1(value).to_bits(), value.to_bits());
    }
    // Comparisons, rather than a blanket NaN guard, also preserve these source
    // final-else outcomes. They are not invented corrections or zero fallbacks.
    assert_eq!(get_dex_crit(f64::NAN, 0.0), 0.15);
    assert_eq!(get_dstat_macc(f64::NAN, 0.0), 30.0);
}
