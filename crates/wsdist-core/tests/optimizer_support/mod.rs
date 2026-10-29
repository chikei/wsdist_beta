use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};
use wsdist_core::model::*;
use wsdist_core::optimizer::probe::*;
use crate::native_support::{self as native, CountingRandom};

use super::support;
use support::{FrozenCatalog, assert_number, number};

pub fn context(catalog: &FrozenCatalog, seeds: &Value) -> EvaluationContext<CountingRandom> {
    EvaluationContext::new(catalog.catalog.clone(), CountingRandom::seeded(seeds["numpy_host"].as_u64().unwrap()))
}

pub fn raw_action(input: &Value) -> RawAction {
    RawAction { ws_name: Arc::from(input["ws_name"].as_str().unwrap()), spell_name: Arc::from(input["spell_name"].as_str().unwrap()),
        action_type: Arc::from(input["action_type"].as_str().unwrap()), min_tp: number(&input["min_tp"]),
        ws_type: WsType::from_name(input["ws_type"].as_str().unwrap()).unwrap(),
        spell_type: SpellType::from_name(input["spell_type"].as_str().unwrap()).unwrap(),
        metric: Metric::new(input["input_metric"].as_str().unwrap()) }
}

pub fn pools(catalog: &FrozenCatalog, input: &Value) -> CandidatePools {
    std::array::from_fn(|index| input[Slot::ALL[index].name()].as_array().unwrap().iter().map(|piece| catalog.gear(piece)).collect())
}

pub fn legality(catalog: &FrozenCatalog, input: &Value, state: &ModelState, label: &str) -> LegalityRules {
    let player = catalog.player(&input["player"], state, label);
    let ws = input["ws_dict"].as_object().unwrap().iter().map(|(skill, names)|
        (SkillType::from_name(skill).unwrap(), names.as_array().unwrap().iter().map(|name| Arc::from(name.as_str().unwrap())).collect())).collect::<Vec<_>>();
    let restricted = input["restricted_ws"].as_object().unwrap().iter().map(|(ws, weapon)|
        (Arc::from(ws.as_str()), Arc::from(weapon.as_str().unwrap()))).collect::<Vec<_>>();
    let req = &input["request"];
    LegalityRules::new(player.main_job, player.sub_job, req["action_type"].as_str().unwrap(), req["ws_name"].as_str().unwrap(),
        req["spell_name"].as_str().unwrap(), &ws, &restricted)
}

pub fn score_context(catalog: &FrozenCatalog, input: &Value, state: &ModelState, label: &str)
    -> (Arc<ScoreContext>, InternedSet, Vec<Vec<usize>>) {
    let player = catalog.player(&input["player"], state, label);
    let candidates = pools(catalog, &input["candidate_pools"]);
    let pieces = intern_pieces(&catalog.catalog, state, std::iter::once(player.gearset.as_slice()).chain(candidates.iter().map(Vec::as_slice)));
    let fast = input.get("fast_gate").and_then(Value::as_bool).unwrap_or_else(|| can_use_fast_dt_gate(&player.abilities).unwrap());
    let rules = legality(catalog, input, state, label);
    let ctx = Arc::new(ScoreContext::new(player.clone(), support::enemy(&input["enemy"]), raw_action(&input["request"]), rules,
        pieces, &catalog.catalog, fast));
    let ids = ctx.ids(&player.gearset);
    let pools = candidates.iter().map(|pool| pool.iter().map(|&gear| ctx.pieces.id(gear)).collect()).collect();
    (ctx, ids, pools)
}

pub fn raw_request(catalog: &FrozenCatalog, input: &Value, state: &ModelState, label: &str) -> RawOptimizeRequest {
    RawOptimizeRequest { player: catalog.player(&input["player"], state, label), enemy: support::enemy(&input["enemy"]),
        action: raw_action(&input["request"]), candidates: pools(catalog, &input["candidate_pools"]),
        min_pdt: number(&input["thresholds"]["pdt"]), min_mdt: number(&input["thresholds"]["mdt"]),
        options: serde_json::from_value(input["options"].clone()).unwrap(), similar_results: true,
        similar_percent: number(&input["next_best_percent"]), verbose_swaps: input["verbose_swaps"].as_bool().unwrap(), automatic_workers: None }
}

