use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wsdist_fixtures::{Value, to_value};
use wsdist_core::actions::{sample_round, sample_ws};
use wsdist_core::math::get_tp;
use wsdist_core::model::{
    AttackKind, CalcError, DistributionRequest, DpsRequest, EvaluationContext,
    Player, RandomSource, RandomStream, SimulationEventKind, build_player,
};
use wsdist_core::rng::RandomSources;
use wsdist_core::simulation::{sample_distribution, simulate_dps};
use wsdist_core::stats::Stat;

use crate::native_support::object;
use crate::support::{FrozenCatalog, enemy, fixture_root, index};
use super::{dps_request, ws_request};

#[derive(Clone, Copy)]
enum HostPolicy { Seeded, MissAll, LandAll, MissFirstMain }

#[derive(Clone)]
struct CancellingRandom {
    inner: RandomSources,
    stop: Arc<AtomicBool>,
    cancel_at: Option<usize>,
    draws: usize,
    host_draws: usize,
    policy: HostPolicy,
}

impl CancellingRandom {
    fn new(seeds: &Value, stop: Arc<AtomicBool>, cancel_at: Option<usize>, policy: HostPolicy) -> Self {
        Self { inner: RandomSources::from_seed(seeds["numpy_host"].as_u64().unwrap()),
            stop, cancel_at, draws: 0, host_draws: 0, policy }
    }
}

impl RandomSource for CancellingRandom {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        let sampled = self.inner.uniform(stream, low, high);
        self.draws += 1;
        if self.cancel_at == Some(self.draws) { self.stop.store(true, Ordering::Relaxed); }
        if stream == RandomStream::Primary {
            self.host_draws += 1;
            match self.policy {
                HostPolicy::Seeded => sampled,
                HostPolicy::MissAll => high - (high - low) * f64::EPSILON,
                HostPolicy::LandAll => low,
                HostPolicy::MissFirstMain if self.host_draws == 7 => high - (high - low) * f64::EPSILON,
                HostPolicy::MissFirstMain => low,
            }
        } else {
            sampled
        }
    }
    fn bounded(&mut self, _: usize) -> usize { panic!("no simulation choice draws") }
    fn reseed(&mut self, _: u64) { panic!("no simulation reseeding") }
}

fn fixture(suffix: &str) -> Value {
    let entry = index().cases.into_iter().find(|entry| entry.id.ends_with(suffix)).unwrap();
    crate::support::fixture(&fixture_root(), &entry)
}

fn context(frozen: &FrozenCatalog, input: &Value, stop: Arc<AtomicBool>, cancel_at: Option<usize>, policy: HostPolicy) -> EvaluationContext<CancellingRandom> {
    EvaluationContext::new(Arc::clone(&frozen.catalog), CancellingRandom::new(&input["seeds"], stop, cancel_at, policy))
}

fn assert_interrupted(error: CalcError, operation: &str) {
    match error { CalcError::Interrupted { operation: actual } => assert_eq!(actual, operation),
        other => panic!("expected cancellation of {operation}, got {other}") }
}

fn local_piece(mut record: Value, identity: &str, name: &str) -> Value {
    let object = record.as_object_mut().unwrap();
    object.shift_remove("id");
    object.insert("local_identity".into(), Value::from(identity));
    object.insert("name".into(), Value::from(name));
    object.insert("name2".into(), Value::from(name));
    record
}

fn armor(input: &Value, identity: &str, stats: Value) -> Value {
    let mut piece = local_piece(input["tp_player"]["gearset"]["body"].clone(), identity, identity);
    piece["type"] = Value::from("Equipment");
    piece["stats"] = stats;
    piece
}

fn build_players(request: &DpsRequest, ctx: &mut EvaluationContext<CancellingRandom>) -> (Player, Player) {
    let mut tp = Player::new(request.tp_player.gearset);
    build_player(&request.tp_player, ctx, &mut tp).unwrap();
    let mut ws = Player::new(request.ws_player.gearset);
    build_player(&request.ws_player, ctx, &mut ws).unwrap();
    (tp, ws)
}

