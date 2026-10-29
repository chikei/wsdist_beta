use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::actions::optimizer_spell_type;
use crate::model::*;

use super::helpers::*;
use super::legality::{LegalityRules, optimizer_ws_type};
use super::neighborhood::Neighborhood;
use super::probe::{RawAction, SearchTrace, TraceEvent, record, state_snapshot};
use super::scoring::{Mapper, MapperConfig, ScoreContext, ScoreScratch, capture_calculation, interrupted};

#[derive(Clone, Debug)]
pub struct PassInput {
    pub ids: InternedSet, pub metric: f64, pub output: Vec<f64>, pub invert: i8,
    pub next_best_percent: f64, pub thresholds: [f64; 2], pub stage: usize, pub pass: usize,
    pub verbose_swaps: bool, pub scored_before: usize,
}

#[derive(Clone, Debug)]
pub struct PassResult {
    pub ids: InternedSet, pub metric: f64, pub output: Vec<f64>,
    pub accepted: Vec<AcceptedCandidate>, pub similar: Vec<SimilarSwap>, pub scored: usize,
}

fn modulo(first: f64, second: f64) -> f64 {
    let remainder = first % second;
    if remainder != 0.0 && remainder.is_sign_negative() != second.is_sign_negative() { remainder + second } else { remainder }
}