pub fn request(catalog: &FrozenCatalog, input: &Value, state: &ModelState, label: &str) -> OptimizeRequest {
    let raw = raw_request(catalog, input, state, label);
    OptimizeRequest { player: raw.player, enemy: raw.enemy, action: raw.action.resolve().unwrap(),
        ws_name: raw.action.ws_name, spell_name: raw.action.spell_name, candidates: raw.candidates,
        min_pdt: raw.min_pdt, min_mdt: raw.min_mdt, options: raw.options, similar_results: raw.similar_results,
        similar_percent: raw.similar_percent, verbose_swaps: raw.verbose_swaps }
}

pub fn tuple(value: &Value) -> Tuple { Tuple::new(&integers(value)) }
pub fn integers(value: &Value) -> Vec<usize> { value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect() }
pub fn groups(value: &Value) -> Vec<Group> {
    value.as_array().unwrap().iter().map(|group| Group { combo: tuple(&group[0]),
        candidates: group[1].as_array().unwrap().iter().map(tuple).collect() }).collect()
}
pub fn thresholds(value: &Value) -> [f64; 2] { [number(&value["pdt"]), number(&value["mdt"])] }
pub fn floats(value: &Value) -> Vec<f64> { value.as_array().unwrap().iter().map(number).collect() }
pub fn assert_floats(actual: &[f64], expected: &Value, label: &str) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{label}: numeric field count");
    for (index, (&actual, expected)) in actual.iter().zip(expected).enumerate() { assert_number(actual, expected, &format!("{label}[{index}]")); }
}
fn exact<T: serde::Serialize>(actual: &T, expected: &Value, label: &str) {
    assert_eq!(serde_json::to_value(actual).unwrap(), *expected, "{label}: exact order/identity/cursor");
}
fn assert_evaluation(actual: &LegacyEvaluation, expected: &Value, label: &str) {
    assert_number(actual.metric, &expected[0], &format!("{label}.metric"));
    assert_floats(&actual.output, &expected[1], &format!("{label}.output"));
    assert_eq!(actual.invert as i64, expected[2].as_i64().unwrap(), "{label}.invert");
    assert_eq!(actual.decimals as u64, expected[3].as_u64().unwrap(), "{label}.decimals");
    assert_eq!(actual.nondecimals as u64, expected[4].as_u64().unwrap(), "{label}.width");
}

pub fn assert_similar(actual: &[SimilarSwap], expected: &Value, catalog: &FrozenCatalog, label: &str) {
    let total: usize = expected.as_object().unwrap().values().map(|v| v.as_array().unwrap().len()).sum();
    assert_eq!(actual.len(), total, "{label}: retained similar candidates");
    for slot in Slot::ALL {
        let expected = expected.get(slot.name()).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let actual: Vec<_> = actual.iter().filter(|swap| swap.slot == *slot).collect();
        assert_eq!(actual.len(), expected.len(), "{label}.{}: retained order/count", slot.name());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(catalog.catalog.piece(actual.gear).name2.as_ref(), expected[0].as_str().unwrap(), "{label}.{}: similar piece", slot.name());
            assert_number(actual.metric, &expected[1], label);
        }
    }
}

pub fn event_kind(event: &TraceEvent) -> &'static str {
    match event {
        TraceEvent::Neighborhood { .. } => "neighborhood", TraceEvent::Shuffle { .. } => "shuffle",
        TraceEvent::Choice { .. } => "choice", TraceEvent::PoolOrders { .. } => "pool_orders",
        TraceEvent::Take { .. } => "take", TraceEvent::Seek { .. } => "seek", TraceEvent::Legality { .. } => "legality",
        TraceEvent::FastDtRejection { .. } => "fast_dt_rejection", TraceEvent::FullPlayerDt { .. } => "full_player_dt_verification",
        TraceEvent::Metric { .. } => "evaluate_metric", TraceEvent::Rescore { .. } => "rescore_with_output",
        TraceEvent::ScoreChunk { .. } => "score_chunk", TraceEvent::MapperSelection { .. } => "mapper_selection",
        TraceEvent::MapperScores { .. } => "mapper_scores", TraceEvent::PassStart { .. } => "pass_start",
        TraceEvent::Accepted { .. } => "accepted", TraceEvent::PassReturn { .. } => "pass_return",
        TraceEvent::DtStage { .. } => "dt_stage", TraceEvent::DtStageEnd { .. } => "dt_stage_end",
        TraceEvent::UnsatisfiedDt { .. } => "unsatisfied_dt_termination", TraceEvent::RestartComparison { .. } => "restart_comparison",
        TraceEvent::BuildReturn { .. } => "build_return",
    }
}

fn legacy_event_kind(kind: &str) -> &str {
    match kind {
        "numpy_shuffle" => "shuffle",
        "numpy_choice" => "choice",
        kind => kind,
    }
}