pub fn cancellation_precedes_builds_or_random_draws() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let input = &case["input"];
    let frozen = FrozenCatalog::from_input(input);
    let stop = Arc::new(AtomicBool::new(true));
    let mut ctx = context(&frozen, input, Arc::clone(&stop), None, HostPolicy::Seeded);
    let request = dps_request(&frozen, input, &ctx, "cancelled before start");
    let initial = ctx.random.inner.clone();
    assert_interrupted(simulate_dps(&request, &mut ctx, &stop).unwrap_err(), "simulate_dps");
    assert_eq!(ctx.random.draws, 0);
    assert_eq!(ctx.random.inner, initial);
    for piece in frozen.catalog.pieces() { assert_eq!(ctx.state.override_for(piece.id), None); }
    let distribution = DistributionRequest { player: request.ws_player, enemy: request.enemy, ws: request.ws };
    assert_interrupted(sample_distribution(&distribution, &mut ctx, &stop).unwrap_err(), "sample_distribution");
    assert_eq!(ctx.random.draws, 0);
    assert_eq!(ctx.random.inner, initial);
}

pub fn cancellation_after_a_round_never_enters_the_following_ws() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let input = &case["input"];
    let frozen = FrozenCatalog::from_input(input);
    let mut baseline = context(&frozen, input, Arc::new(AtomicBool::new(false)), None, HostPolicy::Seeded);
    let mut request = dps_request(&frozen, input, &baseline, "round cancellation");
    request.threshold = 1.0;
    let (tp, _) = build_players(&request, &mut baseline);
    baseline.action_trace = Some(Vec::new());
    sample_round(&tp, &request.enemy, 0.0, request.threshold, &mut baseline).unwrap();
    let first_draws = baseline.random.draws;
    let stop = Arc::new(AtomicBool::new(false));
    let mut ctx = context(&frozen, input, Arc::clone(&stop), Some(first_draws), HostPolicy::Seeded);
    ctx.action_trace = Some(Vec::new());
    assert_interrupted(simulate_dps(&request, &mut ctx, &stop).unwrap_err(), "simulate_dps");
    assert_eq!(ctx.random.draws, first_draws, "shutdown observed before another action");
    assert_eq!(ctx.random.inner, baseline.random.inner, "no WS-only unused draws after round shutdown");
    assert_eq!(to_value(&ctx.action_trace).unwrap(), to_value(&baseline.action_trace).unwrap());
}

pub fn cancellation_in_a_ws_returns_no_partial_distribution() {
    let case = fixture("distribution20000-physical-host11-compiled20240101");
    let input = &case["input"];
    let frozen = FrozenCatalog::from_input(input);
    let mut baseline = context(&frozen, input, Arc::new(AtomicBool::new(false)), None, HostPolicy::Seeded);
    let request = DistributionRequest { player: frozen.player(&input["player"], &baseline.state, "WS cancellation"),
        enemy: enemy(&input["enemy"]), ws: ws_request(&input["request"]) };
    let mut player = Player::new(request.player.gearset);
    build_player(&request.player, &mut baseline, &mut player).unwrap();
    baseline.action_trace = Some(Vec::new());
    sample_ws(&player, &request.enemy, &request.ws, true, &mut baseline).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let mut ctx = context(&frozen, input, Arc::clone(&stop), Some(baseline.random.draws), HostPolicy::Seeded);
    ctx.action_trace = Some(Vec::new());
    assert_interrupted(sample_distribution(&request, &mut ctx, &stop).unwrap_err(), "sample_distribution");
    assert_eq!(ctx.random.draws, baseline.random.draws, "no second distribution sample after shutdown");
    assert_eq!(ctx.random.inner, baseline.random.inner);
    assert_eq!(to_value(&ctx.action_trace).unwrap(), to_value(&baseline.action_trace).unwrap());
}

