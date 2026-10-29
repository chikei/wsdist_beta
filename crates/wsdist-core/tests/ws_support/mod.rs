use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use wsdist_core::catalog::Catalog;
use wsdist_core::model::{Abilities, CalcError, Enemy, EvaluationContext, Job, LegacyF64, Player, RandomSource, RandomStream, Slot, WscContribution};
use wsdist_core::stats::{Stat, Stats};
use wsdist_core::weaponskills::{WsScaling, resolve_name, weaponskill_info};

#[path = "../support/observation.rs"]
pub mod observation;

// An empty strict random protocol: setup must never draw, choose, or reseed.
// These are rejected operations, not a no-op generator or numerical fallback.
#[derive(Clone)]
pub struct NoSetupDraws;

impl RandomSource for NoSetupDraws {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        panic!("unexpected WS setup uniform: {stream:?} [{low}, {high}]")
    }
    fn bounded(&mut self, upper_exclusive: usize) -> usize {
        panic!("unexpected WS setup bounded draw: {upper_exclusive}")
    }
    fn reseed(&mut self, seed: u64) { panic!("unexpected WS setup reseed: {seed}") }
}

#[derive(Deserialize)]
pub struct IndexedCase {
    pub id: String,
    pub operation: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct FixtureIndex { pub cases: Vec<IndexedCase> }

pub fn resource_root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
pub fn fixture_root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy") }

pub fn document(path: &Path) -> Value {
    wsdist_fixtures::read_value(path, None).unwrap_or_else(|error| panic!("{error}"))
}

pub fn index(root: &Path) -> FixtureIndex {
    serde_json::from_value(document(&root.join("combat.json"))).expect("combat fixture index")
}

pub fn number(value: &Value) -> f64 {
    if let Some(number) = value.as_f64() { number }
    else { serde_json::from_value::<LegacyF64>(value.clone()).expect("captured number").0 }
}

pub fn assert_number(label: &str, actual: f64, expected: &Value) {
    let expected = number(expected);
    observation::record(actual, expected, false, label);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}: expected NaN, got {actual:?}");
    } else if expected.is_infinite() || expected == 0.0 {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{label}: expected {expected:?}, got {actual:?}");
    } else {
        let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
        assert!(actual.is_finite() && (actual - expected).abs() <= tolerance,
            "{label}: expected {expected:.17e}, got {actual:.17e}; error {:.17e}, tolerance {tolerance:.17e}",
            (actual - expected).abs());
    }
}

fn wsc_contributions(value: &Value) -> Vec<WscContribution> {
    value.as_array().expect("captured WSC array").iter().map(|term| {
        let stat = Stat::from_name(term[0].as_str().expect("WSC stat name")).expect("known WSC stat");
        WscContribution { stat, percent: number(&term[1]) }
    }).collect()
}

pub fn boundary_player(case: &Value, catalog: &Catalog) -> Player {
    let label = case["id"].as_str().expect("case ID");
    let captured = &case["expected"]["value"]["player_at_action_boundary"];
    let mut gearset = [catalog.empty; Slot::COUNT];
    for &slot in Slot::ALL {
        let piece = &captured["gearset"][slot.name()];
        let original_id = piece["id"].as_u64().expect("original gear ID") as u32;
        let id = catalog.by_original_id(original_id).unwrap_or_else(|| panic!("{label}: {} missing original gear ID {original_id}", slot.name()));
        // Setup reads the base main name, not an alias or a future player build.
        assert_eq!(catalog.piece(id).name.as_ref(), piece["name"].as_str().unwrap(), "{label}: {} base name", slot.name());
        gearset[slot.index()] = id;
    }
    let mut player = Player::new(gearset);
    player.main_job = Job::parse_player_name(captured["main_job"].as_str().unwrap()).expect("main job");
    let sub = captured["sub_job"].as_str().unwrap();
    player.sub_job = if sub == "none" { None } else { Some(Job::parse_player_name(sub).expect("subjob")) };
    player.master_level = captured["master_level"].as_i64().unwrap() as i32;
    player.main_job_level = captured["main_job_level"].as_i64().unwrap() as i32;
    player.sub_job_level = captured["sub_job_level"].as_i64().unwrap() as i32;
    player.stats = serde_json::from_value::<Stats>(captured["stats"].clone()).unwrap_or_else(|error| panic!("{label}: boundary stats: {error}"));
    player.abilities = serde_json::from_value::<Abilities>(captured["abilities"].clone()).unwrap_or_else(|error| panic!("{label}: boundary abilities: {error}"));
    player.wsc = wsc_contributions(&captured["wsc"]);
    player.wyvern_bonus_attack = captured["wyvern_bonus_attack"].as_bool().unwrap();
    player
}

