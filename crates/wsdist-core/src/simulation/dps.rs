use std::sync::atomic::AtomicBool;

use crate::actions::{TWO_HANDED_SKILLS, sample_round, sample_ws};
use crate::math::{get_delay_timing, get_dex_crit, get_tp, python_max, python_round1};
use crate::model::{
    CalcError, DpsRequest, DpsResult, Enemy, EvaluationContext, Job, Metric,
    PieceKind, Player, RandomSource, SimulationEvent, SimulationEventKind,
    SkillType, Slot, build_player,
};
use crate::stats::Stat;

use super::{check_stop, statistics::numpy_mean};

const OPERATION: &str = "simulate_dps";
const HORIZON_SECONDS: f64 = 7200.0;
const WS_DELAY_SECONDS: f64 = 2.0;

struct Timing {
    seconds_per_round: f64,
    round_regain: f64,
    ws_regain: f64,
}

#[inline]
fn regain<R: RandomSource>(player: &Player, ctx: &EvaluationContext<R>) -> f64 {
    let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
    player.stats.get(Stat::DualWield) * if main.name.as_ref() == "Gokotai" { 1.0 } else { 0.0 }
        + player.stats.get(Stat::Regain)
}

fn timing<R: RandomSource>(
    tp_player: &Player,
    ws_player: &Player,
    ctx: &EvaluationContext<R>,
) -> Result<Timing, CalcError> {
    let tp_main = ctx.catalog.piece(tp_player.gearset[Slot::Main.index()]);
    let tp_sub = ctx.catalog.piece(tp_player.gearset[Slot::Sub.index()]);
    let ws_main = ctx.catalog.piece(ws_player.gearset[Slot::Main.index()]);
    let ws_sub = ctx.catalog.piece(ws_player.gearset[Slot::Sub.index()]);
    // Both sets participate, and the WS set (not the TP set) controls the H2H
    // exclusion of Delay2. This intentionally differs from round evaluation.
    let dual_wield = (tp_sub.kind == PieceKind::Weapon || ctx.state.skill(tp_main) == SkillType::HandToHand)
        && (ws_sub.kind == PieceKind::Weapon || ctx.state.skill(ws_main) == SkillType::HandToHand);
    let stats = &tp_player.stats;
    let delay2 = if dual_wield && ctx.state.skill(ws_main) != SkillType::HandToHand {
        stats.require(Stat::Delay2, OPERATION)?
    } else {
        0.0
    };
    let seconds_per_round = get_delay_timing(
        stats.require(Stat::Delay1, OPERATION)?, delay2,
        stats.get(Stat::DualWield) / 100.0, stats.get(Stat::MartialArts),
        stats.get(Stat::MagicHaste), stats.get(Stat::JaHaste), stats.get(Stat::GearHaste),
    );
    Ok(Timing {
        seconds_per_round,
        round_regain: python_round1((seconds_per_round / 3.0) * regain(tp_player, ctx)),
        ws_regain: python_round1((WS_DELAY_SECONDS / 3.0) * regain(ws_player, ctx)),
    })
}

// capture_search_simulation.py:1110–1137. This bound uses only deterministic TP
// kernels and reachable hit types, and never consumes either random stream.
fn tp_upper_bound<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    rounded_regain: f64,
    ctx: &EvaluationContext<R>,
) -> Result<f64, CalcError> {
    let stats = &player.stats;
    let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
    let sub = ctx.catalog.piece(player.gearset[Slot::Sub.index()]);
    let ammo = ctx.catalog.piece(player.gearset[Slot::Ammo.index()]);
    let main_skill = ctx.state.skill(main);
    let modified_delay = ((stats.require(Stat::Delay1, OPERATION)?
        + stats.require(Stat::Delay2, OPERATION)?) / 2.0 - stats.get(Stat::MartialArts))
        * (1.0 - stats.get(Stat::DualWield) / 100.0);
    let crit_rate = stats.require(Stat::CritRate, OPERATION)? / 100.0
        + get_dex_crit(stats.require(Stat::Dex, OPERATION)?, enemy.stats.require(Stat::Agi, OPERATION)?);
    let crit_rate = if crit_rate > 1.0 { 1.0 } else { crit_rate };
    let stp = stats.get(Stat::StoreTp) / 100.0
        + (50.0 / 100.0) * crit_rate * if main.name.as_ref() == "Karambit" { 1.0 } else { 0.0 };
    let hit_delay = if main_skill == SkillType::HandToHand { modified_delay / 2.0 } else { modified_delay };
    let normal_tp = get_tp(1.0, hit_delay, stp, false)?;
    let reachable_zanshin = TWO_HANDED_SKILLS.contains(&main_skill) && sub.kind != PieceKind::Weapon
        && (stats.get(Stat::Zanshin) > 0.0 || stats.get(Stat::ZanshinOa2) > 0.0);
    let zanshin_tp = if reachable_zanshin { get_tp(1.0, hit_delay, stp, player.main_job == Job::Sam)? } else { 0.0 };
    let reachable_daken = ctx.state.skill(ammo) == SkillType::Throwing && stats.get(Stat::Daken) > 0.0;
    let daken_tp = if reachable_daken { get_tp(1.0, stats.get(Stat::AmmoDelay), stp, false)? } else { 0.0 };
    let largest_hit_tp = python_max(python_max(python_max(0.0, normal_tp), zanshin_tp), daken_tp);
    Ok(8.0 * largest_hit_tp + rounded_regain)
}

