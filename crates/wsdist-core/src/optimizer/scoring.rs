use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use rayon::prelude::*;

use crate::actions::evaluate_action;
use crate::catalog::Catalog;
use crate::model::*;

use super::helpers::*;
use super::legality::LegalityRules;
use super::probe::{RawAction, SearchTrace, TraceEvent, record, state_snapshot};

#[derive(Clone, Debug)]
pub struct LegacyEvaluation {
    pub metric: f64, pub output: Vec<f64>, pub invert: i8, pub decimals: usize, pub nondecimals: usize,
}

#[derive(Clone, Debug)]
pub struct ScoreContext {
    pub player: PlayerInput,
    pub enemy: Enemy,
    pub action: RawAction,
    pub legality: LegalityRules,
    pub pieces: InternedPieces,
    pub dt_vectors: Vec<DtVector>,
    pub buff_dt: DtVector,
    pub fast_gate: bool,
    resolved_action: Option<ActionRequest>,
}

impl ScoreContext {
    pub fn new(player: PlayerInput, enemy: Enemy, action: RawAction, legality: LegalityRules,
        pieces: InternedPieces, catalog: &Catalog, fast_gate: bool) -> Self {
        let dt_vectors = pieces.pieces.iter().map(|&gear| piece_dt(catalog, gear)).collect();
        let buff_dt = dt_sum(player.effects.buckets.iter().map(|bucket| dt_vector(&bucket.stats)));
        let resolved_action = action.resolve().ok();
        Self { player, enemy, action, legality, pieces, dt_vectors, buff_dt, fast_gate, resolved_action }
    }
    pub fn gearset(&self, ids: &InternedSet) -> Gearset { ids.map(|id| self.pieces.pieces[id]) }
    pub fn ids(&self, gearset: &Gearset) -> InternedSet { gearset.map(|gear| self.pieces.id(gear)) }
    pub fn evaluate<R: RandomSource>(&self, player: &Player, engine: &mut EvaluationContext<R>,
        trace: &mut Option<SearchTrace>) -> Result<LegacyEvaluation, CalcError> {
        let resolved;
        let action = if let Some(action) = &self.resolved_action { action } else { resolved = self.action.resolve()?; &resolved };
        let result = evaluate_action(player, &self.enemy, action, engine)?;
        let metric = transform_metric(result.metric, result.invert)?;
        let mut output = Vec::with_capacity(4);
        output.push(result.damage);
        output.push(result.tp_return);
        if self.action.action_type.as_ref() == "attack round" { output.push(result.seconds_per_round.expect("round output timing")); }
        output.push(f64::from(result.invert));
        if engine.calculation_trace.is_some() {
            engine.record_calculation("Action evaluation output",
                "Damage = {damage}; TP return = {tp_return}.",
                &[("damage", result.damage), ("tp_return", result.tp_return)]);
            if let Some(seconds) = result.seconds_per_round {
                engine.record_calculation("Action evaluation timing",
                    "Seconds per attack round = {seconds}.", &[("seconds", seconds)]);
            }
            engine.record_calculation("Optimizer metric transformation",
                "score = {action_metric}^{invert}; use 0.0001 if score <= 0; resulting score = {score}.",
                &[("action_metric", result.metric), ("invert", f64::from(result.invert)), ("score", metric)]);
        }
        let evaluation = LegacyEvaluation { metric, output, invert: result.invert,
            decimals: if self.action.action_type.as_ref() == "attack round" { 3 } else { 1 }, nondecimals: 8 };
        let state = state_snapshot(trace, &engine.state);
        record(trace, || TraceEvent::Metric { player: Box::new(player.clone()), state: state.expect("enabled trace snapshot"), evaluation: evaluation.clone() });
        Ok(evaluation)
    }
}

pub struct ScoreScratch {
    pub input: PlayerInput,
    pub player: Player,
    pub(super) calculation: Vec<CalculationStep>,
}