pub fn assert_trace(actual: &SearchTrace, evidence: &Value, catalog: &FrozenCatalog, label: &str) {
    let expected: Vec<_> = evidence["ordered_events"].as_array().unwrap().iter().filter(|event| matches!(legacy_event_kind(event["kind"].as_str().unwrap()),
        "neighborhood" | "shuffle" | "choice" | "pool_orders" | "take" | "seek" | "legality" | "fast_dt_rejection" |
        "full_player_dt_verification" | "evaluate_metric" | "rescore_with_output" | "score_chunk" | "mapper_selection" |
        "explicit_mapper" | "mapper_scores" | "pass_start" | "accepted" | "pass_return" | "dt_stage" | "dt_stage_end" |
        "unsatisfied_dt_termination" | "restart_comparison" | "build_return")).collect();
    assert_eq!(actual.events.len(), expected.len(), "{label}: full ordered decision trace");
    for (index, (actual, expected)) in actual.events.iter().zip(expected).enumerate() {
        let path = format!("{label}.events[{index}] {}", event_kind(actual));
        let kind = legacy_event_kind(expected["kind"].as_str().unwrap());
        assert!(event_kind(actual) == kind || (event_kind(actual) == "mapper_selection" && kind == "explicit_mapper"), "{path}: expected {kind}");
        match actual {
            TraceEvent::Neighborhood { combos, pools } => { exact(combos, &expected["combos"], &path); exact(pools, &expected["pools"], &path); }
            TraceEvent::Shuffle { before, after } => { exact(before, &expected["before"], &path); exact(after, &expected["after"], &path); }
            TraceEvent::Choice { pool, selected, state } => {
                catalog.assert_piece(*selected, Some(state), &expected["selected"], &format!("{path}.selected"));
                let source = expected["pool"].as_array().unwrap();
                assert_eq!(pool.len(), source.len(), "{path}: choice pool count/order");
                for (&id, source) in pool.iter().zip(source) { catalog.assert_piece(id, Some(state), source, &format!("{path}.pool")); }
            }
            TraceEvent::PoolOrders { index, combo, orders } => {
                assert_eq!(*index as u64, expected["combo_index"].as_u64().unwrap(), "{path}: combo index"); exact(combo, &expected["combo"], &path); exact(orders, &expected["orders"], &path);
            }
            TraceEvent::Take { base, limit, before, after, groups, total } => {
                exact(base, &expected["base_ids"], &path); assert_eq!(*limit as u64, expected["limit"].as_u64().unwrap(), "{path}: limit");
                exact(before, &expected["cursor_before"], &path); exact(after, &expected["cursor_after"], &path); exact(groups, &expected["groups"], &path);
                assert_eq!(*total as u64, expected["total"].as_u64().unwrap(), "{path}: count");
            }
            TraceEvent::Seek { before, after } => { exact(before, &expected["cursor_before"], &path); exact(after, &expected["cursor_after"], &path); }
            TraceEvent::Legality { valid } => assert_eq!(Some(*valid), expected["valid"].as_bool(), "{path}: legality branch"),
            TraceEvent::FastDtRejection { combo, items, achieved, thresholds } => {
                exact(combo, &expected["combo"], &path); exact(items, &expected["item_ids"], &path); assert_floats(achieved, &expected["achieved"], &path); assert_floats(thresholds, &expected["thresholds"], &path);
            }
            TraceEvent::FullPlayerDt { combo, items, achieved, thresholds, player, state } => {
                exact(combo, &expected["combo"], &path); exact(items, &expected["item_ids"], &path); assert_floats(achieved, &expected["achieved"], &path); assert_floats(thresholds, &expected["thresholds"], &path);
                catalog.assert_player(player, state, &evidence["players"][expected["player"].as_u64().unwrap() as usize], &path);
            }
            TraceEvent::Metric { player, state, evaluation } => {
                catalog.assert_player(player, state, &evidence["players"][expected["player"].as_u64().unwrap() as usize], &path); assert_evaluation(evaluation, &expected["result"], &path);
            }
            TraceEvent::Rescore { ids, evaluation } => { exact(ids, &expected["ids"], &path); assert_evaluation(evaluation, &expected["result"], &path); }
            TraceEvent::ScoreChunk { base, groups, thresholds, metrics } => {
                exact(base, &expected["base_ids"], &path); exact(groups, &expected["groups"], &path); assert_floats(thresholds, &expected["thresholds"], &path); assert_floats(metrics, &expected["metrics"], &path);
            }
            TraceEvent::MapperSelection { workers, candidate_estimate, parallel, width, floor, ceiling } => {
                assert_eq!(*workers as u64, expected["workers"].as_u64().unwrap(), "{path}: worker width");
                if kind == "mapper_selection" { assert_eq!(*candidate_estimate as u64, expected["candidate_estimate"].as_u64().unwrap(), "{path}: candidate estimate"); assert_eq!(*width as u64, expected["width"].as_u64().unwrap(), "{path}: effective width"); }
                assert_eq!(*parallel, expected["mapper"] == "PoolMapper", "{path}: pool threshold decision");
                assert_eq!(*floor as u64, expected["batch_floor"].as_u64().unwrap(), "{path}: batch floor"); assert_eq!(*ceiling as u64, expected["batch_ceiling"].as_u64().unwrap(), "{path}: batch ceiling");
            }
            TraceEvent::MapperScores { base, groups, total, thresholds, metrics, parallel } => {
                exact(base, &expected["base_ids"], &path); exact(groups, &expected["groups"], &path); assert_eq!(*total as u64, expected["total"].as_u64().unwrap(), "{path}: scored total");
                assert_floats(thresholds, &expected["thresholds"], &path); assert_floats(metrics, &expected["metrics"], &path);
                assert_eq!(if *parallel { "pool" } else { "coordinator" }, expected["execution"].as_str().unwrap(), "{path}: local/pool dispatch boundary");
            }
            TraceEvent::PassStart { ids, metric, thresholds } => { exact(ids, &expected["ids"], &path); assert_number(*metric, &expected["metric"], &path); assert_floats(thresholds, &expected["thresholds"], &path); }
            TraceEvent::Accepted { before, before_metric, combo, items, metric, batch, scanned, total, rewind } => {
                exact(before, &expected["before_ids"], &path); assert_number(*before_metric, &expected["before_metric"], &path); exact(combo, &expected["combo"], &path); exact(items, &expected["item_ids"], &path); assert_number(*metric, &expected["metric"], &path);
                assert_eq!(*batch as u64, expected["batch"].as_u64().unwrap(), "{path}: batch growth/shrink"); assert_eq!(*scanned as u64, expected["scanned"].as_u64().unwrap(), "{path}: strict first improvement index");
                assert_eq!(*total as u64, expected["scored_total"].as_u64().unwrap(), "{path}: speculative score count"); exact(rewind, &expected["rewind_position"], &path);
            }
            TraceEvent::PassReturn { ids, metric, output, cursor, similar } => {
                exact(ids, &expected["ids"], &path); assert_number(*metric, &expected["metric"], &path); assert_floats(output, &expected["output"], &path); exact(cursor, &expected["cursor"], &path); assert_similar(similar, &expected["similar_swaps"], catalog, &path);
            }
            TraceEvent::DtStage { thresholds, prior_achieved, ids, unchanged } => {
                assert_floats(thresholds, &expected["thresholds"], &path); assert_floats(prior_achieved, &expected["prior_achieved"], &path); exact(ids, &expected["ids"], &path); assert_eq!(*unchanged as u64, expected["unchanged_stage_count"].as_u64().unwrap(), "{path}: unchanged stages");
            }
            TraceEvent::DtStageEnd { achieved, next_thresholds, unchanged, metric } => {
                assert_floats(achieved, &expected["achieved"], &path); assert_floats(next_thresholds, &expected["next_thresholds"], &path); assert_eq!(*unchanged as u64, expected["unchanged_stage_count"].as_u64().unwrap(), "{path}: unchanged stages"); assert_number(*metric, &expected["metric"], &path);
            }
            TraceEvent::UnsatisfiedDt { achieved, unchanged } => { assert_floats(achieved, &expected["achieved"], &path); assert_eq!(*unchanged as u64, expected["unchanged_stage_count"].as_u64().unwrap(), "{path}: unsuccessful DT stages"); }
            TraceEvent::RestartComparison { restart, metric, incumbent, improved } => {
                assert_eq!(*restart as u64, expected["restart_index"].as_u64().unwrap(), "{path}: restart index"); assert_number(*metric, &expected["metric"], &path); assert_number(*incumbent, &expected["incumbent_metric"], &path); assert_eq!(Some(*improved), expected["strict_improvement"].as_bool(), "{path}: earliest tied winner");
            }
            TraceEvent::BuildReturn { ids, metric, output, invert, achieved_before_reorder, unchanged, starting, filtered, player, state } => {
                exact(ids, &expected["best_ids_before_earring_reorder"], &path); assert_number(*metric, &expected["legacy_metric"], &path); assert_floats(output, &expected["legacy_output"], &path); assert_eq!(*invert as i64, expected["invert"].as_i64().unwrap(), "{path}: invert");
                assert_floats(achieved_before_reorder, &expected["achieved_before_reorder"], &path); assert_eq!(*unchanged as u64, expected["unchanged_stage_count"].as_u64().unwrap(), "{path}: unchanged stages");
                catalog.assert_gearset(starting, state, &expected["starting_gearset_after_legacy_mutations"], &path);
                for slot in Slot::ALL {
                    let source = expected["filtered_candidates"][slot.name()].as_array().unwrap();
                    assert_eq!(filtered[slot.index()].len(), source.len(), "{path}: filtered {} count/order/duplicates", slot.name());
                    for (&id, source) in filtered[slot.index()].iter().zip(source) {
                        catalog.assert_piece(id, Some(state), source, &format!("{path}.filtered.{}", slot.name()));
                    }
                }
                catalog.assert_player(player, state, &evidence["players"][expected["returned_player"].as_u64().unwrap() as usize], &path);
            }
        }
    }
}

