pub mod boundaries;

use std::sync::{Arc, LazyLock};
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wsdist_core::actions::sample_round;
use wsdist_core::math::{get_delay_timing, get_dex_crit, get_tp, python_round1};
use wsdist_core::model::{
    CalcError, DistributionRequest, DistributionResult, DpsRequest, DpsResult, Enemy,
    EvaluationContext, Metric, ModelState, PieceKind, Player, RandomSource, RoundSample,
    SimulationEventKind, SkillType, Slot, WsRequest, WsType, build_player,
};
use wsdist_core::rng::RandomSources;
use wsdist_core::simulation::{sample_distribution, simulate_dps};
use wsdist_core::stats::Stat;
use wsdist_core::weaponskills::resolve_name;

use crate::native_support::{CountingRandom, expect_native, num, random_value};
use crate::support::{FrozenCatalog, assert_error, assert_number, enemy, number};

pub fn assert_close(actual: f64, expected: f64, label: &str) {
    crate::support::observation::record(actual, expected, false, label);
    if !expected.is_finite() {
        if expected.is_nan() { assert!(actual.is_nan(), "{label}: NaN class"); }
        else { assert_eq!(actual, expected, "{label}: infinity class/sign"); }
    } else if expected == 0.0 {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{label}: signed zero");
    } else {
        let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
        assert!(actual.is_finite() && (actual - expected).abs() <= tolerance,
            "{label}: actual {actual:.17e}, expected {expected:.17e}, tolerance {tolerance:.17e}");
    }
}

fn native_context(frozen: &FrozenCatalog, input: &Value) -> EvaluationContext<CountingRandom> {
    EvaluationContext::new(Arc::clone(&frozen.catalog),
        CountingRandom::seeded(input["seeds"]["numpy_host"].as_u64().unwrap()))
}

pub fn ws_request(value: &Value) -> WsRequest {
    WsRequest {
        skill: resolve_name(value["ws_name"].as_str().unwrap()).unwrap(),
        ws_type: WsType::from_name(value["ws_type"].as_str().unwrap()).unwrap(),
        tp: value.get("input_tp").map_or(0.0, number),
        metric: Metric::new(value.get("input_metric").and_then(Value::as_str).unwrap_or("Damage dealt")),
    }
}

pub fn dps_request<R: RandomSource>(frozen: &FrozenCatalog, input: &Value, ctx: &EvaluationContext<R>, label: &str) -> DpsRequest {
    DpsRequest {
        tp_player: frozen.player(&input["tp_player"], &ctx.state, label),
        ws_player: frozen.player(&input["ws_player"], &ctx.state, label),
        enemy: enemy(&input["enemy"]), ws: ws_request(&input["request"]),
        threshold: number(&input["request"]["ws_threshold"]), trace: true,
    }
}

fn assert_model_changes(frozen: &FrozenCatalog, state: &ModelState, changes: &Value, label: &str) {
    let changes = changes.as_array().unwrap();
    for piece in frozen.catalog.pieces() {
        let expected = changes.iter().find(|change| frozen.gear(&change["before"]) == piece.id);
        let expected_skill = expected.map_or(piece.skill_type, |change|
            SkillType::from_name(change["after"]["skill_type"].as_str().unwrap()).unwrap());
        assert_eq!(state.skill(piece), expected_skill, "{label}: metadata state for original identity {}", piece.original_id);
        if let Some(change) = expected {
            assert_eq!(piece.skill_type.name(), change["before"]["skill_type"].as_str().unwrap(), "{label}: shared catalog remains immutable");
        }
    }
}

struct DpsPlayers {
    tp: Player,
    ws: Player,
    tp_after_build_state: ModelState,
}

fn build_dps_players(
    request: &DpsRequest, ctx: &mut EvaluationContext<CountingRandom>,
) -> DpsPlayers {
    let mut tp = Player::new(request.tp_player.gearset);
    build_player(&request.tp_player, ctx, &mut tp).unwrap();
    let tp_after_build_state = ctx.state.clone();
    let mut ws = Player::new(request.ws_player.gearset);
    build_player(&request.ws_player, ctx, &mut ws).unwrap();
    DpsPlayers { tp, ws, tp_after_build_state }
}

