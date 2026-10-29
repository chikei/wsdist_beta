#[allow(dead_code)]
mod combat_support;

use std::sync::Arc;

use wsdist_core::catalog::Catalog;
use wsdist_core::actions::{evaluate_ws, sample_ws};
use wsdist_core::model::{
    Ability, AttackKind, AttackProc, CalcError, EvaluationContext, HitEvent, Metric,
    WsRequest, WsSample, WsType,
};
use wsdist_core::rng::ScriptedTape;
use wsdist_core::stats::Stat;
use wsdist_core::weaponskills::resolve_name;

use combat_support::{
    boundary_enemy, build_boundary, document, fixture_root, index, number,
    replay_case, resource_root,
};

/// Full action parity, separate from the setup/table suite in legacy_ws.rs.
/// The shared adapter invokes the public analytical/sampled engine with the
/// recorded compiled draws and additionally replays every captured endpoint tape.
#[test]
fn all_frozen_weapon_actions_match_results_errors_draws_and_components() {
    let root = fixture_root();
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("reviewed inert catalogs"));
    let mut cases = 0;
    let mut recorded = 0;
    let mut scripted = 0;
    let mut hits = 0;
    for entry in index(&root).cases {
        if entry.operation != "combat.ws" { continue; }
        let case = document(&root.join(entry.path));
        let replay = replay_case(&case, &catalog);
        cases += 1;
        recorded += replay.recorded;
        scripted += replay.scripted;
        hits += replay.hits;
    }
    assert_eq!(cases, 12812, "the entire frozen combat.ws corpus must be replayed");
    eprintln!("WS action parity: {cases} cases, {recorded} recorded compiled-draw replays, {scripted} scripted, {hits} traced hits");
}

fn request(case: &serde_json::Value) -> WsRequest {
    let action = &case["input"]["action"];
    WsRequest {
        skill: resolve_name(action["name"].as_str().unwrap()).unwrap(),
        ws_type: WsType::from_name(action["ws_type"].as_str().unwrap()).unwrap(),
        tp: number(&action["tp"]),
        metric: Metric::new(action["metric"].as_str().unwrap()),
    }
}

fn traced_sample(path: &str, catalog: &Arc<Catalog>) -> (WsSample, Vec<HitEvent>) {
    let case = document(&fixture_root().join(path));
    let tape = ScriptedTape::from_json_records(
        &case["expected"]["value"]["scripted"]["tape"],
    ).expect("independently captured endpoint tape");
    let mut ctx = EvaluationContext::new(Arc::clone(catalog), tape);
    let player = build_boundary(&case, &mut ctx).expect("actual player build matches captured boundary");
    let enemy = boundary_enemy(&case);
    ctx.action_trace = Some(Vec::with_capacity(16));
    let result = sample_ws(&player, &enemy, &request(&case),
        case["input"]["action"]["single"].as_bool().unwrap(), &mut ctx).unwrap();
    ctx.random.assert_finished().expect("trace must not change random consumption");
    (result, ctx.action_trace.take().unwrap())
}

fn assert_close(actual: f64, expected: f64) {
    let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
    assert!((actual - expected).abs() <= tolerance,
        "actual {actual:.17e}, expected {expected:.17e}, tolerance {tolerance:.17e}");
}