pub fn assert_draw_count(actual: &CountingRandom, evidence: &Value, label: &str) {
    let expected = evidence["ordered_events"].as_array().unwrap().iter()
        .filter(|event| event["kind"] == "unused_or_used_action_draw").count();
    assert_eq!(actual.uniforms(), expected, "{label}: unused/used action draw count");
}

pub fn assert_changes(catalog: &FrozenCatalog, before: &ModelState, after: &ModelState, expected: &Value, label: &str) {
    let changed: Vec<_> = catalog.catalog.pieces().iter().filter(|piece| before.skill(piece) != after.skill(piece)).collect();
    let expected = expected.as_array().unwrap();
    assert_eq!(changed.len(), expected.len(), "{label}: identity-local metadata changes");
    for (piece, expected) in changed.iter().zip(expected) {
        assert_eq!(piece.original_id as u64, expected["catalog_id"].as_u64().unwrap(), "{label}: mutated catalog identity");
        assert_eq!(before.skill(piece).name(), expected["before"]["skill_type"].as_str().unwrap(), "{label}: prior skill");
        assert_eq!(after.skill(piece).name(), expected["after"]["skill_type"].as_str().unwrap(), "{label}: effective skill");
    }
}

pub fn mapper(input: &Value, evidence: &Value, ctx: Arc<ScoreContext>, ids: InternedSet,
    trace: &mut Option<SearchTrace>) -> Mapper<CountingRandom> {
    let spec = &input["mapper"];
    let selection = evidence["ordered_events"].as_array().unwrap().iter()
        .find(|event| event["kind"] == "explicit_mapper" || event["kind"] == "mapper_selection").unwrap();
    let workers = selection["workers"].as_u64().unwrap() as usize;
    let config = MapperConfig { workers, candidate_estimate: spec.get("candidate_estimate").and_then(Value::as_u64).unwrap_or(0) as usize,
        force_pool: spec.get("force_pool").and_then(Value::as_bool).unwrap_or(false),
        batch_floor: spec.get("batch_floor").and_then(Value::as_u64).unwrap_or(8) as usize,
        batch_ceiling: spec.get("batch_ceiling").and_then(Value::as_u64).unwrap_or(64) as usize };
    Mapper::new(ctx, ids, config, trace)
}