pub fn optimize_pass<R: RandomSource>(context: &Arc<ScoreContext>, mapper: &mut Mapper<R>, neighborhood: &mut Neighborhood,
    input: PassInput, engine: &mut EvaluationContext<R>, scratch: &mut ScoreScratch, stop: &AtomicBool,
    progress: &mut dyn FnMut(Progress), trace: &mut Option<SearchTrace>) -> Result<PassResult, CalcError> {
    let mut best_ids = input.ids;
    let mut best_metric = input.metric;
    let mut output = input.output;
    let mut accepted_candidates = Vec::new();
    let mut similar = Vec::new();
    let mut scored = 0;
    let mut batch = mapper.batch_floor;
    let mut best_dt = dt_for_player(&scratch.player);
    record(trace, || TraceEvent::PassStart { ids: best_ids, metric: best_metric, thresholds: input.thresholds });
    while !neighborhood.exhausted() {
        interrupted(stop)?;
        let start = neighborhood.position();
        let (groups, total) = neighborhood.take(&best_ids, batch, &mut engine.random, trace);
        if total == 0 { break; }
        let metrics = mapper.score(&best_ids, &groups, total, input.thresholds, engine, scratch, stop, trace)?;
        scored += total;
        let mut scanned = 0;
        let mut accepted = None;
        'scan: for group in &groups {
            for &items in &group.candidates {
                let metric = metrics[scanned];
                scanned += 1;
                if metric.is_nan() { continue; }
                if metric > best_metric { accepted = Some((group.combo, items, metric)); break 'scan; }
                if group.combo.len() == 1 {
                    let slot = Slot::ALL[group.combo.as_slice()[0]];
                    if !matches!(slot, Slot::Main | Slot::Sub | Slot::Ranged | Slot::Back)
                        && metric != 0.0 && best_metric != 0.0
                        && modulo(best_metric, metric) / best_metric < input.next_best_percent / 100.0 {
                        let displayed_metric = metric.powi(i32::from(input.invert));
                        similar.push(SimilarSwap { slot, gear: context.pieces.pieces[items.as_slice()[0]], metric: displayed_metric,
                            difference_percent: modulo(best_metric, metric) / best_metric * 100.0 });
                    }
                }
            }
        }
        let mut message = None;
        if let Some((combo, items, metric)) = accepted {
            record(trace, || TraceEvent::Accepted { before: best_ids, before_metric: best_metric, combo, items, metric,
                batch, scanned, total, rewind: start.clone() });
            if input.verbose_swaps {
                let mut where_slots = String::new(); let mut before = String::new(); let mut after = String::new();
                for (axis, (&slot, &id)) in combo.as_slice().iter().zip(items.as_slice()).enumerate() {
                    if axis > 0 { where_slots.push_str(" & "); before.push_str(" & "); after.push_str(" & "); }
                    where_slots.push_str(Slot::ALL[slot].name());
                    before.push_str(&engine.catalog.piece(context.pieces.pieces[best_ids[slot]]).name2);
                    after.push_str(&engine.catalog.piece(context.pieces.pieces[id]).name2);
                }
                let decimals = if context.action.action_type.as_ref() == "attack round" { 3 } else { 1 };
                message = Some(format!("[{where_slots:<15}]: [{before}] -> [{after}] [{:>8.decimals$} -> {:>8.decimals$}]",
                    best_metric.powi(i32::from(input.invert)), metric.powi(i32::from(input.invert))));
            }
            neighborhood.seek(start, trace);
            neighborhood.take(&best_ids, scanned, &mut engine.random, trace);
            for (&slot, &id) in combo.as_slice().iter().zip(items.as_slice()) { best_ids[slot] = id; }
            best_metric = metric;
            let evaluation = scratch.with_output(context, &best_ids, engine, trace)?;
            if engine.calculation_trace.is_some() {
                scratch.calculation.push(CalculationStep {
                    label: "Accepted improvement",
                    equation: None,
                    formula: "DT stage {stage}, pass {pass}: a metric-only candidate evaluation selected this set with score {selection_score}. The separate evaluation supplying the retained damage/TP has score {output_score}.",
                    values: vec![("stage", input.stage as f64), ("pass", input.pass as f64),
                        ("selection_score", metric), ("output_score", evaluation.metric)],
                });
            }
            output = evaluation.output;
            let [pdt, mdt] = dt_for_player(&scratch.player);
            best_dt = [pdt, mdt];
            accepted_candidates.push(AcceptedCandidate { gearset: scratch.player.gearset, metric, legacy_output: output.clone(), pdt, mdt });
            batch = mapper.batch_floor.max(batch / 4);
        } else { batch = mapper.batch_ceiling.min(batch.saturating_mul(4)); }
        progress(Progress { stage: input.stage, pass: input.pass, scored: input.scored_before + scored, best_metric,
            pdt: best_dt[0], mdt: best_dt[1], message });
        interrupted(stop)?;
    }
    // Python retains slot-bucket ordering, even when the traversal interleaves slots.
    similar.sort_by_key(|swap| swap.slot.index());
    for swap in &mut similar {
        let score = swap.metric.powi(i32::from(input.invert));
        swap.difference_percent = modulo(best_metric, score) / best_metric * 100.0;
    }
    record(trace, || TraceEvent::PassReturn { ids: best_ids, metric: best_metric, output: output.clone(),
        cursor: neighborhood.position(), similar: similar.clone() });
    interrupted(stop)?;
    Ok(PassResult { ids: best_ids, metric: best_metric, output, accepted: accepted_candidates, similar, scored })
}

#[derive(Clone, Debug)]
pub struct RawOptimizeRequest {
    pub player: PlayerInput, pub enemy: Enemy, pub action: RawAction, pub candidates: CandidatePools,
    pub min_pdt: f64, pub min_mdt: f64, pub options: OptimizeOptions,
    pub similar_results: bool, pub similar_percent: f64, pub verbose_swaps: bool,
    /// Captured runtime CPU width for auto-worker probe replay only.
    pub automatic_workers: Option<usize>,
}