impl ScoreScratch {
    pub fn new(context: &ScoreContext) -> Self {
        Self { input: context.player.clone(), player: Player::new(context.player.gearset), calculation: Vec::new() }
    }

    pub fn with_output<R: RandomSource>(&mut self, context: &ScoreContext, ids: &InternedSet,
        engine: &mut EvaluationContext<R>, trace: &mut Option<SearchTrace>) -> Result<LegacyEvaluation, CalcError> {
        self.input.gearset = context.gearset(ids);
        let (result, calculation) = capture_calculation(engine, |engine| {
            build_player(&self.input, engine, &mut self.player)?;
            let result = context.evaluate(&self.player, engine, trace)?;
            record(trace, || TraceEvent::Rescore { ids: *ids, evaluation: result.clone() });
            Ok(result)
        })?;
        self.calculation = calculation;
        Ok(result)
    }

    pub fn score<R: RandomSource>(&mut self, context: &ScoreContext, base: &InternedSet, groups: &[Group],
        thresholds: [f64; 2], engine: &mut EvaluationContext<R>, stop: &AtomicBool,
        trace: &mut Option<SearchTrace>) -> Result<Vec<f64>, CalcError> {
        self.score_slices(context, base, &group_slices(groups), thresholds, engine, stop, trace)
    }