fn assert_dps_players(
    frozen: &FrozenCatalog, players: &DpsPlayers, state: &ModelState,
    expected: &Value, label: &str,
) {
    frozen.assert_player(&players.tp, &players.tp_after_build_state, &expected["players"]["tp_after_build"], label);
    frozen.assert_player(&players.tp, state, &expected["players"]["tp_after_ws_build"], label);
    frozen.assert_player(&players.ws, state, &expected["players"]["ws_after_build"], label);
}

fn assert_bound_inputs<R: RandomSource>(tp: &Player, ws: &Player, enemy: &Enemy, ctx: &EvaluationContext<R>, bound: &Value, label: &str) {
    let catalog = &ctx.catalog;
    let main = catalog.piece(tp.gearset[Slot::Main.index()]);
    let sub = catalog.piece(tp.gearset[Slot::Sub.index()]);
    let ws_main = catalog.piece(ws.gearset[Slot::Main.index()]);
    let ws_sub = catalog.piece(ws.gearset[Slot::Sub.index()]);
    let ammo = catalog.piece(tp.gearset[Slot::Ammo.index()]);
    let dual = (sub.kind == PieceKind::Weapon || ctx.state.skill(main) == SkillType::HandToHand)
        && (ws_sub.kind == PieceKind::Weapon || ctx.state.skill(ws_main) == SkillType::HandToHand);
    let stats = &tp.stats;
    let timing = [stats.get(Stat::Delay1),
        if dual && ctx.state.skill(ws_main) != SkillType::HandToHand { stats.get(Stat::Delay2) } else { 0.0 },
        stats.get(Stat::DualWield) / 100.0, stats.get(Stat::MartialArts),
        stats.get(Stat::MagicHaste), stats.get(Stat::JaHaste), stats.get(Stat::GearHaste)];
    assert_eq!(bound["timing_arguments"].as_array().unwrap().len(), timing.len());
    for (actual, expected) in timing.into_iter().zip(bound["timing_arguments"].as_array().unwrap()) { assert_number(actual, expected, label); }
    let seconds = get_delay_timing(timing[0], timing[1], timing[2], timing[3], timing[4], timing[5], timing[6]);
    assert_number(seconds, &bound["seconds_per_round"], label);
    let regain = stats.get(Stat::DualWield) * if main.name.as_ref() == "Gokotai" { 1.0 } else { 0.0 } + stats.get(Stat::Regain);
    assert_number(regain, &bound["regain"], label);
    assert_number(python_round1((seconds / 3.0) * regain), &bound["rounded_round_regain"], label);
    let delay = ((stats.get(Stat::Delay1) + stats.get(Stat::Delay2)) / 2.0 - stats.get(Stat::MartialArts)) * (1.0 - stats.get(Stat::DualWield) / 100.0);
    assert_number(delay, &bound["modified_delay"], label);
    let crit = stats.get(Stat::CritRate) / 100.0 + get_dex_crit(stats.get(Stat::Dex), enemy.stats.get(Stat::Agi));
    let crit = if crit > 1.0 { 1.0 } else { crit };
    let stp = stats.get(Stat::StoreTp) / 100.0 + (50.0 / 100.0) * crit * if main.name.as_ref() == "Karambit" { 1.0 } else { 0.0 };
    assert_number(stp, &bound["store_tp_argument"], label);
    let hit_delay = if ctx.state.skill(main) == SkillType::HandToHand { delay / 2.0 } else { delay };
    for (actual, expected) in [1.0, hit_delay, stp].into_iter().zip(bound["normal_hit_arguments"].as_array().unwrap()) { assert_number(actual, expected, label); }
    assert_number(get_tp(1.0, hit_delay, stp, false).unwrap(), &bound["normal_hit_tp"], label);
    if !bound["reachable_zanshin_hit_tp"].is_null() {
        assert_number(get_tp(1.0, hit_delay, stp, tp.main_job.name() == "sam").unwrap(), &bound["reachable_zanshin_hit_tp"], label);
    }
    if !bound["reachable_daken_hit_tp"].is_null() {
        assert_eq!(ctx.state.skill(ammo), SkillType::Throwing, "{label}: reachable Daken ammo");
        assert_number(get_tp(1.0, stats.get(Stat::AmmoDelay), stp, false).unwrap(), &bound["reachable_daken_hit_tp"], label);
    }
}

