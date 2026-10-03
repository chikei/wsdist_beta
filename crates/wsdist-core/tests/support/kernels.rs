use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wsdist_core::math::*;
use wsdist_core::model::{CalcError, RandomSource, RandomStream};
use wsdist_core::rng::RandomSources;

use crate::kernel_support::{self, StrictTape, assert_value, numeric};

#[derive(Clone)]
struct SeededTape {
    random: RandomSources,
    tape: StrictTape,
}

impl RandomSource for SeededTape {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        let expected = self.tape.uniform(stream, low, high);
        let actual = self.random.uniform(stream, low, high);
        assert_eq!(actual.to_bits(), expected.to_bits(), "seeded helper draw {stream:?} [{low},{high}]");
        actual
    }
    fn bounded_numpy(&mut self, upper: usize) -> usize { panic!("kernel unexpectedly chooses from {upper}") }
    fn reseed_numpy(&mut self, seed: u32) { panic!("kernel unexpectedly reseeds with {seed}") }
}

fn temper(mut word: u32) -> u32 {
    word ^= word >> 11;
    word ^= (word << 7) & 0x9d2c_5680;
    word ^= (word << 15) & 0xefc6_0000;
    word ^ (word >> 18)
}

fn assert_state(random: &RandomSources, value: &Value, label: &str) {
    for (name, stream) in [("NumpyHost", RandomStream::NumpyHost), ("NumbaPython", RandomStream::NumbaPython)] {
        let state = &value[name];
        let words = state["words"].as_array().expect("captured full MT state");
        assert_eq!(words.len(), 624, "{label}.{name}: MT width");
        let position = state["position"].as_u64().unwrap() as usize;
        assert!(position <= 624, "{label}.{name}: cursor");
        let mut hash = Sha256::new();
        let tempered = std::array::from_fn::<_, 624, _>(|index| {
            let word = words[index].as_u64().unwrap() as u32;
            hash.update(word.to_le_bytes());
            temper(word)
        });
        if let Some(expected) = state.get("word_sha256") {
            assert_eq!(format!("{:x}", hash.finalize()), expected.as_str().unwrap(), "{label}.{name}: word hash");
        }
        let mut actual = random.clone();
        for &expected in &tempered[position..] {
            assert_eq!(actual.next_u32(stream), expected, "{label}.{name}: current-block continuation");
        }
        let mut next = rand_mt::Mt::recover(tempered).expect("frozen nonzero MT state");
        for _ in 0..32 {
            assert_eq!(actual.next_u32(stream), next.next_u32(), "{label}.{name}: next-twist continuation");
        }
    }
}

pub fn replay(case: &Value) -> (usize, usize, usize) {
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
            (rows.len(), 0, 0)
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
            (count, 0, 0)
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
            (rows.len(), 0, 0)
        }
        "kernel_group" | "kernel_tapes" => {
            let rows = case["expected"]["value"]["rows"].as_array().unwrap();
            let mut seeded = if case["operation"] == "kernel_group" {
                Some(SeededTape { random: RandomSources::from_seeds(case["input"]["host_seed"].as_u64().unwrap() as u32,
                    case["input"]["numba_seed"].as_u64().unwrap() as u32), tape: StrictTape::new(Vec::new()) })
            } else { None };
            if let Some(seeded) = &seeded { assert_state(&seeded.random, &case["expected"]["value"]["rng_initial"], label); }
            let mut taped = 0;
            let mut seeded_rows = 0;
            for row in rows {
                if let Some(seeded) = &mut seeded {
                    seeded.tape = StrictTape::for_row(case, row);
                    let result = kernel_support::execute(case, row, seeded);
                    seeded.tape.assert_exhausted();
                    let path = format!("{label}.{}", row["label"].as_str().unwrap());
                    match (result, row["outcome"].as_str().unwrap()) {
                        (Ok(actual), "success") => {
                            let function = case["input"]["function"].as_str().unwrap();
                            let exact = !matches!(function, "get_delay_timing" | "get_avg_pdif_melee" | "get_avg_pdif_ranged" | "get_pdif_melee" | "get_pdif_ranged" | "get_ma_rate3" | "get_resist_state_average");
                            assert_value(&actual, &row["result"], exact, &path);
                            if let Some(pair) = row.get("seeded_adapter_pair") {
                                assert_value(&actual, &pair["compiled_or_live_result"], exact, &path);
                                assert_value(&actual, &pair["adapter_result"], exact, &path);
                            }
                        }
                        (Err(error), "error") => {
                            let CalcError::Legacy { category, message, .. } = error else { panic!("{path}: wrong error {error}") };
                            assert_eq!(category, row["exception"]["class"].as_str().unwrap(), "{path}: error category");
                            assert_eq!(message, row["exception"]["message"].as_str().unwrap(), "{path}: error message");
                        }
                        (actual, outcome) => panic!("{path}: expected {outcome}, got {actual:?}"),
                    }
                    if seeded.tape.draw_count() > 0 { taped += 1; seeded_rows += 1; }
                } else {
                    kernel_support::replay_row(case, row);
                    taped += usize::from(StrictTape::for_row(case, row).draw_count() > 0);
                }
            }
            if let Some(seeded) = &seeded { assert_state(&seeded.random, &case["expected"]["value"]["rng_final"], label); }
            (rows.len(), taped, seeded_rows)
        }
        operation => panic!("{label}: unsupported kernel protocol {operation}"),
    }
}
