use std::sync::atomic::AtomicBool;

use crate::actions::sample_ws;
use crate::model::{
    CalcError, DistributionRequest, DistributionResult, EvaluationContext,
    Player, RandomSource, build_player,
};

use super::{check_stop, statistics::summarize_distribution};

const OPERATION: &str = "sample_distribution";
const SAMPLE_COUNT: usize = 20000;

/// Sample the source GUI's 20000 independent fixed-TP, single weapon skills.
pub fn sample_distribution<R: RandomSource>(
    request: &DistributionRequest,
    ctx: &mut EvaluationContext<R>,
    stop: &AtomicBool,
) -> Result<DistributionResult, CalcError> {
    check_stop(stop, OPERATION)?;
    let mut player = Player::new(request.player.gearset);
    build_player(&request.player, ctx, &mut player)?;
    check_stop(stop, OPERATION)?;
    let mut damage = Vec::with_capacity(SAMPLE_COUNT);
    let mut tp = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        check_stop(stop, OPERATION)?;
        let sample = sample_ws(&player, &request.enemy, &request.ws, true, ctx)?;
        check_stop(stop, OPERATION)?;
        damage.push(sample.damage);
        tp.push(sample.tp_return);
    }
    check_stop(stop, OPERATION)?;
    let result = summarize_distribution(damage, tp)?;
    check_stop(stop, OPERATION)?;
    Ok(result)
}
