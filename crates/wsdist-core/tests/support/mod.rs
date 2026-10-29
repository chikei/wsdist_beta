//! Shared executable/test parity driver. The numerical engine remains the
//! production crate; these modules only decode frozen inputs and compare output.
pub mod catalog;
pub mod independent;
mod fixture_json;
mod kernels;
mod search;
mod specs;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wsdist_core::model::{EvaluationContext, Player, build_player};
use wsdist_core::Catalog;

use crate::{buffs_support, combat_support, kernel_support, player_support, simulation_support, support, ws_support};

#[derive(Deserialize)]
struct Entry { id: String, operation: String, path: String, sha256: String }
#[derive(Deserialize)]
struct Index { schema_version: u64, cases: Vec<Entry> }

#[derive(Default, Clone, Serialize)]
pub struct Work {
    pub player_builds: usize,
    pub kernel_rows: usize,
    pub strict_kernel_tape_rows: usize,
    pub recorded_actions: usize,
    pub scripted_actions: usize,
    pub hit_records: usize,
    pub uniform_draws: usize,
    pub legacy_errors: usize,
    pub optimizer_cases: usize,
    pub full_optimizer_builds: usize,
    pub public_seed11_optimizer_runs: usize,
    pub dps_trajectories: usize,
    pub dps_actions: usize,
    pub distributions: usize,
    pub distribution_samples: usize,
    pub buff_enemy_snapshots: usize,
    pub profile_player_builds: usize,
}

impl Work {
    fn merge(&mut self, other: &Self) {
        self.player_builds += other.player_builds;
        self.kernel_rows += other.kernel_rows;
        self.strict_kernel_tape_rows += other.strict_kernel_tape_rows;
        self.recorded_actions += other.recorded_actions;
        self.scripted_actions += other.scripted_actions;
        self.hit_records += other.hit_records;
        self.uniform_draws += other.uniform_draws;
        self.legacy_errors += other.legacy_errors;
        self.optimizer_cases += other.optimizer_cases;
        self.full_optimizer_builds += other.full_optimizer_builds;
        self.public_seed11_optimizer_runs += other.public_seed11_optimizer_runs;
        self.dps_trajectories += other.dps_trajectories;
        self.dps_actions += other.dps_actions;
        self.distributions += other.distributions;
        self.distribution_samples += other.distribution_samples;
        self.buff_enemy_snapshots += other.buff_enemy_snapshots;
        self.profile_player_builds += other.profile_player_builds;
    }
}

#[derive(Default, Serialize)]
pub struct CorpusReport {
    pub passed: bool,
    pub indexed_cases: usize,
    pub passed_cases: usize,
    pub failed_cases: usize,
    pub exercised_cases: usize,
    pub evidence_only_cases: usize,
    pub operations: BTreeMap<String, usize>,
    pub matched_coverage_tags: BTreeSet<String>,
    pub observed_work: Work,
    pub numeric_comparisons: player_support::observation::Measurements,
    pub cases: Vec<Value>,
    pub failing_case_paths: Vec<Value>,
    pub catalog: Value,
    pub provenance: Value,
    pub limitations: Vec<String>,
    #[serde(skip)]
    pub manifest: Value,
}

pub fn resource_root() -> PathBuf { player_support::resource_root() }
pub fn fixture_root() -> PathBuf { player_support::core_fixtures() }

pub fn panic_message(error: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = error.downcast_ref::<String>() { message.clone() }
    else if let Some(message) = error.downcast_ref::<&str>() { (*message).to_owned() }
    else { "non-string assertion panic".to_owned() }
}

fn start_measurements() {
    player_support::observation::begin();
    combat_support::observation::begin();
    support::observation::begin();
    kernel_support::observation::begin();
    ws_support::observation::begin();
    buffs_support::observation::begin();
}

