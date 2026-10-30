use std::sync::Arc;

use wsdist_fixtures::Value;
use wsdist_core::Catalog;
use wsdist_core::actions::sample_round;
use wsdist_core::model::{Ability, AttackKind, AttackProc, EvaluationContext, HitEvent, RoundSample};
use wsdist_core::rng::{ScriptedTape, TapeRecord};
use wsdist_core::stats::Stat;

use crate::combat_support::{boundary_enemy, build_boundary, number};

pub struct SampleObservation {
    pub sample: RoundSample,
    pub trace: Vec<HitEvent>,
    pub draws: usize,
}

pub fn assert_close(label: &str, actual: f64, expected: f64) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}: expected NaN, got {actual:?}");
    } else if expected == 0.0 || expected.is_infinite() {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{label}: expected {expected:?}, got {actual:?}");
    } else {
        let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
        assert!(actual.is_finite() && (actual - expected).abs() <= tolerance,
            "{label}: expected {expected:.17e}, got {actual:.17e}");
    }
}

pub fn scripted_sample(case: &Value, catalog: &Arc<Catalog>) -> SampleObservation {
    let tape = wsdist_fixtures::from_value::<ScriptedTape>(&case["expected"]["value"]["scripted"]["tape"])
        .expect("frozen strict round tape");
    let mut ctx = EvaluationContext::new(Arc::clone(catalog), tape);
    let player = build_boundary(case, &mut ctx).expect("real captured player construction");
    let enemy = boundary_enemy(case);
    ctx.action_trace = Some(Vec::with_capacity(8));
    let action = &case["input"]["action"];
    let sample = sample_round(&player, &enemy, number(&action["starting_tp"]),
        number(&action["ws_threshold"]), &mut ctx).expect("scripted round");
    ctx.random.assert_finished().expect("strict complete round consumption");
    SampleObservation { sample, trace: ctx.action_trace.take().unwrap(), draws: ctx.random.position() }
}

// The corpus's sampled EnSpell cases all miss with host seed 11. Extend the
// captured eight-hit dual-wield tape with precisely its two setup uniforms,
// keeping every original physical/proc draw unchanged. This is a native edge
// scenario, not modified oracle evidence or a second random implementation.
pub fn enspell_sample(all_hit_case: &Value, catalog: &Arc<Catalog>) -> SampleObservation {
    let mut records = wsdist_fixtures::from_value::<Vec<TapeRecord>>(
        &all_hit_case["expected"]["value"]["scripted"]["tape"],
    ).expect("captured eight-hit dual-wield tape");
    let setup = TapeRecord::uniform(wsdist_core::model::RandomStream::Primary, 0.0, 1.0, 0.0);
    drop(records.splice(0..0, [setup, setup]));
    let tape = ScriptedTape::new(records);
    let mut ctx = EvaluationContext::new(Arc::clone(catalog), tape);
    let mut player = build_boundary(all_hit_case, &mut ctx).expect("real captured dual-wield player construction");
    let enemy = boundary_enemy(all_hit_case);
    player.abilities.set(Ability::EnSpell, true);
    player.abilities.enhancing_skill = 600.0;
    player.abilities.set_storm(Some(Arc::from("None")));
    for (stat, value) in [
        (Stat::EnSpellDamagePct, 0.0), (Stat::EnSpellDamage, 0.0),
        (Stat::EnSpellDamagePctMain, 50.0), (Stat::EnSpellDamagePctSub, 125.0),
        (Stat::EnSpellDamageMain, 17.0), (Stat::EnSpellDamageSub, 9.0),
        (Stat::MagicCritRateIi, 37.0), (Stat::ElementalBonus, 0.0),
        (Stat::EarthElementalBonus, 0.0), (Stat::WaterElementalBonus, 0.0),
        (Stat::WindElementalBonus, 0.0), (Stat::FireElementalBonus, 11.0),
        (Stat::IceElementalBonus, 0.0), (Stat::ThunderElementalBonus, 0.0),
    ] {
        player.stats.set(stat, value);
    }
    ctx.action_trace = Some(Vec::with_capacity(8));
    let action = &all_hit_case["input"]["action"];
    let sample = sample_round(&player, &enemy, number(&action["starting_tp"]),
        number(&action["ws_threshold"]), &mut ctx).expect("all-hit EnSpell round");
    ctx.random.assert_finished().expect("EnSpell adds exactly two setup draws");
    SampleObservation { sample, trace: ctx.action_trace.take().unwrap(), draws: ctx.random.position() }
}

pub fn assert_trace_totals(observed: &SampleObservation) {
    let mut physical = 0.0;
    let mut magical = 0.0;
    let mut tp = 0.0;
    for (index, event) in observed.trace.iter().enumerate() {
        assert_eq!(usize::from(event.attempt), index + 1, "ordered actual attempts, including misses");
        assert!(event.attempt <= 8, "eight attempted hits, not eight landed hits");
        if !event.landed {
            assert!(!event.critical, "a miss cannot be critical");
            assert_eq!(event.physical_damage.to_bits(), 0.0_f64.to_bits(), "miss physical damage");
            assert_eq!(event.magical_damage.to_bits(), 0.0_f64.to_bits(), "miss magical damage");
            assert_eq!(event.tp_return.to_bits(), 0.0_f64.to_bits(), "miss TP");
        }
        physical += event.physical_damage;
        magical += event.magical_damage;
        tp += event.tp_return;
    }
    assert_close("actual traced physical damage", physical, observed.sample.physical_damage);
    assert_close("actual traced magical damage", magical, observed.sample.magical_damage);
    assert_close("actual traced TP", tp, observed.sample.tp_return);
    assert_eq!(observed.sample.damage.to_bits(),
        (observed.sample.physical_damage + observed.sample.magical_damage).to_bits(),
        "sample total must include magical damage despite the physical-only legacy tuple");
}

pub fn assert_enspell_sample(observed: &SampleObservation, original_case: &Value) {
    let fields = &original_case["expected"]["value"]["scripted"]["fields"];
    assert_close("unchanged physical tape result", observed.sample.physical_damage, number(&fields["physical_damage"]));
    assert_close("unchanged physical tape TP", observed.sample.tp_return, number(&fields["tp_return"]));
    let kinds = [AttackKind::Main, AttackKind::Offhand, AttackKind::Main, AttackKind::Main,
        AttackKind::Main, AttackKind::Offhand, AttackKind::Offhand, AttackKind::Offhand];
    assert_eq!(observed.trace.len(), kinds.len(), "all eight captured attempts are retained");
    for (event, kind) in observed.trace.iter().zip(kinds) {
        assert_eq!(event.kind, kind, "first pair before main/offhand bonus batches");
        assert_eq!(event.proc, AttackProc::QuadAttack, "QA takes precedence over TA/DA");
        assert!(event.landed, "zero-valued host tape lands each attempted hit");
        // Independent fixed values: skill 600 gives base 78; add 17/9, multiply
        // by main-hand 1.5 even for offhand, Fire 1.11, then magic-crit 1.25.
        let expected = if kind == AttackKind::Main { 197.71875 } else { 181.06875 };
        assert_close("sampled EnSpell per-hit damage", event.magical_damage, expected);
    }
    assert_close("eight-hit EnSpell sum", observed.sample.magical_damage, 1515.15);
    assert_eq!(observed.draws, original_case["expected"]["value"]["scripted"]["tape"].as_array().unwrap().len() + 2,
        "only the two observed setup uniforms are added");
    assert_trace_totals(observed);
}
