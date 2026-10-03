//! Analytical and sampled legacy action engines; the request preserves source dispatch.

mod common;
mod round;
mod spell;
mod ws;

use crate::model::{ActionRequest, ActionResult, CalcError, Enemy, EvaluationContext, Player, RandomSource};

pub use round::{evaluate_round, sample_round};
pub use spell::{SPELL_REGISTRY, SpellSpec, evaluate_spell, gui_spell_type, optimizer_spell_type, resolve_spell_name};
pub use ws::{evaluate_ws, sample_ws};
pub(crate) use common::TWO_HANDED_SKILLS;

pub fn evaluate_action<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    request: &ActionRequest,
    ctx: &mut EvaluationContext<R>,
) -> Result<ActionResult, CalcError> {
    match request {
        ActionRequest::Round { metric, starting_tp, threshold } => evaluate_round(player, enemy, *starting_tp, *threshold, metric, ctx),
        ActionRequest::Ws(request) => evaluate_ws(player, enemy, request, ctx),
        ActionRequest::Spell(request) => evaluate_spell(player, enemy, request, ctx),
    }
}