fn assert_same_model_state(frozen: &FrozenCatalog, actual: &ModelState, expected: &ModelState, label: &str) {
    for piece in frozen.catalog.pieces() {
        assert_eq!(actual.override_for(piece.id), expected.override_for(piece.id),
            "{label}: built and sampled model state");
    }
}

fn error_value(error: &CalcError) -> Value {
    match error {
        CalcError::Data { path, json_path, message } => json!({
            "kind": "Data", "path": path, "json_path": json_path, "message": message,
        }),
        CalcError::Legacy { operation, field, category, message } => json!({
            "kind": "Legacy", "operation": operation, "field": field, "category": category, "message": message,
        }),
        CalcError::Input { operation, field, message } => json!({
            "kind": "Input", "operation": operation, "field": field, "message": message,
        }),
        CalcError::Interrupted { operation } => json!({"kind": "Interrupted", "operation": operation}),
        CalcError::NonProgress { operation, starting_tp, threshold, upper_bound } => json!({
            "kind": "NonProgress", "operation": operation, "starting_tp": num(*starting_tp),
            "threshold": num(*threshold), "upper_bound": num(*upper_bound),
        }),
    }
}

fn sampled_projection<T: Serialize>(
    result: &Result<T, CalcError>, context: &EvaluationContext<CountingRandom>,
) -> Value {
    let hits = context.action_trace.as_ref().expect("requested simulation hit trace");
    let mut observation = json!({
        "hit_count": hits.len(),
        "hits_sha256": format!("{:x}", Sha256::digest(serde_json::to_vec(hits).unwrap())),
        "random": random_value(&context.random),
    });
    let fields = observation.as_object_mut().unwrap();
    match result {
        Ok(result) => { fields.insert("result".into(), serde_json::to_value(result).unwrap()); }
        Err(error) => { fields.insert("error".into(), error_value(error)); }
    }
    observation
}

pub struct DpsRun {
    pub context: EvaluationContext<CountingRandom>,
    pub result: Result<DpsResult, CalcError>,
    request: DpsRequest,
    players: DpsPlayers,
    build_context: EvaluationContext<CountingRandom>,
}

pub fn run_dps(case: &Value, cat: &FrozenCatalog) -> DpsRun {
    let input = &case["input"];
    let label = case["id"].as_str().unwrap();
    let mut context = native_context(cat, input);
    let request = dps_request(cat, input, &context, label);
    let mut build_context = native_context(cat, input);
    let players = build_dps_players(&request, &mut build_context);
    context.action_trace = Some(Vec::new());
    let result = simulate_dps(&request, &mut context, &AtomicBool::new(false));
    DpsRun { context, result, request, players, build_context }
}

pub fn assert_dps_legacy(case: &Value, run: &DpsRun, cat: &FrozenCatalog) {
    let label = case["id"].as_str().unwrap();
    assert_eq!(case["expected"]["outcome"], "result", "{label}: frozen capture outcome");
    let expected = &case["expected"]["value"];
    assert_dps_players(cat, &run.players, &run.build_context.state, expected, label);
    assert_bound_inputs(&run.players.tp, &run.players.ws, &run.request.enemy,
        &run.build_context, &expected["bound"], label);
    if expected["status"] == "legacy_exception" {
        assert_error(run.result.as_ref().expect_err("captured DPS exception"), &expected["error"], label);
    } else {
        assert_eq!(expected["status"], "returned", "{label}: complete driver capture");
        let result = run.result.as_ref().unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_close(result.ws_tp_bonus,
            expected["players"]["ws_after_build"]["stats"].get("TP Bonus").map_or(0.0, number), label);
        assert_eq!(result.total_series.len(), result.round_count + result.ws_count,
            "{label}: every action's distinct total timestamp");
        assert_eq!(result.events.len(), result.total_series.len(), "{label}: requested action trace completeness");
        assert!(result.final_time >= 7200.0, "{label}: complete final cycle crosses the actual horizon");
        assert_eq!(result.events.last().unwrap().kind, SimulationEventKind::WeaponSkill,
            "{label}: last stopping action is always a WS");
    }
    // axis_labels remain immutable Python plotting provenance: DpsResult has no label surface.
    assert_model_changes(cat, &run.context.state, &expected["catalog_metadata_changes"], label);
    assert_same_model_state(cat, &run.context.state, &run.build_context.state, label);
}

