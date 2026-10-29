#[path = "legacy_player/support.rs"]
#[allow(dead_code)]
mod player_support;
#[path = "numerical_boundaries/support.rs"]
#[allow(dead_code)]
mod kernel_support;
#[path = "combat_support/mod.rs"]
#[allow(dead_code)]
mod combat_support;
#[path = "ws_support/mod.rs"]
#[allow(dead_code)]
mod ws_support;
#[path = "buffs_support/mod.rs"]
#[allow(dead_code)]
mod buffs_support;
#[allow(dead_code)]
mod native_support;
#[path = "search_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "optimizer_support/mod.rs"]
#[allow(dead_code)]
mod optimizer_support;
#[path = "simulation_support/mod.rs"]
#[allow(dead_code)]
mod simulation_support;
#[path = "support/mod.rs"]
mod verification;

#[test]
fn entire_frozen_corpus_preserves_stats_actions_search_trajectories_distributions_and_random_protocols() {
    let report = verification::verify_corpus(&verification::fixture_root());
    assert!(report.passed, "canonical parity failures: {}", serde_json::to_string_pretty(&report.failing_case_paths).unwrap());
    // These are completed production work counts, not fixture-index lengths.
    assert_eq!(report.observed_work.dps_trajectories, 20);
    assert_eq!(report.observed_work.distributions, 12);
    assert_eq!(report.observed_work.distribution_samples, 240_000);
    assert_eq!(report.observed_work.full_optimizer_builds, 30);
    assert_eq!(report.observed_work.public_seed11_optimizer_runs, 3);
    assert_eq!(report.observed_work.profile_player_builds, 577);
    assert_eq!(report.observed_work.scripted_actions, 112);
}

#[test]
fn independent_conditioning_integration_attempt_trees_and_staged_truncation_keep_legacy_discrepancies() {
    // This executes production analytical/sampled helpers and actual action
    // boundaries. The independent expectations do not call those formulas.
    let diagnostics = verification::independent::diagnostics(&verification::fixture_root());
    let capped = diagnostics.iter().find(|row| row["id"] == "pdif.capped_sword").unwrap();
    assert_eq!(capped["legacy_analytical"], serde_json::json!(3.33125));
    player_support::assert_number(capped["independent_sampled_expectation"].as_f64().unwrap(), &serde_json::json!(3.23515625), "pdif.capped_sword independent integral");
    let cap = diagnostics.iter().find(|row| row["id"] == "multiattack.eight_attempts").unwrap();
    assert_eq!(cap["legacy_analytical_landed_hits"], serde_json::json!(5.0));
    assert_eq!(cap["independent_sampled_landed_hits"], serde_json::json!(4.0));
    let ninjutsu = diagnostics.iter().find(|row| row["id"] == "ninjutsu.staged_truncation").unwrap();
    assert_eq!(ninjutsu["legacy_analytical"], serde_json::json!(109.0));
}

#[test]
fn million_sample_isolated_legacy_samplers_match_independent_expectations_with_six_standard_error_gates() {
    let results = verification::independent::statistics(&verification::fixture_root());
    for result in &results {
        assert_eq!(result["completed"], true, "{}: incomplete sampler: {result}", result["id"]);
        assert_eq!(result["n"], 1_000_000, "{}: independent fixed-seed sample size", result["id"]);
        assert_eq!(result["batch_count"], 40, "{}: fixed batch seeds", result["id"]);
        assert_eq!(result["passed"], true, "{}: independent expectation/gate mismatch: {result}", result["id"]);
    }
}