pub fn product_groups(slots: &[usize], pools: &[Vec<usize>]) -> Vec<Group> {
    let mut tuples = vec![Vec::new()];
    for &slot in slots {
        tuples = tuples.into_iter().flat_map(|prefix| pools[slot].iter().map(move |&id| { let mut ids = prefix.clone(); ids.push(id); ids })).collect();
    }
    vec![Group { combo: Tuple::new(slots), candidates: tuples.into_iter().map(|ids| Tuple::new(&ids)).collect() }]
}

pub fn assert_options(actual: &OptimizeOptions, expected: &Value, label: &str) {
    assert_eq!(actual.iterations, expected["iterations"].as_i64().unwrap(), "{label}: pass limit");
    assert_eq!(actual.max_swap_slots, expected["max_swap_slots"].as_i64().unwrap(), "{label}: swap limit");
    assert_eq!(actual.restart_count, expected["restart_count"].as_i64().unwrap(), "{label}: restart count");
    assert_eq!(actual.seed.map(u64::from), expected["seed"].as_u64(), "{label}: seed");
    assert_number(actual.dt_step, &expected["dt_step"], label);
    assert_eq!(actual.workers, expected["workers"].as_i64().unwrap(), "{label}: requested workers");
}

fn assert_outcome<T>(actual: &Result<T, CalcError>, expected: &Value, label: &str) {
    assert_eq!(actual.is_ok(), expected["status"] == "returned", "{label}: captured return/error status");
    if let Err(error) = actual { support::assert_error(error, &expected["error"], label); }
}

