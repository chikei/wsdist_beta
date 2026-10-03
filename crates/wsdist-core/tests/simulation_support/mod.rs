mod draws;
mod hits;
pub mod boundaries;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::Value;
use wsdist_core::actions::sample_round;
use wsdist_core::math::{get_delay_timing, get_dex_crit, get_tp, python_round1};
use wsdist_core::model::{
    CalcError, DistributionRequest, DpsRequest, DpsResult, Enemy, EvaluationContext,
    Metric, ModelState, PieceKind, Player, RandomSource, SimulationEvent,
    SimulationEventKind, SkillType, Slot, WsRequest, WsType, build_player,
};
use wsdist_core::simulation::{sample_distribution, simulate_dps};
use wsdist_core::stats::Stat;
use wsdist_core::weaponskills::resolve_name;

use crate::support::{FrozenCatalog, assert_error, assert_number, enemy, number};
use draws::ObservedRandom;

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

fn observed_context(frozen: &FrozenCatalog, input: &Value) -> EvaluationContext<ObservedRandom> {
    EvaluationContext::new(Arc::clone(&frozen.catalog), ObservedRandom::new(&input["seeds"]))
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

fn build_dps_players<R: RandomSource>(
    frozen: &FrozenCatalog, request: &DpsRequest, expected: &Value,
    ctx: &mut EvaluationContext<R>, label: &str,
) -> (Player, Player) {
    let mut tp = Player::new(request.tp_player.gearset);
    build_player(&request.tp_player, ctx, &mut tp).unwrap();
    frozen.assert_player(&tp, &ctx.state, &expected["players"]["tp_after_build"], label);
    let mut ws = Player::new(request.ws_player.gearset);
    build_player(&request.ws_player, ctx, &mut ws).unwrap();
    frozen.assert_player(&tp, &ctx.state, &expected["players"]["tp_after_ws_build"], label);
    frozen.assert_player(&ws, &ctx.state, &expected["players"]["ws_after_build"], label);
    (tp, ws)
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

fn assert_series(actual: &[[f64; 2]], times: &Value, damage: &Value, label: &str) {
    let times = times.as_array().unwrap();
    let damage = damage.as_array().unwrap();
    assert_eq!(actual.len(), times.len(), "{label}: timestamp count");
    assert_eq!(actual.len(), damage.len(), "{label}: cumulative damage count");
    for ((actual, time), damage) in actual.iter().zip(times).zip(damage) {
        assert_number(actual[0], time, label);
        assert_number(actual[1], damage, label);
    }
}

#[derive(Clone, Copy, Default)]
struct DriverState { time: f64, tp: f64, damage: f64, tp_damage: f64, ws_damage: f64 }

impl DriverState {
    fn assert_captured(self, expected: &Value, label: &str) {
        for (field, actual) in [("time", self.time), ("tp", self.tp), ("damage", self.damage),
            ("tp_damage", self.tp_damage), ("ws_damage", self.ws_damage)] {
            assert_number(actual, &expected[field], label);
        }
    }
    fn advance(&mut self, event: &SimulationEvent) {
        self.time = event.time;
        self.tp = event.tp_after;
        self.damage = event.cumulative_damage;
        match event.kind { SimulationEventKind::Round => self.tp_damage += event.damage,
            SimulationEventKind::WeaponSkill => self.ws_damage += event.damage }
    }
}

fn assert_driver_transitions(result: &DpsResult, evidence: &Value, label: &str) {
    let expected = evidence["simulation_transitions"].as_array().unwrap();
    let mut transition = 0;
    let mut completed = 0;
    let mut state = DriverState::default();
    let mut check = |line: u64, completed: usize, state: DriverState| {
        let captured = expected.get(transition).unwrap_or_else(|| panic!("{label}: extra driver phase"));
        assert_eq!(captured["source_line"].as_u64().unwrap(), line, "{label}: phase order");
        assert_eq!(captured["completed_samples"].as_u64().unwrap(), completed as u64, "{label}: phase action cursor");
        state.assert_captured(&captured["state"], label);
        transition += 1;
    };
    while completed < result.events.len() {
        assert!(state.time < 7200.0, "{label}: no new cycle after the outer horizon");
        check(224, completed, state);
        loop {
            check(225, completed, state);
            let event = &result.events[completed];
            if event.kind == SimulationEventKind::WeaponSkill { break; }
            state.advance(event);
            completed += 1;
        }
        check(248, completed, state);
        state.advance(&result.events[completed]);
        completed += 1;
    }
    assert!(state.time >= 7200.0, "{label}: complete final cycle crosses the actual horizon");
    check(224, completed, state);
    check(269, completed, state);
    drop(check);
    assert_eq!(transition, expected.len(), "{label}: every captured driver transition");
}

fn assert_dps_result(result: &DpsResult, expected: &Value, label: &str) {
    let evidence = &expected["evidence"];
    let final_state = &evidence["final_driver_locals"];
    for (field, actual) in [("time", result.final_time), ("tp", result.final_tp), ("damage", result.damage),
        ("tp_damage", result.tp_damage), ("ws_damage", result.ws_damage), ("time_per_attack_round", result.seconds_per_round)] {
        assert_number(actual, &final_state[field], label);
    }
    assert_eq!(result.round_count, final_state["avg_tp_dmg"].as_array().unwrap().len(), "{label}: round count");
    assert_eq!(result.ws_count, final_state["avg_ws_dmg"].as_array().unwrap().len(), "{label}: WS count");
    let reductions = expected["printed_summary_reductions"].as_array().unwrap();
    for (actual, index) in [(result.average_tp_damage, 0), (result.average_physical_round_damage, 1),
        (result.average_magical_round_damage, 4), (result.average_ws_damage, 7), (result.average_pre_bonus_tp, 8)] {
        assert_number(actual, &reductions[index]["result"], label);
    }
    assert_close(result.ws_tp_bonus, expected["players"]["ws_after_build"]["stats"].get("TP Bonus").map_or(0.0, number), label);
    assert_series(&result.total_series, &final_state["time_list"], &final_state["damage_list"], label);
    assert_series(&result.tp_series, &final_state["tp_time_list"], &final_state["tp_damage_list"], label);
    assert_series(&result.ws_series, &final_state["ws_time_list"], &final_state["ws_damage_list"], label);
    assert_eq!(result.total_series.len(), result.round_count + result.ws_count, "{label}: every action's distinct total timestamp");
    assert_eq!(result.events.len(), result.total_series.len(), "{label}: requested action trace completeness");
    for (curve, series) in expected["plotted_curves"].as_array().unwrap().iter().zip([&result.total_series, &result.tp_series, &result.ws_series]) {
        let x = curve["x"].as_array().unwrap();
        let y = curve["y"].as_array().unwrap();
        assert_eq!(series.len(), x.len(), "{label}: DPS plot event count");
        assert_eq!(series.len(), y.len(), "{label}: DPS plot reduction count");
        for ((point, x), y) in series.iter().zip(x).zip(y) {
            assert_number(point[0], x, label);
            assert_number(point[1] / point[0], y, label);
        }
    }
    let samples = evidence["samples"].as_array().unwrap();
    assert_eq!(result.events.len(), samples.len(), "{label}: full ordered action trajectory");
    for (index, (event, sample)) in result.events.iter().zip(samples).enumerate() {
        let before = &sample["before"];
        let after = if index + 1 == samples.len() { final_state } else { &samples[index + 1]["before"] };
        assert_number(event.tp_before, &before["tp"], label);
        assert_number(event.time, &after["time"], label);
        assert_number(event.tp_after, &after["tp"], label);
        assert_number(event.cumulative_damage, &after["damage"], label);
        assert_number(event.tp_return, &sample["result"][1], label);
        let regain = match sample["kind"].as_str().unwrap() {
            "round" => {
                assert_eq!(event.kind, SimulationEventKind::Round, "{label}: attack order");
                assert_number(event.physical_damage, &sample["result"][0], label);
                assert_number(event.magical_damage, &sample["result"][2], label);
                assert_close(event.damage, number(&sample["result"][0]) + number(&sample["result"][2]), label);
                assert_number(event.tp_before, &sample["input"]["starting_tp"], label);
                assert!(sample["input"]["simulation"].as_bool().unwrap());
                assert!(number(&before["tp"]) < number(&sample["input"]["ws_threshold"]), "{label}: TP threshold round gating");
                number(&expected["bound"]["rounded_round_regain"])
            }
            "ws" => {
                assert_eq!(event.kind, SimulationEventKind::WeaponSkill, "{label}: WS order");
                assert_number(event.damage, &sample["result"][0], label);
                assert_number(event.physical_damage, &sample["locals"]["physical_damage"], label);
                assert_number(event.magical_damage, &sample["locals"]["magical_damage"], label);
                assert_number(event.tp_before, &sample["input"]["input_tp"], label);
                assert!(sample["input"]["simulation"].as_bool().unwrap());
                assert!(!sample["input"]["single"].as_bool().unwrap(), "{label}: continuous DPS disables single-only bonuses");
                number(&final_state["regain_ws_return"])
            }
            other => panic!("{label}: unknown action kind {other}"),
        };
        assert_close(event.regain_tp, regain, label);
        let expected_time = number(&before["time"]) + if event.kind == SimulationEventKind::Round { result.seconds_per_round } else { 2.0 };
        assert_close(event.time, expected_time, label);
        let expected_tp = if event.kind == SimulationEventKind::Round { number(&before["tp"]) + event.tp_return } else { event.tp_return } + regain;
        assert_close(event.tp_after, expected_tp, label);
    }
    assert_eq!(result.events.last().unwrap().kind, SimulationEventKind::WeaponSkill, "{label}: last stopping action is always a WS");
    assert_driver_transitions(result, evidence, label);
}

pub fn replay_dps(case: &Value) { replay_dps_measured(case); }

pub fn replay_dps_measured(case: &Value) -> (usize, usize, usize) {
    let label = case["id"].as_str().unwrap();
    let input = &case["input"];
    assert_eq!(case["expected"]["outcome"], "result", "{label}: frozen capture outcome");
    let expected = &case["expected"]["value"];
    let frozen = FrozenCatalog::from_input(input);
    let mut context = observed_context(&frozen, input);
    let request = dps_request(&frozen, input, &context, label);
    let mut build_context = frozen.context(&input["seeds"]);
    let (tp, ws) = build_dps_players(&frozen, &request, expected, &mut build_context, label);
    assert_bound_inputs(&tp, &ws, &request.enemy, &build_context, &expected["bound"], label);
    context.action_trace = Some(Vec::new());
    let result = simulate_dps(&request, &mut context, &AtomicBool::new(false));
    let samples = result.as_ref().map_or(0, |result| result.events.len());
    if expected["status"] == "legacy_exception" {
        assert_error(&result.unwrap_err(), &expected["error"], label);
    } else {
        assert_eq!(expected["status"], "returned", "{label}: complete driver capture");
        let result = result.unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_dps_result(&result, expected, label);
    }
    let trace = context.action_trace.as_ref().unwrap();
    let mut cursor = 0;
    for sample in expected["evidence"]["samples"].as_array().unwrap() { hits::assert_sample_hits(sample, trace, &mut cursor, label); }
    assert_eq!(cursor, trace.len(), "{label}: no extra or missing sampled hit records");
    context.random.assert_evidence(&input["seeds"], &expected["evidence"], label);
    assert_model_changes(&frozen, &context.state, &expected["catalog_metadata_changes"], label);
    for piece in frozen.catalog.pieces() {
        assert_eq!(context.state.override_for(piece.id), build_context.state.override_for(piece.id), "{label}: shared TP-then-WS model context");
    }
    (samples, trace.len(), context.random.draw_count())
}

pub fn replay_distribution(case: &Value) { replay_distribution_measured(case); }

pub fn replay_distribution_measured(case: &Value) -> (usize, usize, usize) {
    let label = case["id"].as_str().unwrap();
    let input = &case["input"];
    assert_eq!(case["expected"]["outcome"], "result", "{label}: captured distribution outcome");
    let expected = &case["expected"]["value"];
    assert_eq!(expected["status"], "returned", "{label}: complete distribution capture");
    let frozen = FrozenCatalog::from_input(input);
    let mut context = observed_context(&frozen, input);
    let request = DistributionRequest { player: frozen.player(&input["player"], &context.state, label),
    enemy: enemy(&input["enemy"]), ws: ws_request(&input["request"]) };
    let mut build_context = frozen.context(&input["seeds"]);
    let mut player = Player::new(request.player.gearset);
    build_player(&request.player, &mut build_context, &mut player).unwrap();
    frozen.assert_player(&player, &build_context.state, &expected["player"], label);
    context.action_trace = Some(Vec::new());
    let result = sample_distribution(&request, &mut context, &AtomicBool::new(false))
        .unwrap_or_else(|error| panic!("{label}: {error}"));
    assert_eq!(result.damage.len(), 20000, "{label}: every fixed-count damage sample");
    assert_eq!(result.tp.len(), 20000, "{label}: every fixed-count TP sample");
    let damage = expected["damage_samples"].as_array().unwrap();
    let tp = expected["tp_samples"].as_array().unwrap();
    let samples = expected["evidence"]["samples"].as_array().unwrap();
    assert_eq!(damage.len(), 20000);
    assert_eq!(tp.len(), 20000);
    assert_eq!(samples.len(), 20000);
    let trace = context.action_trace.as_ref().unwrap();
    let mut cursor = 0;
    for (index, sample) in samples.iter().enumerate() {
        assert_number(result.damage[index], &damage[index], label);
        assert_number(result.tp[index], &tp[index], label);
        assert_number(result.damage[index], &sample["result"][0], label);
        assert_number(result.tp[index], &sample["result"][1], label);
        assert_eq!(sample["kind"], "ws", "{label}: only weapon skills in the distribution");
        assert!(sample["input"]["single"].as_bool().unwrap(), "{label}: single-only first-hit behavior");
        assert!(sample["input"]["simulation"].as_bool().unwrap());
        assert_number(request.ws.tp, &sample["input"]["input_tp"], label);
        hits::assert_sample_hits(sample, trace, &mut cursor, label);
    }
    assert_eq!(cursor, trace.len(), "{label}: complete distribution sampled attempt/proc trace");
    let summary = &expected["summary"];
    for (field, actual) in [("minimum", result.minimum), ("mean", result.mean),
        ("median", result.median), ("maximum", result.maximum)] {
        assert_number(actual, &summary[field], label);
    }
    assert_eq!(result.title_mean.to_bits(), number(&summary["title_mean_integer"]).to_bits(), "{label}: truncated title mean integer");
    assert_eq!(result.title_median.to_bits(), number(&summary["title_median_integer"]).to_bits(), "{label}: truncated title median integer");
    let counts = summary["histogram_counts"].as_array().unwrap().iter().map(|count| count.as_u64().unwrap()).collect::<Vec<_>>();
    assert_eq!(result.histogram.counts, counts, "{label}: exact inclusive-edge 300-bin counts");
    assert_eq!(result.histogram.density.len(), 300, "{label}: all density bins");
    assert_eq!(result.histogram.edges.len(), 301, "{label}: all bin edges");
    for (actual, expected) in result.histogram.edges.iter().zip(summary["histogram_edges"].as_array().unwrap()) { assert_number(*actual, expected, label); }
    for (actual, expected) in result.histogram.density.iter().zip(summary["histogram_density"].as_array().unwrap()) { assert_number(*actual, expected, label); }
    let actual_plot = &expected["plot"]["actual_matplotlib_histogram_calls"][0];
    for (actual, expected) in result.histogram.edges.iter().zip(actual_plot["edges"].as_array().unwrap()) { assert_number(*actual, expected, label); }
    for (actual, expected) in result.histogram.density.iter().zip(actual_plot["density"].as_array().unwrap()) { assert_number(*actual, expected, label); }
    for marker in expected["plot"]["mean_marker"].as_array().unwrap() {
        for x in marker["x"].as_array().unwrap() { assert_number(result.mean, x, label); }
    }
    context.random.assert_evidence(&input["seeds"], &expected["evidence"], label);
    assert_model_changes(&frozen, &context.state, &expected["catalog_metadata_changes"], label);
    for piece in frozen.catalog.pieces() {
        assert_eq!(context.state.override_for(piece.id), build_context.state.override_for(piece.id), "{label}: distribution model state");
    }
    (result.damage.len(), trace.len(), context.random.draw_count())
}

pub fn replay_nonprogress(case: &Value) {
    let label = case["id"].as_str().unwrap();
    let input = &case["input"];
    let expected = &case["expected"]["value"];
    assert_eq!(expected["status"], "proven_nonterminating_reference");
    let frozen = FrozenCatalog::from_input(input);
    let mut context = observed_context(&frozen, input);
    let initial_random = context.random.inner.clone();
    let request = dps_request(&frozen, input, &context, label);
    let mut diagnostic_context = observed_context(&frozen, input);
    let (tp, ws) = build_dps_players(&frozen, &request, expected, &mut diagnostic_context, label);
    assert_bound_inputs(&tp, &ws, &request.enemy, &diagnostic_context, &expected["bound"], label);
    match simulate_dps(&request, &mut context, &AtomicBool::new(false)).unwrap_err() {
        CalcError::NonProgress { operation, starting_tp, threshold, upper_bound } => {
            assert_eq!(operation, "simulate_dps");
            assert_eq!(starting_tp, 0.0);
            assert_eq!(threshold.to_bits(), request.threshold.to_bits());
            assert_number(upper_bound, &expected["bound"]["conservative_tp_upper_bound"], label);
        }
        other => panic!("{label}: required no-TP guard, got {other}"),
    }
    assert_eq!(context.random.inner, initial_random, "{label}: guard consumes neither random stream");
    assert_model_changes(&frozen, &context.state, &expected["catalog_metadata_changes"], label);
    diagnostic_context.action_trace = Some(Vec::new());
    let diagnostic = sample_round(&tp, &request.enemy, 0.0, request.threshold, &mut diagnostic_context).unwrap();
    for (actual, expected) in [diagnostic.physical_damage, diagnostic.tp_return, diagnostic.magical_damage].into_iter()
        .zip(expected["observed_one_compiled_sampling_round"].as_array().unwrap()) { assert_number(actual, expected, label); }
    let samples = expected["evidence"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 1, "{label}: source diagnostics did not invoke an infinite driver");
    let mut cursor = 0;
    let trace = diagnostic_context.action_trace.as_ref().unwrap();
    hits::assert_sample_hits(&samples[0], trace, &mut cursor, label);
    assert_eq!(cursor, trace.len());
    diagnostic_context.random.assert_evidence(&input["seeds"], &expected["evidence"], label);
}
