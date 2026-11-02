//! Ordered legacy first-improvement search with DT stages and per-job scoring.

mod helpers;
mod legality;
mod neighborhood;
mod scoring;
mod search;
pub mod probe;

use std::sync::atomic::AtomicBool;

use crate::model::{CalcError, EvaluationContext, OptimizeRequest, OptimizeResult, Progress, RandomSource};
use probe::{RawAction, RawOptimizeRequest, SearchTrace};

pub use helpers::{MAX_SWAP_SLOTS, PARALLEL_CANDIDATE_THRESHOLD};

pub fn optimize<R: RandomSource>(request: &OptimizeRequest, ctx: &mut EvaluationContext<R>, stop: &AtomicBool,
    progress: &mut dyn FnMut(Progress)) -> Result<OptimizeResult, CalcError> {
    let mut action = RawAction::production(&request.action);
    action.ws_name = request.ws_name.clone();
    action.spell_name = request.spell_name.clone();
    let raw = RawOptimizeRequest { player: request.player.clone(), enemy: request.enemy.clone(),
        action, candidates: request.candidates.clone(),
        min_pdt: request.min_pdt, min_mdt: request.min_mdt, options: request.options.clone(),
        similar_results: request.similar_results, similar_percent: request.similar_percent,
        verbose_swaps: request.verbose_swaps, automatic_workers: None };
    // The app-facing accepted/similar records are retained by the driver. Full
    // candidate/player/cursor evidence is requested only by the numerical probe.
    let mut trace = None::<SearchTrace>;
    search::optimize_raw(&raw, ctx, stop, progress, &mut trace)
}