pub fn unlucky_zero_tp_rounds_continue_until_cancellation() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let input = &case["input"];
    let frozen = FrozenCatalog::from_input(input);
    let mut baseline = context(&frozen, input, Arc::new(AtomicBool::new(false)), None, HostPolicy::MissAll);
    let request = dps_request(&frozen, input, &baseline, "unlucky trajectory");
    let (tp, _) = build_players(&request, &mut baseline);
    let missed = sample_round(&tp, &request.enemy, 0.0, request.threshold, &mut baseline).unwrap();
    assert_eq!(missed.tp_return, 0.0);
    assert_eq!(missed.damage, 0.0);
    let limit = baseline.random.draws * 100;
    let stop = Arc::new(AtomicBool::new(false));
    let mut ctx = context(&frozen, input, Arc::clone(&stop), Some(limit), HostPolicy::MissAll);
    ctx.action_trace = Some(Vec::new());
    assert_interrupted(simulate_dps(&request, &mut ctx, &stop).unwrap_err(), "simulate_dps");
    assert_eq!(ctx.random.draws, limit, "one hundred zero-TP rounds are not a false no-progress proof");
    let hits = ctx.action_trace.unwrap();
    assert_eq!(hits.len(), 100);
    assert!(hits.iter().all(|hit| !hit.landed && hit.tp_return == 0.0));
}

pub fn rounded_regain_alone_can_reach_a_ws() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let mut input = case["input"].clone();
    let piece = armor(&input, "Regain-only TP", object([("Store TP", (-100).into()), ("Regain", 90.into())]));
    input["tp_player"]["gearset"]["body"] = piece.clone();
    input["ws_player"]["gearset"]["body"] = piece;
    input["request"]["ws_threshold"] = Value::from(1);
    let frozen = FrozenCatalog::from_input(&input);
    let mut ctx = frozen.context(&input["seeds"]);
    let request = dps_request(&frozen, &input, &ctx, "regain-only progress");
    let result = simulate_dps(&request, &mut ctx, &AtomicBool::new(false)).unwrap();
    assert_eq!(result.round_count, 1);
    assert_eq!(result.ws_count, 3598);
    assert_eq!(result.final_time, 7200.0);
    assert_eq!(result.final_tp, 60.0);
    let round = &result.events[0];
    assert_eq!(round.kind, SimulationEventKind::Round);
    assert_eq!(round.tp_return, 0.0);
    assert_eq!(round.regain_tp, 120.0);
    assert_eq!(round.tp_after, 120.0);
    for ws in &result.events[1..] {
        assert_eq!(ws.kind, SimulationEventKind::WeaponSkill);
        assert_eq!(ws.tp_return, 0.0);
        assert_eq!(ws.regain_tp, 60.0);
        assert_eq!(ws.tp_after, 60.0);
    }
}