#[test]
fn sampled_misses_use_attempts_and_selected_multiattack_precedence() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let (misses, trace) = traced_sample("combat/023398.json", &catalog);
    assert_eq!(misses.damage, 0.0);
    assert_eq!(misses.tp_return, 0.0);
    assert_eq!(trace.len(), 8);
    for (n, event) in trace.iter().enumerate() {
        assert_eq!(event.kind, AttackKind::Main);
        assert_eq!(event.attempt, n as u8 + 1);
        assert!(!event.landed);
        assert!(!event.critical);
        assert_eq!(event.physical_damage, 0.0);
        assert_eq!(event.magical_damage, 0.0);
        assert_eq!(event.tp_return, 0.0);
        assert_eq!(event.proc, if n < 2 { AttackProc::Normal } else { AttackProc::QuadAttack });
    }
    let (_, landed) = traced_sample("combat/023397.json", &catalog);
    assert_eq!(landed.len(), 8);
    assert!(landed.iter().all(|event| event.landed));
    assert!(landed[2..].iter().all(|event| event.proc == AttackProc::QuadAttack));

    // Seven native H2H main hits plus one offhand attempt exhaust the cap even
    // when every attempt misses. Guaranteed QA must never create a ninth hit.
    let (_, h2h) = traced_sample("combat/023404.json", &catalog);
    assert_eq!(h2h.len(), 8);
    assert_eq!(h2h[0].kind, AttackKind::Main);
    assert_eq!(h2h[1].kind, AttackKind::Offhand);
    for (n, event) in h2h.iter().enumerate() {
        assert_eq!(event.attempt, n as u8 + 1);
        assert!(!event.landed);
        assert_eq!(event.proc, AttackProc::Normal);
    }
}

#[test]
fn single_only_sneak_trick_and_flourishes_affect_only_the_first_sampled_hit() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    for (disabled, enabled, critical) in [
        ("combat/023465.json", "combat/023466.json", true),
        ("combat/023471.json", "combat/023472.json", true),
        ("combat/023477.json", "combat/023478.json", true),
        ("combat/023483.json", "combat/023484.json", true),
        ("combat/023489.json", "combat/023490.json", false),
    ] {
        let (_, ordinary) = traced_sample(disabled, &catalog);
        let (_, single) = traced_sample(enabled, &catalog);
        assert_eq!(single.len(), ordinary.len(), "single-only bonuses must not add or drop attempted-hit/proc records");
        assert!(ordinary[0].landed && single[0].landed);
        assert!(!ordinary[0].critical);
        assert_eq!(single[0].critical, critical);
        assert!(single[0].physical_damage > ordinary[0].physical_damage);
        for (ordinary, single) in ordinary[1..].iter().zip(&single[1..]) {
            assert_eq!(ordinary.kind, single.kind);
            assert_eq!(ordinary.proc, single.proc);
            assert_eq!(ordinary.attempt, single.attempt);
            assert_eq!(ordinary.landed, single.landed);
            assert_eq!(ordinary.critical, single.critical);
            assert_eq!(ordinary.physical_damage.to_bits(), single.physical_damage.to_bits());
            assert_eq!(ordinary.magical_damage.to_bits(), single.magical_damage.to_bits());
            assert_eq!(ordinary.tp_return.to_bits(), single.tp_return.to_bits());
        }
    }
}

#[test]
fn sampled_cap_does_not_overwrite_physical_magical_or_hit_components() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    for path in ["combat/023505.json", "combat/023509.json", "combat/023513.json"] {
        let (result, trace) = traced_sample(path, &catalog);
        assert_eq!(result.damage, 99999.0);
        let mut physical = 0.0;
        let mut magical = 0.0;
        let mut tp = 0.0;
        for event in &trace {
            physical += event.physical_damage;
            magical += event.magical_damage;
            tp += event.tp_return;
        }
        assert_close(physical, result.physical_damage);
        assert_close(magical, result.magical_damage);
        assert_close(tp, result.tp_return);
        assert!(physical + magical > result.damage, "the legacy total alone is capped");
        if path == "combat/023509.json" {
            let last = trace.last().unwrap();
            assert_eq!(last.kind, AttackKind::Magical);
            assert_eq!(last.attempt, 0, "hybrid magic is a component, not another physical attempt");
            assert_eq!(last.tp_return, 0.0);
            assert!(result.physical_damage > 0.0 && result.magical_damage > 0.0);
        } else if path == "combat/023513.json" {
            assert_eq!(trace.len(), 1);
            assert_eq!(trace[0].kind, AttackKind::Magical);
            assert_eq!(trace[0].attempt, 1);
            assert_eq!(result.physical_damage, 0.0);
            assert_eq!(trace[0].tp_return, result.tp_return);
        } else {
            assert!(trace.iter().all(|event| event.kind == AttackKind::Main));
            assert_eq!(result.magical_damage, 0.0);
        }
    }
}

