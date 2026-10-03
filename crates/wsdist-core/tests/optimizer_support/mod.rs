use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};
use wsdist_core::model::*;
use wsdist_core::optimizer::probe::*;
use wsdist_core::rng::RandomSources;

use super::support;
use support::{FrozenCatalog, assert_number, number};

#[derive(Clone, Debug)]
pub struct Draw { stream: RandomStream, low: f64, high: f64, value: f64 }
#[derive(Clone, Debug)]
pub struct RecordedRandom { pub random: RandomSources, pub draws: Vec<Draw> }
impl RecordedRandom {
    pub fn seeded(seeds: &Value) -> Self {
        Self { random: RandomSources::from_seeds(seeds["numpy_host"].as_u64().unwrap() as u32,
            seeds["numba_python"].as_u64().unwrap() as u32), draws: Vec::new() }
    }
}
impl RandomSource for RecordedRandom {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        let value = self.random.uniform(stream, low, high);
        self.draws.push(Draw { stream, low, high, value });
        value
    }
    fn bounded_numpy(&mut self, upper: usize) -> usize { self.random.bounded_numpy(upper) }
    fn reseed_numpy(&mut self, seed: u32) { self.random.reseed_numpy(seed); }
}

pub fn context(catalog: &FrozenCatalog, seeds: &Value) -> EvaluationContext<RecordedRandom> {
    EvaluationContext::new(catalog.catalog.clone(), RecordedRandom::seeded(seeds))
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

fn event_kind(event: &TraceEvent) -> &'static str {
    match event {
        TraceEvent::Neighborhood { .. } => "neighborhood", TraceEvent::Shuffle { .. } => "numpy_shuffle",
        TraceEvent::Choice { .. } => "numpy_choice", TraceEvent::PoolOrders { .. } => "pool_orders",
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

pub fn assert_trace(actual: &SearchTrace, evidence: &Value, catalog: &FrozenCatalog, label: &str) {
    let expected: Vec<_> = evidence["ordered_events"].as_array().unwrap().iter().filter(|event| matches!(event["kind"].as_str().unwrap(),
        "neighborhood" | "numpy_shuffle" | "numpy_choice" | "pool_orders" | "take" | "seek" | "legality" | "fast_dt_rejection" |
        "full_player_dt_verification" | "evaluate_metric" | "rescore_with_output" | "score_chunk" | "mapper_selection" |
        "explicit_mapper" | "mapper_scores" | "pass_start" | "accepted" | "pass_return" | "dt_stage" | "dt_stage_end" |
        "unsatisfied_dt_termination" | "restart_comparison" | "build_return")).collect();
    assert_eq!(actual.events.len(), expected.len(), "{label}: full ordered decision trace");
    for (index, (actual, expected)) in actual.events.iter().zip(expected).enumerate() {
        let path = format!("{label}.events[{index}] {}", event_kind(actual));
        let kind = expected["kind"].as_str().unwrap();
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

pub fn assert_draws(actual: &RecordedRandom, evidence: &Value, label: &str) {
    let expected: Vec<_> = evidence["ordered_events"].as_array().unwrap().iter().filter(|event| event["kind"] == "unused_or_used_action_draw").map(|event| &event["draw"]).collect();
    assert_eq!(actual.draws.len(), expected.len(), "{label}: unused/used action draw count");
    for (index, (draw, expected)) in actual.draws.iter().zip(expected).enumerate() {
        let path = format!("{label}.draw[{index}]");
        assert_eq!(match draw.stream { RandomStream::NumpyHost => "NumpyHost", RandomStream::NumbaPython => "NumbaPython" }, expected[0].as_str().unwrap(), "{path}: random stream");
        assert_eq!(expected[1], "uniform", "{path}: random operation");
        let parameters = expected[2].as_array().unwrap();
        let (low, high) = if parameters.is_empty() { (0.0, 1.0) } else { (number(&parameters[0]), number(&parameters[1])) };
        assert_eq!(draw.low.to_bits(), low.to_bits(), "{path}: interval lower"); assert_eq!(draw.high.to_bits(), high.to_bits(), "{path}: interval upper");
        support::observation::record(draw.value, number(&expected[4]), true, &path);
        assert_eq!(draw.value.to_bits(), number(&expected[4]).to_bits(), "{path}: exact random variate");
    }
    support::assert_numpy_state(&actual.random, &evidence["numpy_host_final_state"], label);
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

fn restore_host(random: &mut RandomSources, seed: u32, snapshot: &Value, max_words: usize, label: &str) {
    random.reseed_numpy(seed);
    if support::numpy_state_matches(random, snapshot) { return; }
    let position = snapshot["position"].as_u64().unwrap() as usize;
    for _ in 0..position { random.next_u32(RandomStream::NumpyHost); }
    for _ in 0..=max_words.div_ceil(624) {
        if support::numpy_state_matches(random, snapshot) { return; }
        for _ in 0..624 { random.next_u32(RandomStream::NumpyHost); }
    }
    panic!("{label}: captured worker state is unreachable from recorded seed/draw bound");
}

pub fn mapper(catalog: &FrozenCatalog, input: &Value, evidence: &Value, ctx: Arc<ScoreContext>, ids: InternedSet,
    engine: &EvaluationContext<RecordedRandom>, trace: &mut Option<SearchTrace>, label: &str) -> (Mapper<RecordedRandom>, Vec<Value>) {
    let spec = &input["mapper"];
    let events = evidence["ordered_events"].as_array().unwrap();
    let selection = events.iter().find(|event| event["kind"] == "explicit_mapper" || event["kind"] == "mapper_selection").unwrap();
    let workers = selection["workers"].as_u64().unwrap() as usize;
    let config = MapperConfig { workers, candidate_estimate: spec.get("candidate_estimate").and_then(Value::as_u64).unwrap_or(0) as usize,
        force_pool: spec.get("force_pool").and_then(Value::as_bool).unwrap_or(false),
        batch_floor: spec.get("batch_floor").and_then(Value::as_u64).unwrap_or(8) as usize,
        batch_ceiling: spec.get("batch_ceiling").and_then(Value::as_u64).unwrap_or(64) as usize };
    let mut mapper = Mapper::new(ctx.clone(), ids, config, trace);
    let expected_jobs: Vec<_> = events.iter().filter(|event| event["kind"] == "worker_logical_job").cloned().collect();
    if !expected_jobs.is_empty() {
        let max_words = expected_jobs.iter().map(|job| {
            job["evidence"]["ordered_events"].as_array().unwrap().iter().filter(|event| event["kind"] == "unused_or_used_action_draw").count()
                + job["warmup"]["ordered_events"].as_array().unwrap().iter().filter(|event| event["kind"] == "unused_or_used_action_draw").count()
        }).sum::<usize>() * 2;
        let mut jobs = Vec::new();
        for (index, expected) in expected_jobs.iter().enumerate() {
            let path = format!("{label}.worker[{index}]");
            let mut worker = context(catalog, &expected["seeds"]);
            worker.state = engine.state.clone();
            let mut scratch = ScoreScratch::new(&ctx);
            let mut warmup = Some(SearchTrace::default());
            let candidates = vec![Group { combo: Tuple::new(&[0]), candidates: vec![Tuple::new(&[ids[0]])] }];
            scratch.score(&ctx, &ids, &candidates, [f64::INFINITY; 2], &mut worker, &AtomicBool::new(false), &mut warmup).unwrap();
            assert_trace(warmup.as_ref().unwrap(), &expected["warmup"], catalog, &path);
            assert_draws(&worker.random, &expected["warmup"], &path);
            restore_host(&mut worker.random.random, expected["seeds"]["numpy_host"].as_u64().unwrap() as u32,
                &expected["numpy_host_initial_state"], max_words, &path);
            worker.random.draws.clear();
            jobs.push(WorkerInput { engine: worker });
        }
        mapper.replay_workers(jobs);
    }
    (mapper, expected_jobs)
}

pub fn assert_workers(mapper: &Mapper<RecordedRandom>, expected: &[Value], catalog: &FrozenCatalog, label: &str) {
    assert_eq!(mapper.unconsumed_workers(), 0, "{label}: all captured logical jobs consumed");
    assert_eq!(mapper.completed_workers().len(), expected.len(), "{label}: ordered worker shares");
    for (index, (actual, expected)) in mapper.completed_workers().iter().zip(expected).enumerate() {
        let path = format!("{label}.worker[{index}]");
        assert_floats(&actual.metrics, &expected["metrics"], &path);
        assert_trace(actual.trace.as_ref().unwrap(), &expected["evidence"], catalog, &path);
        assert_draws(&actual.engine.random, &expected["evidence"], &path);
    }
}

pub fn assert_build(actual: &OptimizeResult, expected: &Value, catalog: &FrozenCatalog, state: &ModelState, label: &str) {
    let result = &expected["result"];
    if result["player"].is_null() { assert!(actual.player.is_none(), "{label}: no fabricated restart winner"); }
    else { catalog.assert_player(actual.player.as_ref().unwrap(), state, &result["player"], label); }
    assert_floats(&actual.legacy_output, &result["legacy_output"], label);
    if !result["achieved_dt_from_original_helper"].is_null() { assert_floats(&[actual.pdt, actual.mdt], &result["achieved_dt_from_original_helper"], label); }
    assert_eq!(Some(actual.feasible), result["requirements_satisfied"].as_bool(), "{label}: feasibility metadata");
    assert_eq!(actual.accepted.len() as u64, result["observed_accepted_candidates"].as_u64().unwrap(), "{label}: accepted candidate count");
}

pub fn product_groups(slots: &[usize], pools: &[Vec<usize>]) -> Vec<Group> {
    let mut tuples = vec![Vec::new()];
    for &slot in slots {
        tuples = tuples.into_iter().flat_map(|prefix| pools[slot].iter().map(move |&id| { let mut ids = prefix.clone(); ids.push(id); ids })).collect();
    }
    vec![Group { combo: Tuple::new(slots), candidates: tuples.into_iter().map(|ids| Tuple::new(&ids)).collect() }]
}

pub fn orders_value(orders: &[Option<Vec<Vec<usize>>>]) -> Value {
    let mut result = serde_json::Map::new();
    for (index, orders) in orders.iter().enumerate() { if let Some(orders) = orders { result.insert(index.to_string(), json!(orders)); } }
    Value::Object(result)
}
