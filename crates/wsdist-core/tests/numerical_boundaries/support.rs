use std::path::Path;
use std::sync::{Arc, LazyLock};

use wsdist_fixtures::Value;
use sha2::{Digest, Sha256};
use wsdist_core::math::*;
use wsdist_core::model::{CalcError, Job, LegacyF64, RandomSource, RandomStream};

#[path = "../support/observation.rs"]
pub mod observation;

pub fn fixtures() -> &'static [Value] {
    static FIXTURES: LazyLock<Vec<Value>> = LazyLock::new(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy");
        let index = wsdist_fixtures::read_json_value(&root.join("players.json")).unwrap();
        assert_eq!(index["schema_version"], 3, "player fixture storage schema");
        index["cases"].as_array().unwrap().iter().filter(|entry| {
            matches!(entry["operation"].as_str().unwrap(), "kernel_group" | "kernel_tapes" | "kernel_tables" | "rounding")
                && !entry["id"].as_str().unwrap().starts_with("kernel.create_player.")
        }).map(|entry| {
            let path = root.join(entry["path"].as_str().unwrap());
            let fixture = wsdist_fixtures::read_value(&path, Some(entry["content_xxh3_128"].as_str().unwrap()))
                .unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(fixture["id"], entry["id"]);
            fixture
        }).collect()
    });
    &FIXTURES
}

pub fn numeric(value: &Value) -> f64 {
    wsdist_fixtures::from_value::<LegacyF64>(value).unwrap().0
}

fn number(value: f64) -> Value {
    wsdist_fixtures::to_value(LegacyF64(value)).unwrap()
}

fn numbers(values: &[f64]) -> Value {
    Value::Array(values.iter().copied().map(number).collect())
}

fn boolean(args: &[Value], index: usize) -> bool { args[index].as_bool().unwrap() }
fn text(args: &[Value], index: usize) -> &str { args[index].as_str().unwrap() }
fn fixed<const N: usize>(value: &Value, dtype: Option<&str>) -> [f64; N] {
    let values = value.as_array().unwrap();
    assert_eq!(values.len(), N);
    std::array::from_fn(|index| {
        let value = numeric(&values[index]);
        if dtype == Some("float32") { f64::from(value as f32) } else { value }
    })
}

#[derive(Clone, Debug)]
pub struct Draw {
    pub stream: RandomStream,
    pub operation: Arc<str>,
    pub low: f64,
    pub high: f64,
    pub value: f64,
}

#[derive(Clone, Debug)]
pub struct StrictTape {
    draws: Arc<[Draw]>,
    cursor: usize,
}

impl StrictTape {
    pub fn new(draws: Vec<Draw>) -> Self { Self { draws: draws.into(), cursor: 0 } }
    pub fn assert_exhausted(&self) { assert_eq!(self.cursor, self.draws.len(), "unused random draws: {:?}", &self.draws[self.cursor..]); }
    pub fn draw_count(&self) -> usize { self.draws.len() }