#[test]
fn fotia_and_conserve_are_typed_tp_procs_not_fabricated_damage_hits() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    for (path, proc, returned) in [
        ("combat/023420.json", AttackProc::Fotia, 1000.0),
        ("combat/023429.json", AttackProc::ConserveTp, 104.0),
    ] {
        let (result, trace) = traced_sample(path, &catalog);
        assert_eq!(trace.len(), 3);
        let retention = &trace[2];
        assert_eq!(retention.kind, AttackKind::TpRetention);
        assert_eq!(retention.proc, proc);
        assert_eq!(retention.attempt, 0);
        assert!(retention.landed);
        assert!(!retention.critical);
        assert_eq!(retention.physical_damage, 0.0);
        assert_eq!(retention.magical_damage, 0.0);
        assert_eq!(retention.tp_return, returned);
        assert_eq!(trace[0].tp_return + trace[1].tp_return + returned, result.tp_return);
    }
    let (_, no_proc) = traced_sample("combat/023408.json", &catalog);
    assert_eq!(no_proc.len(), 2);
    assert!(no_proc.iter().all(|event| event.kind != AttackKind::TpRetention));
}

#[test]
fn action_required_key_errors_keep_source_precedence_before_any_draw() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let case = document(&fixture_root().join("combat/000215.json"));
    let mut ctx = EvaluationContext::new(catalog, ScriptedTape::new(Vec::new()));
    let mut player = build_boundary(&case, &mut ctx).unwrap();
    let original_enemy = boundary_enemy(&case);
    let mut enemy = original_enemy.clone();
    enemy.stats.remove(Stat::Evasion);
    player.stats.remove(Stat::Delay1);
    let action = request(&case);
    for (enemy, field) in [(&enemy, Stat::Evasion), (&original_enemy, Stat::Delay1)] {
        let error = evaluate_ws(&player, enemy, &action, &mut ctx).unwrap_err();
        let CalcError::Legacy { operation, category, message, .. } = error else {
            panic!("expected a source required-key error");
        };
        assert_eq!(operation, "average_ws");
        assert_eq!(category, "KeyError");
        assert_eq!(message, format!("'{}'", field.name()));
    }
    ctx.random.assert_finished().unwrap();
}

#[test]
fn magic_and_hybrid_ws_do_not_inherit_spell_burst_or_resist_rank_rules() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    for path in ["combat/023432.json", "combat/023443.json"] {
        let case = document(&fixture_root().join(path));
        let tape = ScriptedTape::from_json_records(
            &case["expected"]["value"]["scripted"]["tape"],
        ).unwrap();
        let mut ctx = EvaluationContext::new(Arc::clone(&catalog), tape.clone());
        let mut player = build_boundary(&case, &mut ctx).unwrap();
        let enemy = boundary_enemy(&case);
        let action = request(&case);
        let ordinary = sample_ws(&player, &enemy, &action, true, &mut ctx).unwrap();
        ctx.random.assert_finished().unwrap();
        player.abilities.set(Ability::MagicBurst, true);
        player.abilities.set_resist_rank(Arc::from("not a spell resist rank"));
        for stat in [Stat::MagicBurstAccuracy, Stat::MagicBurstDamage,
            Stat::MagicBurstDamageIi, Stat::MagicBurstDamageTrait] {
            player.stats.set(stat, 10000.0);
        }
        ctx.random = tape;
        let burst = sample_ws(&player, &enemy, &action, true, &mut ctx).unwrap();
        ctx.random.assert_finished().unwrap();
        assert_eq!(ordinary.damage.to_bits(), burst.damage.to_bits());
        assert_eq!(ordinary.tp_return.to_bits(), burst.tp_return.to_bits());
        assert_eq!(ordinary.physical_damage.to_bits(), burst.physical_damage.to_bits());
        assert_eq!(ordinary.magical_damage.to_bits(), burst.magical_damage.to_bits());
    }
}