/// Run complete TP/WS cycles until the legacy 7200-second outer horizon.
///
/// The inner TP phase and its following WS finish even after crossing the
/// horizon. Stored TP is never clamped: only the sampled WS's effective TP is.
pub fn simulate_dps<R: RandomSource>(
    request: &DpsRequest,
    ctx: &mut EvaluationContext<R>,
    stop: &AtomicBool,
) -> Result<DpsResult, CalcError> {
    check_stop(stop, OPERATION)?;
    let mut tp_player = Player::new(request.tp_player.gearset);
    build_player(&request.tp_player, ctx, &mut tp_player)?;
    check_stop(stop, OPERATION)?;
    let mut ws_player = Player::new(request.ws_player.gearset);
    build_player(&request.ws_player, ctx, &mut ws_player)?;
    check_stop(stop, OPERATION)?;
    let timing = timing(&tp_player, &ws_player, ctx)?;
    let mut ws_request = request.ws.clone();
    ws_request.metric = Metric::new("Damage dealt");

    // These five arrays retain the source reduction inputs, not differences of
    // cumulative totals (which would change NumPy's displayed averages).
    let mut round_damage = Vec::with_capacity(4096);
    let mut physical_round_damage = Vec::with_capacity(4096);
    let mut magical_round_damage = Vec::with_capacity(4096);
    let mut ws_damage_samples = Vec::with_capacity(4096);
    let mut pre_bonus_tp = Vec::with_capacity(4096);
    let mut total_series = Vec::with_capacity(8192);
    let mut tp_series = Vec::with_capacity(4096);
    let mut ws_series = Vec::with_capacity(4096);
    let mut events = if request.trace { Vec::with_capacity(8192) } else { Vec::new() };
    let mut time = 0.0;
    let mut tp = 0.0;
    let mut damage = 0.0;
    let mut tp_damage = 0.0;
    let mut ws_damage = 0.0;
    // Players, timing and hit eligibility are immutable for this job. Compute
    // lazily only if a TP phase is entered, then reuse its no-draw bound.
    let mut cached_upper_bound = None;

    while time < HORIZON_SECONDS {
        check_stop(stop, OPERATION)?;
        if tp < request.threshold {
            let upper_bound = match cached_upper_bound {
                Some(bound) => bound,
                None => {
                    let bound = tp_upper_bound(&tp_player, &request.enemy, timing.round_regain, ctx)?;
                    cached_upper_bound = Some(bound);
                    bound
                }
            };
            if upper_bound <= 0.0 {
                return Err(CalcError::NonProgress {
                    operation: OPERATION, starting_tp: tp, threshold: request.threshold, upper_bound,
                });
            }
        }
        while tp < request.threshold {
            check_stop(stop, OPERATION)?;
            let tp_before = tp;
            let sample = sample_round(&tp_player, &request.enemy, tp, request.threshold, ctx)?;
            check_stop(stop, OPERATION)?;
            let sampled_damage = sample.physical_damage + sample.magical_damage;
            damage += sampled_damage;
            tp_damage += sampled_damage;
            tp += sample.tp_return;
            tp += timing.round_regain;
            time += timing.seconds_per_round;
            round_damage.push(sampled_damage);
            physical_round_damage.push(sample.physical_damage);
            magical_round_damage.push(sample.magical_damage);
            total_series.push([time, damage]);
            tp_series.push([time, tp_damage]);
            if request.trace {
                events.push(SimulationEvent {
                    kind: SimulationEventKind::Round, time, tp_before, tp_after: tp,
                    damage: sampled_damage, physical_damage: sample.physical_damage,
                    magical_damage: sample.magical_damage, tp_return: sample.tp_return,
                    regain_tp: timing.round_regain, cumulative_damage: damage,
                });
            }
        }
        check_stop(stop, OPERATION)?;
        let tp_before = tp;
        ws_request.tp = tp;
        let sample = sample_ws(&ws_player, &request.enemy, &ws_request, false, ctx)?;
        check_stop(stop, OPERATION)?;
        pre_bonus_tp.push(tp);
        damage += sample.damage;
        ws_damage += sample.damage;
        tp = sample.tp_return;
        time += WS_DELAY_SECONDS;
        tp += timing.ws_regain;
        ws_damage_samples.push(sample.damage);
        total_series.push([time, damage]);
        ws_series.push([time, ws_damage]);
        if request.trace {
            events.push(SimulationEvent {
                kind: SimulationEventKind::WeaponSkill, time, tp_before, tp_after: tp,
                damage: sample.damage, physical_damage: sample.physical_damage,
                magical_damage: sample.magical_damage, tp_return: sample.tp_return,
                regain_tp: timing.ws_regain, cumulative_damage: damage,
            });
        }
    }
    check_stop(stop, OPERATION)?;
    let result = DpsResult {
        final_time: time, final_tp: tp, damage, tp_damage, ws_damage,
        seconds_per_round: timing.seconds_per_round,
        round_count: round_damage.len(), ws_count: ws_damage_samples.len(),
        average_tp_damage: numpy_mean(&round_damage),
        average_physical_round_damage: numpy_mean(&physical_round_damage),
        average_magical_round_damage: numpy_mean(&magical_round_damage),
        average_ws_damage: numpy_mean(&ws_damage_samples),
        average_pre_bonus_tp: numpy_mean(&pre_bonus_tp),
        ws_tp_bonus: ws_player.stats.get(Stat::TpBonus),
        total_series, tp_series, ws_series, events,
    };
    check_stop(stop, OPERATION)?;
    Ok(result)
}