fn error_value(error: &CalcError) -> Value {
    match error {
        CalcError::Data { path, json_path, message } =>
            json!({"kind": "data", "path": path, "json_path": json_path, "message": message}),
        CalcError::Legacy { operation, field, category, message } =>
            json!({"kind": "legacy", "operation": operation, "field": field, "category": category, "message": message}),
        CalcError::Input { operation, field, message } =>
            json!({"kind": "input", "operation": operation, "field": field, "message": message}),
        CalcError::Interrupted { operation } => json!({"kind": "interrupted", "operation": operation}),
        CalcError::NonProgress { operation, starting_tp, threshold, upper_bound } => json!({
            "kind": "nonprogress", "operation": operation, "starting_tp": native::num(*starting_tp),
            "threshold": native::num(*threshold), "upper_bound": native::num(*upper_bound),
        }),
    }
}

fn accepted_value(accepted: &[AcceptedCandidate], catalog: &FrozenCatalog) -> Value {
    Value::Array(accepted.iter().map(|candidate| json!({
        "gearset": native::gearset_value(catalog, &candidate.gearset),
        "metric": native::num(candidate.metric), "legacy_output": native::nums(&candidate.legacy_output),
        "pdt": native::num(candidate.pdt), "mdt": native::num(candidate.mdt),
    })).collect())
}

pub struct NeighborhoodRun {
    pub combos: Vec<Tuple>,
    pub ordered_candidates: Vec<Value>,
    pub exhausted_cursor: Cursor,
    pub replay_count: usize,
    pub replay_start: Cursor,
    pub replayed_tail: Vec<Group>,
    pub rewind_prefix: Vec<Group>,
    pub rewind_prefix_count: usize,
    pub trace: SearchTrace,
    pub random: CountingRandom,
}

pub fn run_neighborhood(case: &Value, _catalog: &FrozenCatalog) -> NeighborhoodRun {
    let input = &case["input"];
    let combos = if input["combos"].is_null() {
        slot_combos(input["pools"].as_array().unwrap().len(), input["max_swap_slots"].as_u64().unwrap() as usize)
    } else { input["combos"].as_array().unwrap().iter().map(tuple).collect() };
    let pools: Vec<_> = input["pools"].as_array().unwrap().iter().map(integers).collect();
    let base = integers(&input["base_ids"]);
    let mut random = CountingRandom::seeded(input["seeds"]["numpy_host"].as_u64().unwrap());
    let mut trace = Some(SearchTrace::default());
    let mut neighborhood = Neighborhood::new(combos.clone(), pools, &mut trace);
    let mut ordered_candidates = Vec::new();
    while !neighborhood.exhausted() {
        let (groups, count) = neighborhood.take(&base, input["batch"].as_u64().unwrap() as usize, &mut random, &mut trace);
        for group in groups {
            for ids in group.candidates { ordered_candidates.push(json!([group.combo, ids])); }
        }
        if count == 0 { break; }
    }
    let exhausted_cursor = neighborhood.position();
    neighborhood.seek(Cursor { combo_index: 0, odometer: Vec::new(), entered: false }, &mut trace);
    let (rewind_prefix, rewind_prefix_count) = neighborhood.take(&base,
        input["rewind_after"].as_u64().unwrap() as usize, &mut random, &mut trace);
    let replay_start = neighborhood.position();
    let (replayed_tail, replay_count) = neighborhood.take(&base, 100000, &mut random, &mut trace);
    NeighborhoodRun { combos, ordered_candidates, exhausted_cursor, replay_count, replay_start, replayed_tail,
        rewind_prefix, rewind_prefix_count, trace: trace.unwrap(), random }
}

pub fn assert_neighborhood_legacy(case: &Value, run: &NeighborhoodRun, _catalog: &FrozenCatalog) {
    assert_eq!(json!(run.combos), case["expected"]["value"]["combos"], "{}: combo walk", case["id"]);
}

pub fn project_neighborhood(run: &NeighborhoodRun, catalog: &FrozenCatalog) -> Value {
    json!({
        "result": {
            "ordered_candidates": run.ordered_candidates, "exhausted_cursor": run.exhausted_cursor,
            "replay_count": run.replay_count, "replay_start": run.replay_start, "replayed_tail": run.replayed_tail,
            "rewind_prefix": run.rewind_prefix, "rewind_prefix_count": run.rewind_prefix_count,
        },
        "trace": native::trace_value(&run.trace, catalog), "random": native::random_value(&run.random),
    })
}