    pub fn for_row(fixture: &Value, row: &Value) -> Self {
        if let Some(draws) = row.get("draws").or_else(|| row.get("seeded_adapter_pair").map(|pair| &pair["draws"])) {
            return Self::new(draws.as_array().unwrap().iter().map(|draw| Draw {
                stream: match draw["stream"].as_str().unwrap() {
                    "NumpyHost" => RandomStream::Primary,
                    "NumbaPython" => RandomStream::Pdif,
                    other => panic!("unsupported recorded stream {other}"),
                },
                operation: draw["operation"].as_str().unwrap().into(),
                low: numeric(&draw["low"]), high: numeric(&draw["high"]), value: numeric(&draw["value"]),
            }).collect());
        }
        let function = fixture["input"]["function"].as_str().unwrap();
        if row["outcome"] == "error" && matches!(function, "get_pdif_melee" | "get_pdif_ranged") {
            // The capture adapter has no result-pair on exceptions. Recover the
            // one consumed variate from frozen MT words in this same fixture,
            // not from a regenerated oracle or a fabricated random fallback.
            assert_eq!(row["exception"]["class"], "ZeroDivisionError");
            let before = &row["rng_before"]["NumbaPython"];
            let after = &row["rng_after"]["NumbaPython"];
            let state = &fixture["expected"]["value"]["rng_final"]["NumbaPython"];
            let words = state["words"].as_array().unwrap();
            let mut hasher = Sha256::new();
            for word in words { hasher.update((word.as_u64().unwrap() as u32).to_le_bytes()); }
            let hash = format!("{:x}", hasher.finalize());
            assert_eq!(hash, before["word_sha256"].as_str().unwrap());
            assert_eq!(hash, after["word_sha256"].as_str().unwrap());
            let position = before["position"].as_u64().unwrap() as usize;
            assert!(position + 2 <= words.len());
            assert_eq!(after["position"].as_u64().unwrap() as usize, position + 2);
            let high27 = temper(words[position].as_u64().unwrap() as u32) >> 5;
            let low26 = temper(words[position + 1].as_u64().unwrap() as u32) >> 6;
            let value = (f64::from(high27) * 67_108_864.0 + f64::from(low26)) / 9_007_199_254_740_992.0;
            return Self::new(vec![Draw { stream: RandomStream::Pdif, operation: "uniform".into(), low: 0.0, high: 1.0, value }]);
        }
        Self::new(Vec::new())
    }
}

// Temper only the two already-frozen words needed by an exception row. This
// is neither an MT implementation nor a production random source.
fn temper(mut word: u32) -> u32 {
    word ^= word >> 11;
    word ^= (word << 7) & 0x9d2c_5680;
    word ^= (word << 15) & 0xefc6_0000;
    word ^ (word >> 18)
}

impl RandomSource for StrictTape {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        let draw = self.draws.get(self.cursor).unwrap_or_else(|| panic!("extra uniform draw #{}, {stream:?} [{low},{high}]", self.cursor));
        assert_eq!(draw.operation.as_ref(), "uniform", "random operation order at {}", self.cursor);
        assert_eq!(stream, draw.stream, "random stream at {}", self.cursor);
        assert_eq!(low.to_bits(), draw.low.to_bits(), "uniform low at {}", self.cursor);
        assert_eq!(high.to_bits(), draw.high.to_bits(), "uniform high at {}", self.cursor);
        self.cursor += 1;
        draw.value
    }
    fn bounded(&mut self, upper_exclusive: usize) -> usize {
        panic!("unexpected bounded({upper_exclusive}) at random draw {}", self.cursor)
    }
    fn reseed(&mut self, seed: u64) {
        panic!("unexpected reseed({seed}) at random draw {}", self.cursor)
    }
}