pub fn optimize_raw<R: RandomSource>(request: &RawOptimizeRequest, engine: &mut EvaluationContext<R>, stop: &AtomicBool,
    progress: &mut dyn FnMut(Progress), trace: &mut Option<SearchTrace>) -> Result<OptimizeResult, CalcError> {
    interrupted(stop)?;
    let options = normalize_options(Some(&request.options));
    // The recursive source reseeds once before dispatching its individually seeded starts.
    if let Some(seed) = options.seed { engine.random.reseed(u64::from(seed)); }
    if options.restart_count > 1 {
        let mut best = None;
        let mut best_metric = 0.0001;
        let mut stages = 0; let mut passes = 0; let mut scored = 0;
        let mut all_accepted = Vec::new(); let mut warnings = Vec::new();
        for index in 0..options.restart_count as usize {
            interrupted(stop)?;
            let single = restart_options(&options, index)?;
            if let Some(seed) = single.seed { engine.random.reseed(u64::from(seed)); }
            let mut result = single_start(request, &single, engine, stop, progress, trace)?;
            stages += result.stages; passes += result.passes; scored += result.scored;
            all_accepted.extend_from_slice(&result.accepted);
            warnings.extend_from_slice(&result.warnings);
            let player = result.player.as_ref().expect("single start returns legacy final player");
            let mut action = request.action.clone();
            action.ws_type = optimizer_ws_type(&action.ws_name);
            action.spell_type = optimizer_spell_type(&action.spell_name);
            let context = post_reorder_context(request, action, engine)?;
            let (evaluation, mut calculation) = capture_calculation(engine, |engine| context.evaluate(player, engine, trace))?;
            interrupted(stop)?;
            let improved = evaluation.metric > best_metric;
            record(trace, || TraceEvent::RestartComparison { restart: index, metric: evaluation.metric, incumbent: best_metric, improved });
            if improved {
                if engine.calculation_trace.is_some() {
                    result.calculation.push(CalculationStep {
                        label: "Post-reorder restart selection",
                        equation: None,
                        formula: "The following calculations are the existing post-reorder comparison for restart {restart}, selecting it with score {selection_score}. They do not replace the retained damage/TP above; its single-start search score was {single_start_score}.",
                        values: vec![("restart", (index + 1) as f64), ("selection_score", evaluation.metric),
                            ("single_start_score", result.metric)],
                    });
                    result.calculation.append(&mut calculation);
                    result.calculation.push(CalculationStep {
                        label: "Restart winner score",
                        equation: None,
                        formula: "Reported optimizer score = {score}, from this restart's post-reorder comparison; reported damage/TP remain the earlier retained legacy output.",
                        values: vec![("score", evaluation.metric)],
                    });
                }
                best_metric = evaluation.metric;
                best = Some(result);
            }
        }
        interrupted(stop)?;
        if let Some(mut best) = best {
            best.metric = best_metric;
            best.stages = stages; best.passes = passes; best.scored = scored; best.restarts = options.restart_count as usize;
            best.accepted = all_accepted; best.warnings = warnings;
            return Ok(best);
        }
        return Ok(OptimizeResult { gearset: request.player.gearset, player: None, metric: best_metric, legacy_output: Vec::new(),
            calculation: Vec::new(),
            pdt: f64::NAN, mdt: f64::NAN, feasible: false, termination: OptimizeTermination::NoWinner,
            accepted: all_accepted, similar: Vec::new(), stages, passes, scored,
            restarts: options.restart_count as usize, invert: 1, warnings });
    }
    single_start(request, &options, engine, stop, progress, trace)
}

fn post_reorder_context<R: RandomSource>(request: &RawOptimizeRequest, action: RawAction,
    engine: &EvaluationContext<R>) -> Result<ScoreContext, CalcError> {
    let pieces = intern_pieces(&engine.catalog, &engine.state, [request.player.gearset.as_slice()]);
    let legality = LegalityRules::production(request.player.main_job, request.player.sub_job, &action.action_type, &action.ws_name, &action.spell_name);
    Ok(ScoreContext::new(request.player.clone(), request.enemy.clone(), action, legality, pieces, &engine.catalog,
        can_use_fast_dt_gate(&request.player.abilities)?))
}

fn resolve_workers(options: &OptimizeOptions, automatic: Option<usize>) -> usize {
    if options.workers > 0 { options.workers as usize }
    else { automatic.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from)).max(1) }
}