    pub(crate) fn score_slices<R: RandomSource>(&mut self, context: &ScoreContext, base: &InternedSet,
        groups: &[GroupSlice<'_>], thresholds: [f64; 2], engine: &mut EvaluationContext<R>,
        stop: &AtomicBool, trace: &mut Option<SearchTrace>) -> Result<Vec<f64>, CalcError> {
        // Candidate and warmup actions never contribute display calculations.
        // Restore the caller's observer even when scoring fails or is cancelled.
        let calculation = engine.calculation_trace.take();
        let result = self.score_candidates(context, base, groups, thresholds, engine, stop, trace);
        engine.calculation_trace = calculation;
        result
    }

    fn score_candidates<R: RandomSource>(&mut self, context: &ScoreContext, base: &InternedSet,
        groups: &[GroupSlice<'_>], thresholds: [f64; 2], engine: &mut EvaluationContext<R>,
        stop: &AtomicBool, trace: &mut Option<SearchTrace>) -> Result<Vec<f64>, CalcError> {
        interrupted(stop)?;
        self.input.gearset = context.gearset(base);
        let base_dt = dt_sum(std::iter::once(context.buff_dt).chain(base.iter().map(|&id| context.dt_vectors[id])));
        let count = groups.iter().map(|group| group.candidates.len()).sum();
        let mut metrics = Vec::with_capacity(count);
        for group in groups {
            let vacated = dt_sum(group.combo.as_slice().iter().map(|&slot| context.dt_vectors[base[slot]]));
            let stripped = std::array::from_fn::<_, 6, _>(|index| base_dt[index] - vacated[index]);
            for items in group.candidates {
                for (&slot, &id) in group.combo.as_slice().iter().zip(items.as_slice()) {
                    self.input.gearset[slot] = context.pieces.pieces[id];
                }
                let valid = context.legality.is_valid(&self.input.gearset, &engine.catalog, &engine.state);
                record(trace, || TraceEvent::Legality { valid });
                if !valid { metrics.push(f64::NAN); continue; }
                if context.fast_gate {
                    let mut dt = stripped;
                    for &id in items.as_slice() { for (total, added) in dt.iter_mut().zip(context.dt_vectors[id]) { *total += added; } }
                    let achieved = dt_totals(dt);
                    if achieved[0] > thresholds[0] || achieved[1] > thresholds[1] {
                        record(trace, || TraceEvent::FastDtRejection { combo: group.combo, items: *items, achieved, thresholds });
                        metrics.push(f64::NAN);
                        continue;
                    }
                }
                build_player(&self.input, engine, &mut self.player)?;
                let achieved = dt_for_player(&self.player);
                let state = state_snapshot(trace, &engine.state);
                record(trace, || TraceEvent::FullPlayerDt { combo: group.combo, items: *items, achieved, thresholds,
                    player: Box::new(self.player.clone()), state: state.expect("enabled trace snapshot") });
                if achieved[0] > thresholds[0] || achieved[1] > thresholds[1] { metrics.push(f64::NAN); continue; }
                metrics.push(self.metric_only(context, engine, trace)?);
            }
            for &slot in group.combo.as_slice() { self.input.gearset[slot] = context.pieces.pieces[base[slot]]; }
        }
        record(trace, || TraceEvent::ScoreChunk { base: *base,
            groups: groups.iter().map(|group| Group { combo: group.combo, candidates: group.candidates.to_vec() }).collect(),
            thresholds, metrics: metrics.clone() });
        interrupted(stop)?;
        Ok(metrics)
    }

    fn metric_only<R: RandomSource>(&self, context: &ScoreContext, engine: &mut EvaluationContext<R>,
        trace: &mut Option<SearchTrace>) -> Result<f64, CalcError> {
        // Do not allocate the discarded legacy display tuple in the candidate loop.
        if trace.is_some() { return Ok(context.evaluate(&self.player, engine, trace)?.metric); }
        let resolved;
        let action = if let Some(action) = &context.resolved_action { action } else { resolved = context.action.resolve()?; &resolved };
        let result = evaluate_action(&self.player, &context.enemy, action, engine)?;
        transform_metric(result.metric, result.invert)
    }
}

/// Capture one existing evaluation without replacing or filling the caller's observer.
pub(super) fn capture_calculation<R: RandomSource, T>(engine: &mut EvaluationContext<R>,
    evaluate: impl FnOnce(&mut EvaluationContext<R>) -> Result<T, CalcError>)
    -> Result<(T, Vec<CalculationStep>), CalcError> {
    let observer = engine.calculation_trace.take();
    engine.calculation_trace = observer.as_ref().map(|_| Vec::new());
    let result = evaluate(engine);
    let calculation = std::mem::replace(&mut engine.calculation_trace, observer).unwrap_or_default();
    result.map(|result| (result, calculation))
}

fn transform_metric(value: f64, invert: i8) -> Result<f64, CalcError> {
    // Python float exponentiation raises here; Rust powi would invent an infinite winner.
    if value == 0.0 && invert < 0 {
        return Err(CalcError::legacy("optimize", "metric", "ZeroDivisionError",
            "0.0 cannot be raised to a negative power"));
    }
    let metric = value.powi(i32::from(invert));
    Ok(if metric <= 0.0 { 0.0001 } else { metric })
}

pub(crate) fn interrupted(stop: &AtomicBool) -> Result<(), CalcError> {
    if stop.load(Ordering::Relaxed) { Err(CalcError::Interrupted { operation: "optimize" }) } else { Ok(()) }
}

#[derive(Clone, Copy, Debug)]
pub struct MapperConfig {
    pub workers: usize,
    pub candidate_estimate: usize,
    pub force_pool: bool,
    pub batch_floor: usize,
    pub batch_ceiling: usize,
}

impl MapperConfig {
    pub fn production(workers: usize, candidate_estimate: usize) -> Self {
        Self { workers, candidate_estimate, force_pool: false, batch_floor: 8, batch_ceiling: 64 }
    }
}

struct Worker<R: RandomSource> { engine: EvaluationContext<R>, scratch: ScoreScratch, warmed: bool }

pub struct Mapper<R: RandomSource> {
    context: Arc<ScoreContext>,
    pool: Option<rayon::ThreadPool>,
    workers: Vec<Mutex<Worker<R>>>,
    warmup_ids: InternedSet,
    pub width: usize,
    pub batch_floor: usize,
    pub batch_ceiling: usize,
    pub warning: Option<String>,
}

impl<R: RandomSource> Mapper<R> {
    pub fn new(context: Arc<ScoreContext>, warmup_ids: InternedSet,
        config: MapperConfig, trace: &mut Option<SearchTrace>) -> Self {
        let use_pool = config.force_pool || (config.workers > 1 && config.candidate_estimate >= PARALLEL_CANDIDATE_THRESHOLD);
        let (pool, warning) = if use_pool {
            match rayon::ThreadPoolBuilder::new().num_threads(config.workers).build() {
                Ok(pool) => (Some(pool), None),
                Err(error) => (None, Some(format!("Parallel evaluation unavailable ({error}); scoring in a single process."))),
            }
        } else { (None, None) };
        let parallel = pool.is_some();
        let width = if parallel { config.workers } else { 1 };
        let floor = if parallel { 256 } else { config.batch_floor };
        let ceiling = if parallel { 256 * width } else { config.batch_ceiling };
        record(trace, || TraceEvent::MapperSelection { workers: config.workers, candidate_estimate: config.candidate_estimate,
            parallel, width, floor, ceiling });
        Self { context, pool, workers: Vec::new(), warmup_ids,
            width, batch_floor: floor, batch_ceiling: ceiling, warning }
    }

    pub fn is_parallel(&self) -> bool { self.pool.is_some() }

    pub fn score(&mut self, base: &InternedSet, groups: &[Group], total: usize, thresholds: [f64; 2],
        coordinator: &mut EvaluationContext<R>, scratch: &mut ScoreScratch, stop: &AtomicBool,
        trace: &mut Option<SearchTrace>) -> Result<Vec<f64>, CalcError> {
        interrupted(stop)?;
        let metrics = if total == 0 { Vec::new() }
        else if self.pool.is_none() || total < 192 {
            scratch.score(&self.context, base, groups, thresholds, coordinator, stop, trace)?
        } else {
            let parts = (4 * self.width).min((total / 24).max(1));
            let shares = split_groups(groups, total, parts);
            // ProcessPoolExecutor serializes initializer inputs at its first
            // dispatched batch, after any preceding local model mutations.
            if self.workers.is_empty() {
                self.workers = (0..self.width).map(|_| {
                    let engine = EvaluationContext {
                        catalog: coordinator.catalog.clone(), state: coordinator.state.clone(), random: coordinator.random.clone(),
                        scratch: EvaluationScratch::default(), player_stages: None, action_trace: None, calculation_trace: None,
                    };
                    Mutex::new(Worker { engine, scratch: ScoreScratch::new(&self.context), warmed: false })
                }).collect();
            }
            let pool = self.pool.as_ref().expect("pool selected");
            let context = &self.context;
            let workers = &self.workers;
            let warmup_ids = self.warmup_ids;
            let results: Vec<Result<Vec<f64>, CalcError>> = pool.install(|| shares.par_iter().map(|share| {
                let index = rayon::current_thread_index().expect("per-job pool worker");
                let mut worker = workers[index].lock();
                let Worker { engine, scratch, warmed } = &mut *worker;
                if !*warmed {
                    let candidates = [Tuple::new(&[warmup_ids[0]])];
                    let group = [GroupSlice { combo: Tuple::new(&[0]), candidates: &candidates }];
                    scratch.score_slices(context, &warmup_ids, &group, [f64::INFINITY; 2], engine, stop, &mut None)?;
                    *warmed = true;
                }
                scratch.score_slices(context, base, share, thresholds, engine, stop, &mut None)
            }).collect());
            let mut metrics = Vec::with_capacity(total);
            for result in results { metrics.extend(result?); }
            metrics
        };
        record(trace, || TraceEvent::MapperScores { base: *base, groups: groups.to_vec(), total, thresholds, metrics: metrics.clone(),
            parallel: self.pool.is_some() && total >= 192 });
        interrupted(stop)?;
        Ok(metrics)
    }
}
