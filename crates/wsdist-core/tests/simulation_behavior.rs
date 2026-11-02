#[path = "search_support/mod.rs"]
#[allow(dead_code)]
mod support;
mod simulation_support;
#[allow(dead_code)]
mod native_support;
#[allow(dead_code)]
mod optimizer_support;

use wsdist_fixtures::Value;
use support::{fixture, fixture_root, index};

fn replay_all(operation: &str, replay: fn(&Value)) -> usize {
    let mut count = 0;
    for entry in index().cases.into_iter().filter(|entry| entry.operation == operation) {
        let case = fixture(&fixture_root(), &entry);
        assert_eq!(case["id"], entry.id, "frozen index identity");
        assert_eq!(case["operation"], entry.operation, "frozen index operation");
        replay(&case);
        count += 1;
    }
    count
}

#[test]
fn all_twenty_full_7200_trajectories_match_native_goldens_and_legacy_inputs() {
    assert_eq!(replay_all("simulation_dps", simulation_support::replay_dps), 20,
        "every frozen single/dual/H2H/both-set timing/Regain/EnSpell trajectory");
}

#[test]
fn all_twelve_20000_sample_distributions_match_native_goldens_and_legacy_players() {
    assert_eq!(replay_all("simulation_distribution", simulation_support::replay_distribution), 12,
        "every physical/magical/hybrid/single-only/Fotia/Conserve/capped/resist distribution");
}

#[test]
fn frozen_proven_nonterminating_state_is_rejected_without_sampling() {
    assert_eq!(replay_all("simulation_nonprogress", simulation_support::replay_nonprogress), 1,
        "the source diagnostic round is separate from its intentionally uninvoked infinite driver");
}

#[test]
fn cancellation_precedes_player_construction_and_random_draws() {
    simulation_support::boundaries::cancellation_precedes_builds_or_random_draws();
}

#[test]
fn cancellation_between_tp_and_ws_never_dispatches_the_ws() {
    simulation_support::boundaries::cancellation_after_a_round_never_enters_the_following_ws();
}

#[test]
fn cancellation_after_a_ws_never_returns_a_partial_distribution() {
    simulation_support::boundaries::cancellation_in_a_ws_returns_no_partial_distribution();
}

#[test]
fn repeated_missed_rounds_are_not_a_no_progress_proof_and_remain_interruptible() {
    simulation_support::boundaries::unlucky_zero_tp_rounds_continue_until_cancellation();
}

#[test]
fn rounded_regain_only_trajectory_reaches_weapon_skills() {
    simulation_support::boundaries::rounded_regain_alone_can_reach_a_ws();
}

#[test]
fn reachable_daken_tp_is_included_in_the_no_progress_bound() {
    simulation_support::boundaries::reachable_daken_prevents_false_nonprogress();
}

#[test]
fn reachable_sam_zanshin_tp_is_included_in_the_no_progress_bound() {
    simulation_support::boundaries::reachable_sam_zanshin_prevents_false_nonprogress();
}

#[test]
fn zero_duration_tp_rounds_with_reachable_tp_finish_the_full_horizon() {
    simulation_support::boundaries::zero_duration_rounds_with_positive_tp_complete_the_horizon();
}

#[test]
fn disabling_traces_does_not_change_the_trajectory_or_random_consumption() {
    simulation_support::boundaries::trace_disabled_keeps_the_same_damage_and_streams_without_events();
}

#[test]
fn unreachable_zanshin_or_daken_cannot_mask_a_proven_no_tp_loop() {
    simulation_support::boundaries::unreachable_special_hits_do_not_mask_nonprogress();
}

#[test]
fn ws_only_threshold_keeps_empty_tp_averages_as_nonfinite_results() {
    use std::sync::atomic::AtomicBool;
    use wsdist_core::simulation::simulate_dps;
    let case = wsdist_fixtures::read_value(&fixture_root().join("search_simulation/000399.bitcode.zst"), None).unwrap();
    let frozen = support::FrozenCatalog::from_input(&case["input"]);
    let mut ctx = frozen.context(&case["input"]["seeds"]);
    let mut request = simulation_support::dps_request(&frozen, &case["input"], &ctx, "WS-only threshold");
    request.threshold = 0.0;
    let result = simulate_dps(&request, &mut ctx, &AtomicBool::new(false)).unwrap();
    assert_eq!(result.round_count, 0);
    assert_eq!(result.ws_count, 3600);
    assert_eq!(result.final_time, 7200.0);
    assert_eq!(result.tp_damage, 0.0);
    assert_eq!(result.damage, result.ws_damage);
    assert!(result.tp_series.is_empty());
    assert!(result.events.iter().all(|event| event.kind == wsdist_core::model::SimulationEventKind::WeaponSkill));
    let serialized = wsdist_fixtures::to_value(result).unwrap();
    let nan = wsdist_fixtures::to_value(wsdist_core::model::LegacyF64(f64::NAN)).unwrap();
    for field in ["average_tp_damage", "average_physical_round_damage", "average_magical_round_damage"] {
        assert_eq!(serialized[field], nan, "{field}: empty NumPy reduction is not null or fabricated zero");
    }
}