fn finish_measurements() -> player_support::observation::Measurements {
    let mut result = player_support::observation::Measurements::default();
    for value in [player_support::observation::take(), combat_support::observation::take(),
        support::observation::take(), kernel_support::observation::take(), ws_support::observation::take(), buffs_support::observation::take()]
    { result.merge(value); }
    result
}

fn load_case(root: &Path, entry: &Entry) -> Value {
    let relative = Path::new(&entry.path);
    assert!(relative.components().all(|component| matches!(component, Component::Normal(_))), "{}: fixture path is not relative", entry.id);
    let path = root.join(relative);
    let case = fixture_json::read::<Value>(&path, Some(&entry.sha256)).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(case["id"], entry.id, "{}: index identity", path.display());
    assert_eq!(case["operation"], entry.operation, "{}: operation", path.display());
    assert_eq!(case["expected"]["outcome"], "result", "{}: capture process must have completed", path.display());
    case
}

fn replay(case: &Value, catalog: &Arc<Catalog>, work: &mut Work) {
    let label = case["id"].as_str().unwrap();
    match case["operation"].as_str().unwrap() {
        "player_build" | "player_sequence" => {
            let mut ctx = EvaluationContext::new(Arc::clone(catalog), player_support::RejectRandom);
            ctx.player_stages = Some(Vec::with_capacity(5));
            let mut player = Player::new([catalog.empty; 16]);
            let sequence = case["operation"] == "player_sequence";
            let inputs = if sequence { case["input"]["players"].as_array().unwrap().iter().collect::<Vec<_>>() }
                else { vec![&case["input"]] };
            let expected = if sequence { case["expected"]["value"]["players"].as_array().unwrap().iter().collect::<Vec<_>>() }
                else { vec![&case["expected"]["value"]] };
            assert_eq!(inputs.len(), expected.len(), "{label}: complete state-transition sequence");
            for (index, (input, expected)) in inputs.into_iter().zip(expected).enumerate() {
                let path = format!("{label}.players[{index}]");
                let before = player_support::effective_skills(catalog, &ctx.state);
                let input = player_support::parse_input(input, catalog, &ctx.state, &path);
                let result = build_player(&input, &mut ctx, &mut player);
                player_support::compare_build(&result, &player, &ctx, &before, expected, &path);
                work.player_builds += 1;
                work.legacy_errors += usize::from(result.is_err());
            }
            let isolated = EvaluationContext::new(Arc::clone(catalog), player_support::RejectRandom);
            assert_eq!(player_support::effective_skills(catalog, &isolated.state), catalog.pieces().iter().map(|piece| piece.skill_type).collect::<Vec<_>>(), "{label}: catalog/context isolation");
        }
        "kernel_group" | "kernel_tapes" | "kernel_tables" | "rounding" => {
            let (rows, tapes) = kernels::replay(case);
            work.kernel_rows += rows;
            work.strict_kernel_tape_rows += tapes;
        }
        "combat.metadata" => specs::metadata(case),
        "combat.weapon_bonus" => specs::weapon_bonus(case),
        "combat.round" | "combat.spell" | "combat.ws" => {
            let result = combat_support::replay_case(case, catalog);
            work.player_builds += 1;
            work.recorded_actions += result.recorded;
            work.scripted_actions += result.scripted;
            work.hit_records += result.hits;
            work.uniform_draws += result.uniform_draws;
            work.legacy_errors += result.legacy_errors;
            if case["operation"] == "combat.ws" {
                let ctx = EvaluationContext::new(Arc::clone(catalog), ws_support::NoSetupDraws);
                let result = ws_support::evaluate_case(case, &ctx);
                ws_support::verify_case(case, &result);
            }
        }
        "simulation_dps" => {
            let (samples, hits, draws) = simulation_support::replay_dps_measured(case);
            work.dps_trajectories += 1;
            work.dps_actions += samples;
            work.hit_records += hits;
            work.uniform_draws += draws;
        }
        "simulation_distribution" => {
            let (samples, hits, draws) = simulation_support::replay_distribution_measured(case);
            work.distributions += 1;
            work.distribution_samples += samples;
            work.hit_records += hits;
            work.uniform_draws += draws;
        }
        "simulation_nonprogress" => simulation_support::replay_nonprogress(case),
        operation if operation.starts_with("optimizer_") => {
            search::replay(case);
            work.optimizer_cases += 1;
            if operation == "optimizer_build" {
                work.full_optimizer_builds += 1;
                work.public_seed11_optimizer_runs += usize::from(label.contains("seed11-complete-build"));
            }
        }
        other => panic!("{label}: no adapter for frozen operation {other}"),
    }
}