pub fn project_dps(run: &DpsRun, _cat: &FrozenCatalog) -> Value {
    sampled_projection(&run.result, &run.context)
}

pub struct DistributionRun {
    pub context: EvaluationContext<CountingRandom>,
    pub result: Result<DistributionResult, CalcError>,
    player: Player,
    build_context: EvaluationContext<CountingRandom>,
}

pub fn run_distribution(case: &Value, cat: &FrozenCatalog) -> DistributionRun {
    let input = &case["input"];
    let label = case["id"].as_str().unwrap();
    let mut context = native_context(cat, input);
    let request = DistributionRequest {
        player: cat.player(&input["player"], &context.state, label),
        enemy: enemy(&input["enemy"]), ws: ws_request(&input["request"]),
    };
    let mut build_context = native_context(cat, input);
    let mut player = Player::new(request.player.gearset);
    build_player(&request.player, &mut build_context, &mut player).unwrap();
    context.action_trace = Some(Vec::new());
    let result = sample_distribution(&request, &mut context, &AtomicBool::new(false));
    DistributionRun { context, result, player, build_context }
}

pub fn assert_distribution_legacy(case: &Value, run: &DistributionRun, cat: &FrozenCatalog) {
    let label = case["id"].as_str().unwrap();
    assert_eq!(case["expected"]["outcome"], "result", "{label}: captured distribution outcome");
    let expected = &case["expected"]["value"];
    cat.assert_player(&run.player, &run.build_context.state, &expected["player"], label);
    if expected["status"] == "legacy_exception" {
        assert_error(run.result.as_ref().expect_err("captured distribution exception"), &expected["error"], label);
    } else {
        assert_eq!(expected["status"], "returned", "{label}: complete distribution capture");
        let result = run.result.as_ref().unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_eq!(result.damage.len(), 20000, "{label}: every fixed-count damage sample");
        assert_eq!(result.tp.len(), 20000, "{label}: every fixed-count TP sample");
        assert_eq!(result.histogram.density.len(), 300, "{label}: all density bins");
        assert_eq!(result.histogram.edges.len(), 301, "{label}: all bin edges");
    }
    assert_model_changes(cat, &run.context.state, &expected["catalog_metadata_changes"], label);
    assert_same_model_state(cat, &run.context.state, &run.build_context.state, label);
}

pub fn project_distribution(run: &DistributionRun, _cat: &FrozenCatalog) -> Value {
    sampled_projection(&run.result, &run.context)
}

pub struct NonprogressRun {
    pub context: EvaluationContext<CountingRandom>,
    pub result: Result<DpsResult, CalcError>,
    pub diagnostic_context: EvaluationContext<CountingRandom>,
    pub diagnostic: RoundSample,
    request: DpsRequest,
    players: DpsPlayers,
    initial_random: RandomSources,
}

pub fn run_nonprogress(case: &Value, cat: &FrozenCatalog) -> NonprogressRun {
    let input = &case["input"];
    let label = case["id"].as_str().unwrap();
    let mut context = native_context(cat, input);
    let initial_random = context.random.inner.clone();
    let request = dps_request(cat, input, &context, label);
    let mut diagnostic_context = native_context(cat, input);
    let players = build_dps_players(&request, &mut diagnostic_context);
    let result = simulate_dps(&request, &mut context, &AtomicBool::new(false));
    diagnostic_context.action_trace = Some(Vec::new());
    let diagnostic = sample_round(&players.tp, &request.enemy, 0.0, request.threshold, &mut diagnostic_context)
        .unwrap_or_else(|error| panic!("{label}: diagnostic round: {error}"));
    NonprogressRun { context, result, diagnostic_context, diagnostic, request, players, initial_random }
}