fn assert_special_tp_reachable(mut input: Value, expected_kind: AttackKind, policy: HostPolicy) {
    input["request"]["ws_threshold"] = Value::from(1);
    let frozen = FrozenCatalog::from_input(&input);
    let mut baseline = context(&frozen, &input, Arc::new(AtomicBool::new(false)), None, policy);
    let request = dps_request(&frozen, &input, &baseline, "special-hit-only progress");
    let (tp, _) = build_players(&request, &mut baseline);
    let modified_delay = ((tp.stats.get(Stat::Delay1) + tp.stats.get(Stat::Delay2)) / 2.0
        - tp.stats.get(Stat::MartialArts)) * (1.0 - tp.stats.get(Stat::DualWield) / 100.0);
    assert_eq!(get_tp(1.0, modified_delay, tp.stats.get(Stat::StoreTp) / 100.0, false).unwrap(), 0.0,
        "ordinary hits cannot increase TP in this boundary");
    baseline.action_trace = Some(Vec::new());
    let round = sample_round(&tp, &request.enemy, 0.0, 1.0, &mut baseline).unwrap();
    assert!(round.tp_return >= 1.0, "the eligible special hit reaches the requested WS");
    assert!(baseline.action_trace.as_ref().unwrap().iter().any(|hit| hit.kind == expected_kind && hit.landed && hit.tp_return > 0.0));
    let stop = Arc::new(AtomicBool::new(false));
    let mut ctx = context(&frozen, &input, Arc::clone(&stop), Some(baseline.random.draws), policy);
    ctx.action_trace = Some(Vec::new());
    assert_interrupted(simulate_dps(&request, &mut ctx, &stop).unwrap_err(), "simulate_dps");
    assert_eq!(ctx.random.draws, baseline.random.draws, "guard must admit the reachable special hit without consuming RNG");
    assert_eq!(ctx.random.inner, baseline.random.inner);
    assert_eq!(to_value(&ctx.action_trace).unwrap(), to_value(&baseline.action_trace).unwrap());
}

pub fn reachable_daken_prevents_false_nonprogress() {
    let case = fixture("full7200-dual-host11-compiled20240101");
    let mut input = case["input"].clone();
    let mut main = local_piece(input["tp_player"]["gearset"]["main"].clone(), "Daken-only main", "Daken-only main");
    main["delay"] = Value::from(-165);
    input["tp_player"]["gearset"]["main"] = main;
    input["tp_player"]["gearset"]["sub"] = input["ws_player"]["gearset"]["head"].clone();
    input["tp_player"]["gearset"]["body"] = armor(&input, "Daken-only eligibility", object([("Daken", 100.into())]));
    assert_special_tp_reachable(input, AttackKind::Daken, HostPolicy::LandAll);
}

pub fn reachable_sam_zanshin_prevents_false_nonprogress() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let mut input = case["input"].clone();
    input["tp_player"]["main_job"] = Value::from("sam");
    let mut main = local_piece(input["tp_player"]["gearset"]["main"].clone(), "Zanshin-only main", "Zanshin-only main");
    main["skill_type"] = Value::from("Great Katana");
    main["jobs"] = Value::Array(vec!["sam".into()]);
    main["delay"] = Value::from(-165);
    input["tp_player"]["gearset"]["main"] = main;
    input["tp_player"]["gearset"]["body"] = armor(&input, "Zanshin-only eligibility",
        object([("Zanshin OA2", 100.into()), ("DA", (-1000).into()), ("TA", (-1000).into()), ("QA", (-1000).into())]));
    assert_special_tp_reachable(input, AttackKind::Zanshin, HostPolicy::MissFirstMain);
}

pub fn zero_duration_rounds_with_positive_tp_complete_the_horizon() {
    let case = fixture("full7200-dual-tp-single-ws-host11-compiled20240101");
    let mut input = case["input"].clone();
    let mut main = local_piece(input["tp_player"]["gearset"]["main"].clone(), "Zero-time TP main", "Zero-time TP main");
    main["delay"] = Value::from(0);
    input["tp_player"]["gearset"]["main"] = main;
    input["tp_player"]["gearset"]["body"] = armor(&input, "Zero-time fallback delay", object([("Martial Arts", 480.into())]));
    input["request"]["ws_threshold"] = Value::from(1);
    let frozen = FrozenCatalog::from_input(&input);
    let mut ctx = frozen.context(&input["seeds"]);
    let request = dps_request(&frozen, &input, &ctx, "zero-time reachable rounds");
    let result = simulate_dps(&request, &mut ctx, &AtomicBool::new(false)).unwrap();
    assert_eq!(result.seconds_per_round, 0.0);
    assert_eq!(result.final_time, 7200.0);
    assert_eq!(result.ws_count, 3600);
    assert_eq!(result.events[0].kind, SimulationEventKind::Round);
    assert_eq!(result.events[0].time, 0.0);
    assert_eq!(result.total_series[0][0], 0.0, "a zero-time first point is retained, not repaired");
    let mut previous = 0.0;
    for event in &result.events {
        let elapsed = event.time - previous;
        assert_eq!(elapsed, if event.kind == SimulationEventKind::Round { 0.0 } else { 2.0 });
        previous = event.time;
    }
}