fn record_case(report: &mut CorpusReport, entry: &Entry, root: &Path, catalog: &Arc<Catalog>, ui: bool) {
    let timer = Instant::now();
    let mut work = Work::default();
    let mut tags = Vec::new();
    start_measurements();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let case = load_case(root, entry);
        if ui { replay_ui_numeric(&case, catalog, &mut work); }
        else if entry.operation != "player_tables" { replay(&case, catalog, &mut work); }
        tags = case.get("coverage").and_then(Value::as_array).map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
    }));
    let measured = finish_measurements();
    report.numeric_comparisons.merge(serde_json::to_value(&measured).unwrap());
    report.observed_work.merge(&work);
    report.indexed_cases += 1;
    let path = root.join(&entry.path).display().to_string();
    let failure = result.err().map(panic_message);
    let evidence_only = entry.operation == "player_tables"
        || (ui && work.buff_enemy_snapshots == 0 && work.profile_player_builds == 0);
    if let Some(diagnostic) = &failure {
        report.failed_cases += 1;
        report.failing_case_paths.push(json!({"path": path, "id": entry.id, "diagnostic": diagnostic}));
    } else {
        report.passed_cases += 1;
        if evidence_only { report.evidence_only_cases += 1; } else { report.exercised_cases += 1; }
        *report.operations.entry(entry.operation.clone()).or_default() += 1;
        report.matched_coverage_tags.extend(tags);
    }
    report.cases.push(json!({"id": entry.id, "path": path, "operation": entry.operation,
        "status": if failure.is_some() { "failed" } else if evidence_only { "evidence_hash_verified" } else { "passed" }, "failure": failure,
        "elapsed_ms": timer.elapsed().as_secs_f64() * 1000.0, "observed_work": work, "numeric_comparisons": measured,
        "scope": if ui { "core effective-input, buff, enemy and player checks only; controller/UI checked by app suites" } else { "core parity" }}));
}

fn replay_ui_numeric(case: &Value, catalog: &Arc<Catalog>, work: &mut Work) {
    let label = case["id"].as_str().unwrap();
    let mut states = Vec::new();
    buffs_support::states(&case["expected"], &mut states);
    for (index, state) in states.into_iter().enumerate() {
        let path = format!("{label}.state[{index}]");
        let actual = wsdist_core::buffs::aggregate_buffs(&buffs_support::controls(state), catalog).unwrap();
        let expected = &state["aggregate_buffs"];
        assert_eq!(actual.buckets.iter().map(|bucket| bucket.name.as_ref()).collect::<Vec<_>>(), expected["active_buffs"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), "{path}: bucket order");
        for bucket in &actual.buckets { buffs_support::assert_stats(&bucket.stats, &expected["active_buffs"][bucket.name.as_ref()], &path); }
        buffs_support::assert_stats(&actual.enemy_debuffs, &expected["combined_debuffs"], &path);
        if state["enemy_input"].is_object() {
            let enemy = wsdist_core::buffs::build_enemy(&buffs_support::enemy(state), &actual).unwrap();
            buffs_support::assert_stats(&enemy.stats, &state["normalized_enemy"], &path);
        } else { assert!(state.get("gather_player_inputs_error").is_some(), "{path}: absent input must be a captured rejection"); }
        work.buff_enemy_snapshots += 1;
    }
    if case["operation"] == "ui.profile" {
        let mut ctx = EvaluationContext::new(Arc::clone(catalog), player_support::RejectRandom);
        ctx.player_stages = Some(Vec::with_capacity(5));
        let mut player = Player::new([catalog.empty; 16]);
        for (index, expected) in case["expected"]["value"]["player_builds"].as_array().unwrap().iter().enumerate() {
            let path = format!("{label}.player_builds[{index}]");
            let input = player_support::parse_input(&expected["input"], catalog, &ctx.state, &path);
            let result = build_player(&input, &mut ctx, &mut player);
            player_support::assert_error(&result, expected.get("error"), &path);
            player_support::compare_profile_build(&player, &ctx, expected, &path);
            work.profile_player_builds += 1;
            work.legacy_errors += usize::from(result.is_err());
        }
    }
}