pub fn assert_nonprogress_legacy(case: &Value, run: &NonprogressRun, cat: &FrozenCatalog) {
    let label = case["id"].as_str().unwrap();
    assert_eq!(case["expected"]["outcome"], "result", "{label}: nonprogress capture outcome");
    let expected = &case["expected"]["value"];
    assert_eq!(expected["status"], "proven_nonterminating_reference");
    assert_dps_players(cat, &run.players, &run.diagnostic_context.state, expected, label);
    assert_bound_inputs(&run.players.tp, &run.players.ws, &run.request.enemy,
        &run.diagnostic_context, &expected["bound"], label);
    match run.result.as_ref().expect_err("required no-TP guard") {
        CalcError::NonProgress { operation, starting_tp, threshold, upper_bound } => {
            assert_eq!(*operation, "simulate_dps");
            assert_eq!(*starting_tp, 0.0);
            assert_eq!(threshold.to_bits(), run.request.threshold.to_bits());
            assert_number(*upper_bound, &expected["bound"]["conservative_tp_upper_bound"], label);
        }
        other => panic!("{label}: required no-TP guard, got {other}"),
    }
    assert_eq!(run.context.random.inner, run.initial_random, "{label}: guard consumes neither random stream");
    assert_eq!(run.context.random.uniforms() + run.context.random.bounded + run.context.random.reseeds, 0,
        "{label}: guard makes no random calls");
    assert_model_changes(cat, &run.context.state, &expected["catalog_metadata_changes"], label);
    // reason explains the Python driver's uninvoked loop; it remains fixture provenance, not a core output.
}

pub fn project_nonprogress(run: &NonprogressRun, _cat: &FrozenCatalog) -> Value {
    json!({
        "diagnostic_round": serde_json::to_value(run.diagnostic).unwrap(),
        "hits": run.diagnostic_context.action_trace.as_ref().expect("diagnostic hit trace"),
        "random": random_value(&run.diagnostic_context.random),
    })
}

fn expect_case_native(case: &Value, observation: &Value) {
    static INDEX: LazyLock<crate::support::FixtureIndex> = LazyLock::new(crate::support::index);
    let label = case["id"].as_str().unwrap();
    let entry = INDEX.cases.iter().find(|entry| entry.id == label)
        .unwrap_or_else(|| panic!("{label}: no hash-bearing legacy index entry"));
    assert_eq!(case["operation"], entry.operation, "{label}: indexed operation");
    expect_native(label, &entry.source_sha256, observation);
}

pub fn replay_dps(case: &Value) { replay_dps_measured(case); }

pub fn replay_dps_measured(case: &Value) -> (usize, usize, usize) {
    let frozen = FrozenCatalog::from_input(&case["input"]);
    let run = run_dps(case, &frozen);
    assert_dps_legacy(case, &run, &frozen);
    expect_case_native(case, &project_dps(&run, &frozen));
    (run.result.as_ref().map_or(0, |result| result.round_count + result.ws_count),
        run.context.action_trace.as_ref().unwrap().len(), run.context.random.uniforms())
}

pub fn replay_distribution(case: &Value) { replay_distribution_measured(case); }

pub fn replay_distribution_measured(case: &Value) -> (usize, usize, usize) {
    let frozen = FrozenCatalog::from_input(&case["input"]);
    let run = run_distribution(case, &frozen);
    assert_distribution_legacy(case, &run, &frozen);
    expect_case_native(case, &project_distribution(&run, &frozen));
    (run.result.as_ref().map_or(0, |result| result.damage.len()),
        run.context.action_trace.as_ref().unwrap().len(), run.context.random.uniforms())
}

pub fn replay_nonprogress(case: &Value) {
    let frozen = FrozenCatalog::from_input(&case["input"]);
    let run = run_nonprogress(case, &frozen);
    assert_nonprogress_legacy(case, &run, &frozen);
    expect_case_native(case, &project_nonprogress(&run, &frozen));
}