pub fn trace_disabled_keeps_the_same_damage_and_streams_without_events() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let input = &case["input"];
    let frozen = FrozenCatalog::from_input(input);
    let mut traced_ctx = super::native_context(&frozen, input);
    let mut request = dps_request(&frozen, input, &traced_ctx, "trace-invariant trajectory");
    traced_ctx.action_trace = Some(Vec::new());
    let traced = simulate_dps(&request, &mut traced_ctx, &AtomicBool::new(false)).unwrap();
    assert!(!traced.events.is_empty(), "requested driver traces are captured");
    assert!(!traced_ctx.action_trace.as_ref().unwrap().is_empty(), "requested per-hit traces are captured");
    let mut untraced_ctx = super::native_context(&frozen, input);
    request.trace = false;
    let untraced = simulate_dps(&request, &mut untraced_ctx, &AtomicBool::new(false)).unwrap();
    assert!(untraced.events.is_empty(), "no unrequested driver traces");
    assert!(untraced_ctx.action_trace.is_none(), "no unrequested per-hit buffer");
    let mut traced_value = to_value(traced).unwrap();
    let mut untraced_value = to_value(untraced).unwrap();
    // DpsResult serializes events; only this intentionally trace-dependent field is excluded.
    traced_value.as_object_mut().unwrap().shift_remove("events").unwrap();
    untraced_value.as_object_mut().unwrap().shift_remove("events").unwrap();
    crate::native_support::assert_json_eq(&traced_value, &untraced_value, "trace-invariant DPS result");
    crate::native_support::assert_json_eq(
        &crate::native_support::random_value(&traced_ctx.random),
        &crate::native_support::random_value(&untraced_ctx.random),
        "trace-invariant random consumption and continuation");
}

pub fn unreachable_special_hits_do_not_mask_nonprogress() {
    let case = fixture("full7200-single-host11-compiled20240101");
    let mut input = case["input"].clone();
    input["tp_player"]["main_job"] = Value::from("sam");
    let mut main = local_piece(input["tp_player"]["gearset"]["main"].clone(), "Unreachable Zanshin main", "Unreachable Zanshin main");
    main["jobs"] = Value::Array(vec!["sam".into()]);
    main["delay"] = Value::from(-165);
    input["tp_player"]["gearset"]["main"] = main;
    input["tp_player"]["gearset"]["body"] = armor(&input, "Unreachable special-hit stats",
        object([("Zanshin OA2", 100.into()), ("Daken", 100.into()), ("DA", (-1000).into()), ("TA", (-1000).into()), ("QA", (-1000).into())]));
    input["request"]["ws_threshold"] = Value::from(1);
    let frozen = FrozenCatalog::from_input(&input);
    let mut ctx = frozen.context(&input["seeds"]);
    let request = dps_request(&frozen, &input, &ctx, "unreachable special hits");
    let initial = ctx.random.clone();
    match simulate_dps(&request, &mut ctx, &AtomicBool::new(false)).unwrap_err() {
        CalcError::NonProgress { starting_tp, threshold, upper_bound, .. } => {
            assert_eq!(starting_tp, 0.0);
            assert_eq!(threshold, 1.0);
            assert_eq!(upper_bound, 0.0);
        }
        other => panic!("one-handed Zanshin and absent-ammo Daken cannot progress: {other}"),
    }
    assert_eq!(ctx.random, initial, "unreachable special types are excluded without sampling");
}