pub struct PassRun {
    pub initial_ids: InternedSet,
    pub result: Result<PassResult, CalcError>,
    pub before: ModelState,
    pub state: ModelState,
    pub trace: SearchTrace,
    pub random: CountingRandom,
}

fn run_pass_in_context(input: &Value, expected: &Value, catalog: &FrozenCatalog,
    engine: &mut EvaluationContext<CountingRandom>, baseline: Option<&ModelState>, label: &str) -> PassRun {
    let before = baseline.cloned().unwrap_or_else(|| engine.state.clone());
    let mut trace = Some(SearchTrace::default());
    let (ctx, ids, pools) = score_context(catalog, input, &engine.state, label);
    let mut mapper = mapper(input, &expected["evidence"], ctx.clone(), ids, &mut trace);
    let swappable = pools.iter().enumerate().filter_map(|(index, pool)| (!pool.is_empty()).then_some(index)).collect::<Vec<_>>();
    let combos = slot_combos(swappable.len(), input["max_swap_slots"].as_u64().unwrap() as usize).iter().map(|combo|
        Tuple::new(&combo.as_slice().iter().map(|&axis| swappable[axis]).collect::<Vec<_>>())).collect();
    let mut neighborhood = Neighborhood::new(combos, pools, &mut trace);
    let result = optimize_pass(&ctx, &mut mapper, &mut neighborhood, PassInput {
        ids, metric: number(&input["initial_metric"]), output: floats(&input["initial_output"]), invert: 1,
        next_best_percent: number(&input["next_best_percent"]), thresholds: thresholds(&input["thresholds"]),
        stage: 1, pass: 1, scored_before: 0, verbose_swaps: input["verbose_swaps"].as_bool().unwrap(),
    }, engine, &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut |_| {}, &mut trace);
    PassRun { initial_ids: ids, result, before, state: engine.state.clone(), trace: trace.unwrap(), random: engine.random.clone() }
}

pub fn run_pass(case: &Value, catalog: &FrozenCatalog) -> PassRun {
    let input = &case["input"];
    let mut engine = context(catalog, &input["seeds"]);
    run_pass_in_context(input, &case["expected"]["value"], catalog, &mut engine, None, case["id"].as_str().unwrap())
}

fn assert_pass_observation(expected: &Value, run: &PassRun, catalog: &FrozenCatalog, label: &str) {
    assert_outcome(&run.result, expected, label);
    if run.result.is_ok() {
        assert_eq!(json!(run.initial_ids), expected["result"]["initial_ids"], "{label}: initial interned ids");
    }
    assert_changes(catalog, &run.before, &run.state, &expected["catalog_metadata_changes"], label);
}

pub fn assert_pass_legacy(case: &Value, run: &PassRun, catalog: &FrozenCatalog) {
    assert_pass_observation(&case["expected"]["value"], run, catalog, case["id"].as_str().unwrap());
}

pub fn project_pass(run: &PassRun, catalog: &FrozenCatalog) -> Value {
    let mut observation = json!({"trace": native::trace_value(&run.trace, catalog), "random": native::random_value(&run.random)});
    match &run.result {
        Ok(result) => observation["result"] = json!({
            "ids": result.ids, "metric": native::num(result.metric), "output": native::nums(&result.output),
            "accepted": accepted_value(&result.accepted, catalog), "similar": native::similar_value(catalog, &result.similar),
            "scored": result.scored,
        }),
        Err(error) => { observation["result"] = Value::Null; observation["error"] = error_value(error); }
    }
    observation
}

pub struct BuildRun {
    pub engine: EvaluationContext<CountingRandom>,
    pub before: ModelState,
    pub request: RawOptimizeRequest,
    pub trace: SearchTrace,
    pub result: Result<OptimizeResult, CalcError>,
}

pub fn run_build(case: &Value, catalog: &FrozenCatalog) -> BuildRun {
    let input = &case["input"];
    let mut engine = context(catalog, &input["seeds"]);
    let before = engine.state.clone();
    let mut trace = Some(SearchTrace::default());
    let mut request = raw_request(catalog, input, &engine.state, case["id"].as_str().unwrap());
    if request.options.workers == 0 {
        request.automatic_workers = Some(case["expected"]["value"]["evidence"]["ordered_events"].as_array().unwrap().iter()
            .find(|event| event["kind"] == "resolved_workers").unwrap()["resolved"].as_u64().unwrap() as usize);
    }
    let result = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut trace);
    BuildRun { engine, before, request, trace: trace.unwrap(), result }
}

