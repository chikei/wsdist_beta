use serde_json::{Value, json};
use wsdist_core::math::*;

use crate::kernel_support::{self, StrictTape, assert_value, numeric};

pub fn replay(case: &Value) -> (usize, usize) {
    let label = case["id"].as_str().unwrap();
    match case["operation"].as_str().unwrap() {
        "rounding" => {
            let rows = case["expected"]["value"]["rows"].as_array().unwrap();
            for (index, row) in rows.iter().enumerate() {
                let input = numeric(&row["input"]);
                let python = python_round1(input);
                let numpy = numpy_round1(input);
                assert_value(&json!(python), &row["python_round"], true, &format!("{label}[{index}].python"));
                assert_value(&json!(numpy), &row["numpy_round"], true, &format!("{label}[{index}].numpy"));
                assert_eq!(python.is_sign_negative(), numeric(&row["python_sign"]) < 0.0, "{label}: Python signed zero");
                assert_eq!(numpy.is_sign_negative(), numeric(&row["numpy_sign"]) < 0.0, "{label}: NumPy signed zero");
            }
            (rows.len(), 0)
        }
        "kernel_tables" => {
            let mut count = 0;
            for table in case["expected"]["value"]["tables"].as_array().unwrap() {
                match table["name"].as_str().unwrap() {
                    "get_mv_blm.mv" => for (tier_name, elements) in table["value"].as_object().unwrap() {
                        for (element_name, windows) in elements.as_object().unwrap() {
                            for (window_name, coefficients) in windows.as_object().unwrap() {
                                let window: f64 = window_name.parse().unwrap();
                                let (m, v, actual_window) = get_mv_blm(BlmElement::from_name(element_name).unwrap(), BlmTier::from_name(tier_name).unwrap(), window).unwrap();
                                let path = format!("{label}.{tier_name}.{element_name}.{window_name}");
                                assert_value(&json!(m), &coefficients[1], true, &format!("{path}.M"));
                                assert_value(&json!(v), &coefficients[0], true, &format!("{path}.V"));
                                assert_eq!(actual_window, window, "{path}: coefficient window");
                                count += 1;
                            }
                        }
                    },
                    "get_weapon_bonus.weapon_skill_bonuses" => for (weapon, skills) in table["value"].as_object().unwrap() {
                        for (skill, amount) in skills.as_object().unwrap() {
                            let actual = get_weapon_bonus(WeaponBonusWeapon::from_name(weapon), WeaponBonusWeapon::from_name(""), WeaponBonusSkill::from_name(skill));
                            assert_value(&json!(actual), amount, true, &format!("{label}.{weapon}.{skill}"));
                            count += 1;
                        }
                    },
                    name => panic!("{label}: unknown kernel table {name}"),
                }
            }
            (count, 0)
        }
        "kernel_group" | "kernel_tapes" if case["input"]["module"] == "create_player" => {
            let rows = case["expected"]["value"]["rows"].as_array().unwrap();
            for row in rows {
                let path = format!("{label}.{}", row["label"].as_str().unwrap());
                let result = wsdist_core::model::get_skill_accuracy(numeric(&row["args"][0]));
                match (result, row["outcome"].as_str().unwrap()) {
                    (Ok(actual), "success") => assert_value(&json!(actual), &row["result"], true, &path),
                    (Err(error), "error") => crate::support::assert_error(&error, &row["exception"], &path),
                    (actual, expected) => panic!("{path}: expected {expected}, got {actual:?}"),
                }
            }
            (rows.len(), 0)
        }
        "kernel_group" | "kernel_tapes" => {
            let rows = case["expected"]["value"]["rows"].as_array().unwrap();
            let mut taped = 0;
            for row in rows {
                kernel_support::replay_row(case, row);
                taped += usize::from(StrictTape::for_row(case, row).draw_count() > 0);
            }
            (rows.len(), taped)
        }
        operation => panic!("{label}: unsupported kernel protocol {operation}"),
    }
}