pub fn execute(fixture: &Value, row: &Value, tape: &mut impl RandomSource) -> Result<Value, CalcError> {
    let args = row["args"].as_array().unwrap();
    let arg = |index: usize| numeric(&args[index]);
    assert!(row["kwargs"].as_object().unwrap().is_empty(), "new keyword protocol requires explicit support");
    let value = match fixture["input"]["function"].as_str().unwrap() {
        "get_tp" => number(get_tp(arg(0), arg(1), arg(2), boolean(args, 3))?),
        "get_hit_rate" => number(get_hit_rate(arg(0), arg(1), arg(2))),
        "get_fstr" => number(get_fstr(arg(0), arg(1), arg(2))),
        "get_fstr2" => number(get_fstr2(arg(0), arg(1), arg(2))),
        "get_dex_crit" => number(get_dex_crit(arg(0), arg(1))),
        "get_delay_timing" => number(get_delay_timing(arg(0), arg(1), arg(2), arg(3), arg(4), arg(5), arg(6))),
        "get_pdif_melee" | "get_pdif_ranged" => {
            let (pdif, crit) = if fixture["input"]["function"] == "get_pdif_melee" {
                get_pdif_melee(arg(0), PdifSkill::from_name(text(args, 1)), arg(2), arg(3), arg(4), arg(5), tape)?
            } else {
                get_pdif_ranged(arg(0), PdifSkill::from_name(text(args, 1)), arg(2), arg(3), arg(4), arg(5), tape)?
            };
            Value::Array(vec![number(pdif), Value::Bool(crit)])
        }
        "get_avg_pdif_melee" => number(get_avg_pdif_melee(arg(0), PdifSkill::from_name(text(args, 1)), arg(2), arg(3), arg(4), arg(5))?),
        "get_avg_pdif_ranged" => number(get_avg_pdif_ranged(arg(0), PdifSkill::from_name(text(args, 1)), arg(2), arg(3), arg(4), arg(5))?),
        "get_phys_damage" => number(get_phys_damage(arg(0), arg(1), arg(2), arg(3), arg(4), boolean(args, 5), arg(6), arg(7), arg(8), arg(9), arg(10), arg(11), arg(12), arg(13), arg(14), arg(15))?),
        "get_avg_phys_damage" => number(get_avg_phys_damage(arg(0), arg(1), arg(2), arg(3), arg(4), arg(5), arg(6), arg(7), arg(8), arg(9), arg(10), arg(11), arg(12), arg(13), arg(14))?),
        "get_ma_rate3" => {
            let dtype = |index: &str| row["array_dtypes"][index].as_str();
            let oa = fixed(&args[5], dtype("5"));
            let fua = fixed(&args[6], dtype("6"));
            let hitrates = [fixed(&args[8][0], dtype("8")), fixed(&args[8][1], dtype("8"))];
            let hits = get_ma_rate3(Job::parse_player_name(text(args, 0)).unwrap(), arg(1), arg(2), arg(3), arg(4), &oa, &fua, boolean(args, 7), &hitrates, arg(9), arg(10), arg(11), arg(12), arg(13), arg(14), arg(15), boolean(args, 16), boolean(args, 17), boolean(args, 18));
            numbers(&[hits.main, hits.sub, hits.daken, hits.kick, hits.zanshin])
        }
        "get_mv_ninjutsu" => {
            let (m, v) = get_mv_ninjutsu(NinjutsuTier::from_name(text(args, 0)), arg(1));
            numbers(&[m, v])
        }
        "get_mv_blm" => {
            let tier = BlmTier::from_name(text(args, 1))?;
            let element = BlmElement::from_name(text(args, 0))?;
            let (m, v, window) = get_mv_blm(element, tier, arg(2))?;
            numbers(&[m, v, window])
        }
        "get_enspell_damage" => number(get_enspell_damage(arg(0), arg(1), arg(2))?),
        "get_dstat_macc" => number(get_dstat_macc(arg(0), arg(1))),
        "get_magic_hit_rate" => number(get_magic_hit_rate(arg(0), arg(1))?),
        "get_resist_state_average" => number(get_resist_state_average(arg(0))),
        "get_resist_state" => number(get_resist_state(arg(0), tape)),
        "get_weapon_bonus" => number(get_weapon_bonus(WeaponBonusWeapon::from_name(text(args, 0)), WeaponBonusWeapon::from_name(text(args, 1)), WeaponBonusSkill::from_name(text(args, 2)))),
        function => panic!("unhandled frozen kernel {function}"),
    };
    Ok(value)
}

pub fn assert_value(actual: &Value, expected: &Value, exact: bool, path: &str) {
    if let Some(expected) = expected.as_array() {
        let actual = actual.as_array().unwrap_or_else(|| panic!("{path}: expected array, actual {actual}"));
        assert_eq!(actual.len(), expected.len(), "{path}");
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_value(actual, expected, exact, &format!("{path}[{index}]"));
        }
    } else if expected.is_number() || expected.get("nonfinite").is_some() {
        let actual = numeric(actual);
        let expected = numeric(expected);
        observation::record(actual, expected, exact, path);
        if expected.is_nan() { assert!(actual.is_nan(), "{path}: {actual:?} != NaN"); }
        else if expected.is_infinite() || expected == 0.0 || exact {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{path}: {actual:?} != {expected:?}");
        } else {
            let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
            assert!(actual.is_finite() && (actual - expected).abs() <= tolerance, "{path}: {actual:?} != {expected:?}, tolerance {tolerance}");
        }
    } else {
        assert_eq!(actual, expected, "{path}");
    }
}