pub fn assert_build_legacy(case: &Value, run: &BuildRun, catalog: &FrozenCatalog) {
    let label = case["id"].as_str().unwrap();
    let expected = &case["expected"]["value"];
    assert_options(&run.request.options, &case["input"]["options"], label);
    assert_outcome(&run.result, expected, label);
    assert_changes(catalog, &run.before, &run.engine.state, &expected["catalog_metadata_changes"], label);
    if let Ok(result) = &run.result {
        assert_eq!(result.restarts as i64, normalize_options(Some(&run.request.options)).restart_count, "{label}: completed restarts");
        assert_eq!(result.stages, run.trace.events.iter().filter(|event| matches!(event, TraceEvent::DtStage { .. })).count(), "{label}: completed stages");
        assert_eq!(result.passes, run.trace.events.iter().filter(|event| matches!(event, TraceEvent::PassStart { .. })).count(), "{label}: completed passes");
        let scored: usize = run.trace.events.iter().filter_map(|event| match event {
            TraceEvent::MapperScores { total, .. } => Some(*total), _ => None,
        }).sum();
        assert_eq!(result.scored, scored, "{label}: ordered scored candidate count");
    }
}

pub fn project_build_result(result: &OptimizeResult, catalog: &FrozenCatalog) -> Value {
    json!({
        "gearset": native::gearset_value(catalog, &result.gearset),
        "player": result.player.as_ref().map(|player| native::player_value(catalog, player)),
        "metric": native::num(result.metric), "legacy_output": native::nums(&result.legacy_output),
        "pdt": native::num(result.pdt), "mdt": native::num(result.mdt), "feasible": result.feasible,
        "termination": result.termination, "accepted": accepted_value(&result.accepted, catalog),
        "similar": native::similar_value(catalog, &result.similar), "stages": result.stages,
        "passes": result.passes, "scored": result.scored, "restarts": result.restarts, "invert": result.invert,
        "warnings": result.warnings,
    })
}

pub fn project_build(run: &BuildRun, catalog: &FrozenCatalog) -> Value {
    let mut observation = json!({"trace": native::trace_value(&run.trace, catalog), "random": native::random_value(&run.engine.random)});
    match &run.result {
        Ok(result) => observation["result"] = project_build_result(result, catalog),
        Err(error) => { observation["result"] = Value::Null; observation["error"] = error_value(error); }
    }
    observation
}

pub struct SequenceRun {
    pub steps: Vec<PassRun>,
    pub before: ModelState,
    pub state: ModelState,
    pub isolated: ModelState,
}

pub fn run_sequence(case: &Value, catalog: &FrozenCatalog) -> SequenceRun {
    let steps = case["input"]["steps"].as_array().unwrap();
    let mut engine = context(catalog, &steps[0]["seeds"]);
    let before = engine.state.clone();
    let mut observations = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        engine.random = CountingRandom::seeded(step["seeds"]["numpy_host"].as_u64().unwrap());
        observations.push(run_pass_in_context(step, &case["expected"]["value"]["steps"][index], catalog, &mut engine,
            Some(&before), &format!("{}.step[{index}]", case["id"].as_str().unwrap())));
    }
    let isolated = context(catalog, &steps[0]["seeds"]).state;
    SequenceRun { steps: observations, before, state: engine.state, isolated }
}

pub fn assert_sequence_legacy(case: &Value, run: &SequenceRun, catalog: &FrozenCatalog) {
    let label = case["id"].as_str().unwrap();
    let expected = &case["expected"]["value"];
    assert_eq!(run.steps.len(), expected["steps"].as_array().unwrap().len(), "{label}: complete shared-state sequence");
    for (index, step) in run.steps.iter().enumerate() {
        assert_pass_observation(&expected["steps"][index], step, catalog, &format!("{label}.step[{index}]"));
    }
    assert_changes(catalog, &run.before, &run.state, &expected["catalog_metadata_changes"], label);
    let empty = catalog.catalog.piece(catalog.catalog.by_symbol("Empty").unwrap());
    assert_eq!(run.state.skill(empty), SkillType::HandToHand, "{label}: sequential shared Empty override");
    assert_eq!(run.isolated.skill(empty), SkillType::None, "{label}: independent session override boundary");
}

pub fn project_sequence(run: &SequenceRun, catalog: &FrozenCatalog) -> Value {
    json!({"steps": run.steps.iter().map(|step| project_pass(step, catalog)).collect::<Vec<_>>()})
}