fn single_start<R: RandomSource>(request: &RawOptimizeRequest, options: &OptimizeOptions, engine: &mut EvaluationContext<R>,
    stop: &AtomicBool, progress: &mut dyn FnMut(Progress), trace: &mut Option<SearchTrace>) -> Result<OptimizeResult, CalcError> {
    let fast_gate = can_use_fast_dt_gate(&request.player.abilities)?;
    let mut action = request.action.clone();
    action.ws_type = optimizer_ws_type(&action.ws_name);
    action.spell_type = optimizer_spell_type(&action.spell_name);
    let mut candidates = request.candidates.clone();
    if action.ws_type == WsType::Melee && !matches!(request.player.main_job, Job::Rng | Job::Cor) {
        candidates[Slot::Ranged.index()].retain(|&id| !matches!(engine.catalog.piece(id).kind, PieceKind::Crossbow | PieceKind::Gun | PieceKind::Bow));
        candidates[Slot::Ammo.index()].retain(|&id| { let piece = engine.catalog.piece(id);
            !matches!(piece.kind, PieceKind::Bolt | PieceKind::Bullet | PieceKind::Arrow) && !piece.name2.contains("antitail") });
    }
    for pool in &mut candidates { pool.retain(|&id| engine.catalog.piece(id).eligible_for(request.player.main_job)); }
    let mut starting = request.player.gearset;
    let empty = engine.catalog.by_symbol("Empty").ok_or_else(|| CalcError::Input { operation: "optimize", field: "Empty", message: "catalog has no Empty piece".into() })?;
    for &slot in Slot::ALL {
        let index = slot.index();
        if !engine.catalog.piece(starting[index]).eligible_for(request.player.main_job) { starting[index] = empty; }
        if !candidates[index].is_empty() {
            let selected = candidates[index][engine.random.bounded(candidates[index].len())];
            let state = state_snapshot(trace, &engine.state);
            record(trace, || TraceEvent::Choice { pool: candidates[index].clone(), selected, state: state.expect("enabled trace snapshot") });
            starting[index] = selected;
            let paired = match slot { Slot::Ring2 => Some(Slot::Ring1), Slot::Ear2 => Some(Slot::Ear1), _ => None };
            if let Some(paired) = paired {
                if engine.catalog.piece(starting[paired.index()]).name2 == engine.catalog.piece(starting[index]).name2 { starting[index] = empty; }
            }
        }
    }
    let pieces = intern_pieces(&engine.catalog, &engine.state,
        std::iter::once(starting.as_slice()).chain(candidates.iter().map(Vec::as_slice)));
    let legality = LegalityRules::production(request.player.main_job, request.player.sub_job, &action.action_type, &action.ws_name, &action.spell_name);
    let context = Arc::new(ScoreContext::new(request.player.clone(), request.enemy.clone(), action, legality, pieces, &engine.catalog, fast_gate));
    let mut best_ids = context.ids(&starting);
    let pools: Arc<[Vec<usize>]> = candidates.iter().map(|pool| pool.iter().map(|&id| context.pieces.id(id)).collect()).collect::<Vec<_>>().into();
    let swappable: Vec<usize> = pools.iter().enumerate().filter_map(|(index, pool)| (!pool.is_empty()).then_some(index)).collect();
    let local_combos = slot_combos(swappable.len(), options.max_swap_slots as usize);
    let candidate_estimate = local_combos.iter().map(|combo| combo.as_slice().iter().fold(1_usize,
        |product, &axis| product.saturating_mul(pools[swappable[axis]].len()))).fold(0_usize, usize::saturating_add);
    if context.legality.kind == super::legality::SearchKind::Other {
        return Err(CalcError::legacy("optimize", "action_type", "KeyError",
            crate::weaponskills::python_key_repr(&context.action.action_type)));
    }
    let mut scratch = ScoreScratch::new(&context);
    // This source-only fallback tolerates an invalid initial action but still preserves its model/draw effects.
    let calculation_observer = engine.calculation_trace.take();
    let invert = scratch.with_output(&context, &best_ids, engine, trace).map_or(1, |evaluation| evaluation.invert);
    engine.calculation_trace = calculation_observer;
    let mut mapper = Mapper::new(context.clone(), best_ids,
        MapperConfig::production(resolve_workers(options, request.automatic_workers), candidate_estimate), trace);
    let mut warnings = Vec::new();
    if let Some(warning) = &mapper.warning {
        eprintln!("{warning}");
        progress(Progress { stage: 0, pass: 0, scored: 0, best_metric: 0.0001, pdt: 200.0, mdt: 200.0, message: Some(warning.clone()) });
        warnings.push(warning.clone());
    } else if mapper.is_parallel() {
        progress(Progress { stage: 0, pass: 0, scored: 0, best_metric: 0.0001, pdt: 200.0, mdt: 200.0,
            message: Some(format!("Scoring ~{candidate_estimate} candidates per pass across {} workers.", mapper.width)) });
    }
    let mut achieved = [200.0; 2];
    let mut old_achieved = achieved;
    let requirements = [request.min_pdt, request.min_mdt];
    let mut thresholds = [200.0; 2];
    let mut unchanged = 0;
    let mut best_metric = 0.0001;
    let mut output = vec![0.0; 3];
    let mut accepted = Vec::new();
    let mut similar = Vec::new();
    let mut stages = 0; let mut passes = 0; let mut scored = 0;
    let mut stage_pass = 0;
    let mut termination = OptimizeTermination::Converged;
    let mut best_set = starting;
    while achieved[0] > requirements[0] || achieved[1] > requirements[1] {
        interrupted(stop)?;
        stages += 1;
        record(trace, || TraceEvent::DtStage { thresholds, prior_achieved: achieved, ids: best_ids, unchanged });
        best_metric = 0.0001;
        stage_pass = 0;
        for pass in 0..options.iterations as usize {
            interrupted(stop)?;
            passes += 1;
            stage_pass = pass + 1;
            let converged_ids = best_ids;
            let mut order = swappable.clone();
            let before = trace.as_ref().map(|_| order.clone());
            shuffle(&mut engine.random, &mut order);
            if let Some(before) = before { record(trace, || TraceEvent::Shuffle { before, after: order.clone() }); }
            let combos = local_combos.iter().map(|combo| {
                let mut mapped = [0; MAX_SWAP_SLOTS];
                for (index, &axis) in combo.as_slice().iter().enumerate() { mapped[index] = order[axis]; }
                Tuple::new(&mapped[..combo.len()])
            }).collect();
            let mut neighborhood = Neighborhood::new(combos, pools.clone(), trace);
            let result = optimize_pass(&context, &mut mapper, &mut neighborhood,
                PassInput { ids: best_ids, metric: best_metric, output, invert, next_best_percent: request.similar_percent,
                    thresholds, stage: stages, pass: pass + 1, scored_before: scored,
                    verbose_swaps: request.verbose_swaps || request.player.abilities.enabled(Ability::VerboseSwaps) },
                engine, &mut scratch, stop, progress, trace)?;
            best_ids = result.ids; best_metric = result.metric; output = result.output;
            scored += result.scored; accepted.extend(result.accepted); similar = result.similar;
            if best_ids == converged_ids { termination = OptimizeTermination::Converged; break; }
            termination = OptimizeTermination::IterationLimit;
        }
        best_set = context.gearset(&best_ids);
        scratch.input.gearset = best_set;
        build_player(&scratch.input, engine, &mut scratch.player)?;
        achieved = dt_for_player(&scratch.player);
        if achieved == old_achieved {
            unchanged += 1;
            if unchanged >= 3 {
                record(trace, || TraceEvent::UnsatisfiedDt { achieved, unchanged });
                termination = OptimizeTermination::UnchangedDtStages;
                break;
            }
        } else { unchanged = 0; }
        old_achieved = achieved;
        thresholds = std::array::from_fn(|index| {
            let next = achieved[index] - options.dt_step;
            if next > requirements[index] { next } else { requirements[index] }
        });
        record(trace, || TraceEvent::DtStageEnd { achieved, next_thresholds: thresholds, unchanged, metric: best_metric });
        progress(Progress { stage: stages, pass: stage_pass, scored, best_metric, pdt: achieved[0], mdt: achieved[1], message: None });
    }
    interrupted(stop)?;
    let ear1 = engine.catalog.piece(best_set[Slot::Ear1.index()]);
    if context.legality.jse_ears.contains(&ear1.name) || ear1.name.as_ref() == "Balder Earring +1" {
        best_set.swap(Slot::Ear1.index(), Slot::Ear2.index());
    }
    scratch.input.gearset = best_set;
    build_player(&scratch.input, engine, &mut scratch.player)?;
    let final_achieved = dt_for_player(&scratch.player);
    let feasible = final_achieved[0] <= requirements[0] && final_achieved[1] <= requirements[1];
    if !feasible {
        if termination != OptimizeTermination::UnchangedDtStages { termination = OptimizeTermination::UnsatisfiedDt; }
        let warning = "Unable to find a set which satisfies the conditions better than the current set. Exiting.".to_owned();
        progress(Progress { stage: stages, pass: stage_pass, scored, best_metric, pdt: final_achieved[0], mdt: final_achieved[1], message: Some(warning.clone()) });
        warnings.push(warning);
    } else if best_metric == 0.0001 && accepted.is_empty() { termination = OptimizeTermination::NoWinner; }
    if engine.calculation_trace.is_some() {
        if scratch.calculation.is_empty() {
            scratch.calculation.push(CalculationStep {
                label: "Legacy output sentinel",
                equation: None,
                formula: "No accepted output evaluation was retained; legacy damage = {damage} and TP return = {tp_return} remain their initial display placeholders.",
                values: vec![("damage", output[0]), ("tp_return", output[1])],
            });
        } else {
            let scored_ear1 = context.pieces.pieces[best_ids[Slot::Ear1.index()]];
            let scored_ear2 = context.pieces.pieces[best_ids[Slot::Ear2.index()]];
            scratch.calculation.push(CalculationStep {
                label: "Retained output provenance",
                equation: None,
                formula: "Damage and TP above use the last accepted scored set before final earring ordering: ear1 item ID = {scored_ear1}; ear2 item ID = {scored_ear2}. The returned set has ear1 item ID = {final_ear1}; ear2 item ID = {final_ear2}. The retained damage/TP are not recomputed for that final ordering.",
                values: vec![("scored_ear1", f64::from(engine.catalog.piece(scored_ear1).original_id)),
                    ("scored_ear2", f64::from(engine.catalog.piece(scored_ear2).original_id)),
                    ("final_ear1", f64::from(engine.catalog.piece(best_set[Slot::Ear1.index()]).original_id)),
                    ("final_ear2", f64::from(engine.catalog.piece(best_set[Slot::Ear2.index()]).original_id))],
            });
        }
        scratch.calculation.push(CalculationStep {
            label: "Single-start search score",
            equation: None,
            formula: "Final single-start search score = {score}, not a new evaluation of the final ordered set. A DT-stage boundary can reset the search score without replacing retained damage/TP.",
            values: vec![("score", best_metric)],
        });
    }
    let state = state_snapshot(trace, &engine.state);
    record(trace, || TraceEvent::BuildReturn { ids: best_ids, metric: best_metric, output: output.clone(), invert,
        achieved_before_reorder: achieved, unchanged, starting, filtered: candidates, player: Box::new(scratch.player.clone()), state: state.expect("enabled trace snapshot") });
    interrupted(stop)?;
    Ok(OptimizeResult { gearset: best_set, player: Some(scratch.player), metric: best_metric, legacy_output: output,
        calculation: scratch.calculation,
        pdt: final_achieved[0], mdt: final_achieved[1], feasible, termination, accepted, similar,
        stages, passes, scored, restarts: 1, invert, warnings })
}