pub fn replay_row(fixture: &Value, row: &Value) -> Value {
    let path = format!("{}.{}", fixture["id"].as_str().unwrap(), row["label"].as_str().unwrap());
    let function = fixture["input"]["function"].as_str().unwrap();
    let mut tape = StrictTape::for_row(fixture, row);
    let result = execute(fixture, row, &mut tape);
    tape.assert_exhausted();
    if let Some(before) = row.get("rng_before") {
        let after = &row["rng_after"];
        let selected = if function == "get_resist_state" { "NumpyHost" } else { "NumbaPython" };
        let other = if selected == "NumpyHost" { "NumbaPython" } else { "NumpyHost" };
        assert_eq!(before[other], after[other], "{path}: unrelated stream changed in oracle evidence");
        if tape.draw_count() == 0 { assert_eq!(before[selected], after[selected], "{path}: unrecorded oracle draw"); }
        else {
            let position = before[selected]["position"].as_u64().unwrap() as usize;
            let expected_position = (position + tape.draw_count() * 2 - 1) % 624 + 1;
            assert_eq!(after[selected]["position"].as_u64().unwrap() as usize, expected_position, "{path}: uniform count versus frozen cursor");
        }
    }
    match row["outcome"].as_str().unwrap() {
        "success" => {
            let actual = result.unwrap_or_else(|error| panic!("{path}: {error}"));
            // These exact comparisons cannot mask an int-stage, coefficient,
            // or categorical transition behind the continuous pDIF tolerance.
            let exact = matches!(function, "get_tp" | "get_hit_rate" | "get_fstr" | "get_fstr2" | "get_dex_crit" | "get_phys_damage" | "get_avg_phys_damage" | "get_mv_ninjutsu" | "get_mv_blm" | "get_enspell_damage" | "get_dstat_macc" | "get_magic_hit_rate" | "get_resist_state" | "get_weapon_bonus");
            assert_value(&actual, &row["result"], exact, &path);
            if let Some(pair) = row.get("seeded_adapter_pair") {
                assert_eq!(pair["exact_equal"], true, "{path}: frozen adapter was not equivalent");
                assert_value(&actual, &pair["compiled_or_live_result"], exact, &path);
                assert_value(&actual, &pair["adapter_result"], exact, &path);
            }
            actual
        }
        "error" => {
            let error = result.expect_err(&path);
            match error {
                CalcError::Legacy { operation, field, category, message } => {
                    assert_eq!(operation, function, "{path}: error operation");
                    let expected_field = match function {
                        "get_tp" => "base_tp",
                        "get_pdif_melee" | "get_pdif_ranged" | "get_avg_pdif_melee" | "get_avg_pdif_ranged" => "enemy_defense",
                        "get_mv_blm" if row["label"] == "unknown_tier" => "tier",
                        "get_mv_blm" => "element",
                        _ => panic!("{path}: unhandled error field"),
                    };
                    assert_eq!(field, expected_field, "{path}: error field");
                    assert_eq!(category, row["exception"]["class"].as_str().unwrap(), "{path}: error class");
                    assert_eq!(message, row["exception"]["message"].as_str().unwrap(), "{path}: error message");
                    Value::Object([
                        ("error".to_owned(), Value::from(category)),
                        ("field".to_owned(), Value::from(field)),
                        ("message".to_owned(), Value::from(message)),
                    ].into_iter().collect())
                }
                other => panic!("{path}: wrong error kind {other:?}"),
            }
        }
        outcome => panic!("{path}: unknown outcome {outcome}"),
    }
}

pub fn replay_where(predicate: impl Fn(&Value) -> bool) {
    for fixture in fixtures().iter().filter(|fixture| predicate(fixture)) {
        assert_eq!(fixture["expected"]["outcome"], "result");
        for row in fixture["expected"]["value"]["rows"].as_array().unwrap() { replay_row(fixture, row); }
    }
}