pub fn verify_corpus(root: &Path) -> CorpusReport {
    let mut report = CorpusReport::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let manifest = player_support::document(&root.join("manifest.json"));
        assert_eq!(manifest["capture_complete"], true, "canonical capture must be complete");
        report.provenance = json!({"manifest_sha256": catalog::hash_file(&root.join("manifest.json")),
            "source": manifest["source"], "oracle_runtime": manifest["runtime"],
            "capture_program_hashes": manifest["capture_program_hashes"], "catalog_hashes": manifest["catalog_hashes"],
            "baseline_tests": manifest["baseline_tests"],
            "note": "Oracle hashes are frozen provenance, not a claim that Python or its source is executed by this verifier."});
        let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
        start_measurements();
        let catalog_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| catalog::verify(&resource_root(), &catalog, &manifest)));
        report.numeric_comparisons.merge(serde_json::to_value(finish_measurements()).unwrap());
        match catalog_result {
            Ok(value) => report.catalog = value,
            Err(error) => report.failing_case_paths.push(json!({"path": "catalog", "diagnostic": panic_message(error)})),
        }
        let mut identities = BTreeSet::new();
        for (name, count_name) in [("players.json", "capture_players"), ("combat.json", "capture_combat"), ("search_simulation.json", "capture_search_simulation")] {
            let index: Index = serde_json::from_value(player_support::document(&root.join(name))).unwrap();
            assert_eq!(index.schema_version, 1, "{name}: supported fixture schema");
            assert_eq!(index.cases.len() as u64, manifest["fixture_counts"][count_name].as_u64().unwrap(), "{name}: complete frozen family");
            for entry in index.cases {
                assert!(identities.insert(entry.id.clone()), "{}: duplicate fixture identity", entry.id);
                record_case(&mut report, &entry, root, &catalog, false);
            }
        }
        // The UI subset contains actual Qt-restored numerical inputs. This is
        // not a substitute for the separately executable app/controller gate.
        let ui_root = player_support::app_fixtures();
        let ui_index: Index = serde_json::from_value(player_support::document(&ui_root.join("ui.json"))).unwrap();
        assert_eq!(ui_index.cases.len() as u64, manifest["fixture_counts"]["capture_ui"].as_u64().unwrap(), "complete effective-input UI subset");
        for entry in ui_index.cases { record_case(&mut report, &entry, &ui_root, &catalog, true); }
        report.manifest = manifest;
    }));
    if let Err(error) = result { report.failing_case_paths.push(json!({"path": root.display().to_string(), "diagnostic": panic_message(error)})); }
    report.limitations.push("The player_tables fixture is inert provenance; its numerical consumers are verified through all captured raw/staged builds, not Python table execution.".to_owned());
    report.limitations.push("UI-only catalog filters, profiles and controller interactions remain app-suite/UI-smoke coverage; this binary verifies their captured effective numerical inputs only.".to_owned());
    report.passed = report.failed_cases == 0 && report.failing_case_paths.is_empty();
    report
}
