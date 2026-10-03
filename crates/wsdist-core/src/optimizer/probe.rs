//! Narrow numerical probes for the frozen helper, cursor and logical-worker corpus.
//! These use the same scorer and first-improvement driver as production.

use std::sync::Arc;

use crate::actions::{optimizer_spell_type, resolve_spell_name};
use crate::model::*;
use crate::weaponskills::resolve_name;

pub use super::helpers::{DtVector, Group, GroupSlice, InternedPieces, InternedSet, Tuple, can_use_fast_dt_gate,
    dt_for_player, dt_sum, dt_totals, dt_vector, group_slices, intern_pieces, normalize_options,
    restart_options, slot_combos, split_groups};
pub use super::legality::{LegalityRules, RESTRICTED_WS, SearchKind, WS_MENUS, jse_ear_names, optimizer_ws_type};
pub use super::neighborhood::{Cursor, Neighborhood};
pub use super::scoring::{CompletedWorker, LegacyEvaluation, Mapper, MapperConfig, ScoreContext, ScoreScratch, WorkerInput};
pub use super::search::{PassInput, PassResult, RawOptimizeRequest, optimize_pass, optimize_raw};

#[derive(Clone, Debug)]
pub struct RawAction {
    pub ws_name: Arc<str>, pub spell_name: Arc<str>, pub action_type: Arc<str>,
    pub min_tp: f64, pub ws_type: WsType, pub spell_type: SpellType, pub metric: Metric,
}

impl RawAction {
    pub fn production(action: &ActionRequest) -> Self {
        match action {
            ActionRequest::Round { metric, threshold, .. } => Self { ws_name: Arc::from(""), spell_name: Arc::from(""),
                action_type: Arc::from("attack round"), min_tp: *threshold, ws_type: WsType::None,
                spell_type: SpellType::ElementalMagic, metric: metric.clone() },
            ActionRequest::Ws(request) => Self { ws_name: Arc::from(request.skill.name()), spell_name: Arc::from(""),
                action_type: Arc::from("weapon skill"), min_tp: request.tp, ws_type: optimizer_ws_type(request.skill.name()),
                spell_type: SpellType::ElementalMagic, metric: request.metric.clone() },
            ActionRequest::Spell(request) => {
                let name = crate::actions::SPELL_REGISTRY[request.spell.index()].name;
                Self { ws_name: Arc::from(""), spell_name: Arc::from(name), action_type: Arc::from("spell cast"),
                    min_tp: 1000.0, ws_type: WsType::None, spell_type: optimizer_spell_type(name), metric: request.metric.clone() }
            }
        }
    }

    pub fn resolve(&self) -> Result<ActionRequest, CalcError> {
        match self.action_type.as_ref() {
            "weapon skill" => Ok(ActionRequest::Ws(WsRequest { skill: resolve_name(&self.ws_name)?, ws_type: self.ws_type,
                tp: self.min_tp, metric: self.metric.clone() })),
            "spell cast" => Ok(ActionRequest::Spell(SpellRequest { spell: resolve_spell_name(&self.spell_name)?,
                spell_type: self.spell_type, metric: self.metric.clone() })),
            "attack round" => Ok(ActionRequest::Round { metric: self.metric.clone(), starting_tp: 0.0, threshold: self.min_tp }),
            name => Err(CalcError::legacy("optimize", "action_type", "ValueError", format!("Unknown action_type ({name})"))),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SearchTrace {
    pub events: Vec<TraceEvent>,
    state_cache: Option<(usize, u64, Arc<ModelState>)>,
}

#[derive(Clone, Debug)]
pub enum TraceEvent {
    Neighborhood { combos: Vec<Tuple>, pools: Vec<Vec<usize>> },
    Shuffle { before: Vec<usize>, after: Vec<usize> },
    Choice { pool: Vec<GearId>, selected: GearId, state: Arc<ModelState> },
    PoolOrders { index: usize, combo: Tuple, orders: Vec<Vec<usize>> },
    Take { base: Vec<usize>, limit: usize, before: Cursor, after: Cursor, groups: Vec<Group>, total: usize },
    Seek { before: Cursor, after: Cursor },
    Legality { valid: bool },
    FastDtRejection { combo: Tuple, items: Tuple, achieved: [f64; 2], thresholds: [f64; 2] },
    FullPlayerDt { combo: Tuple, items: Tuple, achieved: [f64; 2], thresholds: [f64; 2], player: Box<Player>, state: Arc<ModelState> },
    Metric { player: Box<Player>, state: Arc<ModelState>, evaluation: LegacyEvaluation },
    Rescore { ids: InternedSet, evaluation: LegacyEvaluation },
    ScoreChunk { base: InternedSet, groups: Vec<Group>, thresholds: [f64; 2], metrics: Vec<f64> },
    MapperSelection { workers: usize, candidate_estimate: usize, parallel: bool, width: usize, floor: usize, ceiling: usize },
    MapperScores { base: InternedSet, groups: Vec<Group>, total: usize, thresholds: [f64; 2], metrics: Vec<f64>, parallel: bool },
    PassStart { ids: InternedSet, metric: f64, thresholds: [f64; 2] },
    Accepted { before: InternedSet, before_metric: f64, combo: Tuple, items: Tuple, metric: f64,
        batch: usize, scanned: usize, total: usize, rewind: Cursor },
    PassReturn { ids: InternedSet, metric: f64, output: Vec<f64>, cursor: Cursor, similar: Vec<SimilarSwap> },
    DtStage { thresholds: [f64; 2], prior_achieved: [f64; 2], ids: InternedSet, unchanged: usize },
    DtStageEnd { achieved: [f64; 2], next_thresholds: [f64; 2], unchanged: usize, metric: f64 },
    UnsatisfiedDt { achieved: [f64; 2], unchanged: usize },
    RestartComparison { restart: usize, metric: f64, incumbent: f64, improved: bool },
    BuildReturn { ids: InternedSet, metric: f64, output: Vec<f64>, invert: i8, achieved_before_reorder: [f64; 2],
        unchanged: usize, starting: Gearset, filtered: CandidatePools, player: Box<Player>, state: Arc<ModelState> },
}

pub(crate) fn record(trace: &mut Option<SearchTrace>, event: impl FnOnce() -> TraceEvent) {
    if let Some(trace) = trace { trace.events.push(event()); }
}

pub(crate) fn state_snapshot(trace: &mut Option<SearchTrace>, state: &ModelState) -> Option<Arc<ModelState>> {
    trace.as_mut().map(|trace| {
        // A generation is local to one mutable coordinator/worker, not global.
        let owner = std::ptr::from_ref(state) as usize;
        let generation = state.generation();
        if let Some((cached_owner, cached_generation, snapshot)) = &trace.state_cache {
            if *cached_owner == owner && *cached_generation == generation { return snapshot.clone(); }
        }
        let snapshot = Arc::new(state.clone());
        trace.state_cache = Some((owner, generation, snapshot.clone()));
        snapshot
    })
}