pub fn boundary_enemy(case: &Value) -> Enemy {
    let label = case["id"].as_str().expect("case ID");
    Enemy { name: Arc::from("captured action-boundary enemy"), location: Arc::from("captured"),
        stats: serde_json::from_value(case["expected"]["value"]["enemy_at_action_boundary"].clone())
            .unwrap_or_else(|error| panic!("{label}: boundary enemy: {error}")) }
}

pub fn helper_event(case: &Value) -> &Value {
    let label = case["id"].as_str().expect("case ID");
    let events = case["expected"]["value"]["compiled"]["events"].as_array().expect("compiled events");
    let mut matches = events.iter().filter(|event| event["name"] == "weaponskill_info");
    let event = matches.next().unwrap_or_else(|| panic!("{label}: missing weaponskill_info helper event"));
    assert!(matches.next().is_none(), "{label}: multiple setup events require an explicit sequence replay");
    event
}

pub fn evaluate_case(case: &Value, ctx: &EvaluationContext<NoSetupDraws>) -> Result<WsScaling, CalcError> {
    let player = boundary_player(case, &ctx.catalog);
    let enemy = boundary_enemy(case);
    let event = helper_event(case);
    let args = &event["arguments"];
    assert_eq!(args["player"]["model_reference"], "player_at_action_boundary");
    assert_eq!(args["enemy"]["model_reference"], "enemy_at_action_boundary");
    let skill = resolve_name(args["ws_name"].as_str().unwrap())?;
    let bonus = wsc_contributions(&args["wsc_bonus"]);
    weaponskill_info(skill, number(&args["tp"]), &player, &enemy, &bonus, args["dual_wield"].as_bool().unwrap(), ctx)
}

pub fn assert_scaling(label: &str, actual: &WsScaling, expected: &Value) {
    assert_eq!(expected.as_object().expect("scaling object").len(), 17, "{label}: scaling field coverage");
    assert_eq!(actual.hybrid, expected["hybrid"].as_bool().unwrap(), "{label}: hybrid");
    assert_eq!(actual.magical, expected["magical"].as_bool().unwrap(), "{label}: magical");
    assert_eq!(actual.ftp_rep, expected["ftp_rep"].as_bool().unwrap(), "{label}: ftp_rep");
    assert_eq!(actual.nhits as u64, expected["nhits"].as_u64().unwrap(), "{label}: nhits");
    assert_eq!(actual.element.name(), expected["element"].as_str().unwrap(), "{label}: element");
    for (field, value) in [
        ("dSTAT", actual.dstat), ("wsc", actual.wsc), ("ftp", actual.ftp),
        ("player_attack1", actual.player_attack1), ("player_attack2", actual.player_attack2),
        ("player_accuracy1", actual.player_accuracy1), ("player_accuracy2", actual.player_accuracy2),
        ("player_rangedattack", actual.player_rangedattack), ("player_rangedaccuracy", actual.player_rangedaccuracy),
        ("enemy_def", actual.enemy_def), ("crit_rate", actual.crit_rate), ("ftp_hybrid", actual.ftp_hybrid),
    ] {
        assert_number(&format!("{label}.{field}"), value, &expected[field]);
    }
}

pub fn assert_error(label: &str, error: &CalcError, expected: &Value) {
    match error {
        CalcError::Legacy { operation, category, message, .. } => {
            assert_eq!(*operation, "weaponskill_info", "{label}: operation");
            assert_eq!(*category, expected["class"].as_str().unwrap(), "{label}: exception class");
            assert_eq!(message, expected["message"].as_str().unwrap(), "{label}: exception message");
        }
        error => panic!("{label}: expected captured legacy exception {expected}, got {error}"),
    }
}

pub fn verify_case(case: &Value, actual: &Result<WsScaling, CalcError>) {
    let label = case["id"].as_str().expect("case ID");
    let event = helper_event(case);
    match (actual, event.get("result"), event.get("error")) {
        (Ok(actual), Some(expected), None) => assert_scaling(label, actual, expected),
        (Err(error), None, Some(expected)) => assert_error(label, error, expected),
        (Ok(actual), _, _) => panic!("{label}: unexpected successful WS setup {actual:?}; expected {event}"),
        (Err(error), _, _) => panic!("{label}: unexpected WS setup error {error}; expected {event}"),
    }
}
