use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wsdist_app::exchange::GearSource;
use wsdist_app::jobs::{
    AnalyticalKind, GuiAction, JobCompletion, WorkRequest, WorkResult, WorkerCoordinator,
    WorkerError, WorkerEvent,
};
use wsdist_app::state::CalculationInputs;
use wsdist_core::Catalog;
use wsdist_core::model::{
    CalcError, CalculationStep, CandidatePools, Enemy, EvaluationContext, HitEvent, OptimizeOptions,
    Player, PlayerStage, PlayerStageSnapshot, Progress, RandomSource, RandomStream,
    SkillType, Slot, WsType,
};
use wsdist_core::rng::RandomSources;

#[path = "../../wsdist-core/tests/legacy_player/support.rs"]
#[allow(dead_code)]
mod support;
use support::{assert_number, document, number};

type Engine = EvaluationContext<RandomSources>;
const COMPLETION_DEADLINE: Duration = Duration::from_secs(30);

fn catalog() -> Arc<Catalog> {
    static CATALOG: LazyLock<Arc<Catalog>> = LazyLock::new(|| {
        Arc::new(Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap())
    });
    Arc::clone(&CATALOG)
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../wsdist-core/tests/fixtures/legacy")
}

fn frozen(index: &str, id: &str) -> Value {
    let root = fixture_root();
    let index = document(&root.join(index));
    let entry = index["cases"].as_array().unwrap().iter()
        .find(|entry| entry["id"] == id).unwrap_or_else(|| panic!("uncaptured scenario {id}"));
    support::fixture(&root, entry)
}

fn native(id: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wsdist-core/tests/fixtures/native");
    let index = document(&root.join("index.json"));
    let entry = index["cases"].as_array().unwrap().iter()
        .find(|entry| entry["id"] == id).unwrap_or_else(|| panic!("uncaptured native scenario {id}"));
    support::fixture(&root, entry)["observation"].clone()
}

fn combat(id: &str) -> Value { frozen("combat.json", id) }
fn search(id: &str) -> Value { frozen("search_simulation.json", id) }
fn sequence() -> Value { frozen("players.json", "player.state.h2h_then_shared_empty") }

// The outer deadline also catches a blocking poll, shutdown, or Drop. No Slint
// backend/event loop is installed: a worker cannot depend on a synchronous UI reply.
fn bounded(test: impl FnOnce() + Send + 'static) {
    let (send, receive) = mpsc::channel::<Result<(), Box<dyn Any + Send>>>();
    let handle = thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
        let _ = send.send(outcome);
    });
    let outcome = receive.recv_timeout(COMPLETION_DEADLINE)
        .expect("coordinator lifecycle exceeded its completion deadline");
    handle.join().expect("lifecycle owner must terminate after returning its context");
    if let Err(payload) = outcome { std::panic::resume_unwind(payload); }
}

fn finish(worker: &mut WorkerCoordinator) -> (JobCompletion, Vec<Progress>) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut progress = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "worker failed to publish completion at a safe point");
        match worker.poll() {
            Some(WorkerEvent::Progress(value)) => {
                assert!(worker.is_busy(), "progress must not release the owned computation context");
                progress.push(value);
            }
            Some(WorkerEvent::Finished(completion)) => {
                assert!(!worker.is_busy(), "only terminal delivery releases the active job");
                assert!(worker.poll().is_none(), "a completion must be delivered exactly once");
                return (completion, progress);
            }
            None => thread::sleep(Duration::from_millis(1)),
        }
    }
}

fn first_progress(worker: &mut WorkerCoordinator) -> Progress {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "search did not publish scored progress");
        match worker.poll() {
            Some(WorkerEvent::Progress(progress)) if progress.scored > 0 => return progress,
            Some(WorkerEvent::Progress(_)) | None => thread::sleep(Duration::from_millis(1)),
            Some(WorkerEvent::Finished(completion)) => match completion.outcome {
                Err(error) => panic!("search failed before scored progress: {error}"),
                Ok(_) => panic!("search completed without exposing its scored batches"),
            },
        }
    }
}

fn combat_inputs(case: &Value) -> (CalculationInputs, Engine) {
    let seed = &case["input"]["random"];
    let context = EvaluationContext::new(catalog(), RandomSources::from_seed(
        seed["numpy_host_seed"].as_u64().unwrap(),
    ));
    let source = &case["input"]["player"];
    assert!(source["stat_overrides"].as_object().unwrap().is_empty(),
        "worker fixtures must enter through build_player, not post-build stat overrides");
    let player = support::parse_input(source, &context.catalog, &context.state, "worker input");
    let action = &case["input"]["action"];
    let inputs = CalculationInputs {
        player,
        enemy: Enemy {
            name: Arc::from("Apex Toad"), location: Arc::from("captured"),
            stats: serde_json::from_value(case["expected"]["value"]["enemy_at_action_boundary"].clone()).unwrap(),
        },
        ws_name: Arc::from(action["name"].as_str().unwrap_or("Savage Blade")),
        ws_type: WsType::from_name(action["ws_type"].as_str().unwrap_or("melee")).unwrap(),
        spell_name: Arc::from("None"),
        tp: number(action.get("tp").unwrap_or(&action["ws_threshold"])),
    };
    (inputs, context)
}

fn search_inputs(case: &Value) -> (CalculationInputs, Engine) {
    let input = &case["input"];
    let seed = &input["seeds"];
    let context = EvaluationContext::new(catalog(), RandomSources::from_seed(
        seed["numpy_host"].as_u64().unwrap(),
    ));
    let source = input.get("player").unwrap_or(&input["tp_player"]);
    let player = support::parse_input(source, &context.catalog, &context.state, "search input");
    let request = &input["request"];
    (CalculationInputs {
        player,
        enemy: Enemy { name: Arc::from("Apex Toad"), location: Arc::from("captured"),
            stats: serde_json::from_value(input["enemy"]["normalized_stats"].clone()).unwrap() },
        ws_name: Arc::from(request["ws_name"].as_str().unwrap()),
        ws_type: WsType::from_name(request["ws_type"].as_str().unwrap()).unwrap(),
        spell_name: Arc::from("None"),
        tp: number(request.get("min_tp").or_else(|| request.get("input_tp"))
            .unwrap_or(&request["ws_threshold"])),
    }, context)
}

fn optimize_action(case: &Value, catalog: &Catalog) -> GuiAction {
    let input = &case["input"];
    let candidates: CandidatePools = std::array::from_fn(|index| {
        input["candidate_pools"][Slot::ALL[index].name()].as_array().unwrap().iter()
            .map(|piece| catalog.by_original_id(piece["id"].as_u64().unwrap() as u32).unwrap()).collect()
    });
    GuiAction::Optimize {
        kind: AnalyticalKind::Ws, metric: Arc::from(input["request"]["input_metric"].as_str().unwrap()),
        candidates, min_pdt: number(&input["thresholds"]["pdt"]),
        min_mdt: number(&input["thresholds"]["mdt"]),
        options: serde_json::from_value::<OptimizeOptions>(input["options"].clone()).unwrap(),
        similar_results: false, similar_percent: number(&input["next_best_percent"]),
        verbose_swaps: false,
    }
}

fn cancellation_action(case: &Value, catalog: &Catalog) -> GuiAction {
    let mut action = optimize_action(case, catalog);
    let GuiAction::Optimize { candidates, options, min_pdt, min_mdt, .. } = &mut action else { unreachable!() };
    // Duplicate entries are a source-supported pool invariant. Three 96-entry
    // axes cross the production 20000-candidate pool threshold; two real scoring
    // workers and 100 restarts exercise cancellation beyond a tiny local search.
    // The reference stress fixture asks for impossible 200% DT; cancellation
    // must instead enter a feasible search, not the zero-scored failure path.
    *min_pdt = 0.0;
    *min_mdt = 0.0;
    for pool in candidates.iter_mut().filter(|pool| !pool.is_empty()) {
        *pool = pool.iter().copied().cycle().take(96).collect();
    }
    options.seed = None;
    options.workers = 2;
    options.restart_count = 100;
    action
}

fn assert_player(player: &Player, expected: &Value, label: &str) {
    support::assert_metadata(player, expected, label);
    support::assert_sheet(&player.stats, &player.wsc, player.wyvern_bonus_attack, expected, label);
}

fn assert_action(actual: &wsdist_core::model::ActionResult, expected: &Value) {
    for (field, actual) in [("damage", actual.damage), ("tp_return", actual.tp_return),
        ("metric", actual.metric), ("physical_damage", actual.physical_damage),
        ("magical_damage", actual.magical_damage)] {
        assert_number(actual, &expected[field], field);
    }
    assert_eq!(actual.invert as i64, expected["invert"].as_i64().unwrap());
}

fn stats_request(quicklook: CalculationInputs) -> WorkRequest {
    let input = quicklook.player.clone();
    WorkRequest { quicklook, action: GuiAction::Stats { source: GearSource::Quicklook, input } }
}

#[test]
fn busy_start_returns_the_supplied_context_intact_and_success_releases_only_once() {
    bounded(|| {
        let fixture = combat("combat.000359.ws.Savage_Blade.tp1000.Damage_dealt");
        let (inputs, context) = combat_inputs(&fixture);
        let mut worker = WorkerCoordinator::new();
        worker.start(WorkRequest { quicklook: inputs.clone(), action: GuiAction::Ws }, context).unwrap();

        let mut rejected = EvaluationContext::new(catalog(), RandomSources::from_seed(20240101));
        rejected.state.set_skill(rejected.catalog.empty, SkillType::HandToHand);
        rejected.random.uniform(RandomStream::Primary, 0.0, 1.0);
        rejected.random.uniform(RandomStream::Pdif, 0.0, 1.0);
        rejected.scratch.oa_main[3] = 0.625;
        rejected.scratch.hit_rates[1][0] = 0.875;
        rejected.action_trace = Some(Vec::with_capacity(9));
        rejected.player_stages = Some(Vec::with_capacity(5));
        let trace_address = rejected.action_trace.as_ref().unwrap().as_ptr();
        let stage_address = rejected.player_stages.as_ref().unwrap().as_ptr();
        let rng = rejected.random.clone();
        let generation = rejected.state.generation();
        let resource = Arc::clone(&rejected.catalog);
        let failure = worker.start(stats_request(inputs.clone()), rejected).unwrap_err();
        assert!(matches!(failure.error, WorkerError::Busy));
        assert!(Arc::ptr_eq(&failure.context.catalog, &resource));
        assert_eq!(failure.context.random, rng);
        assert_eq!(failure.context.state.generation(), generation);
        assert_eq!(failure.context.state.override_for(resource.empty), Some(SkillType::HandToHand));
        assert_eq!(failure.context.scratch.oa_main[3], 0.625);
        assert_eq!(failure.context.scratch.hit_rates[1][0], 0.875);
        assert_eq!(failure.context.action_trace.as_ref().unwrap().as_ptr(), trace_address);
        assert_eq!(failure.context.player_stages.as_ref().unwrap().as_ptr(), stage_address);

        let (completion, _) = finish(&mut worker);
        let WorkResult::Analytical { kind, player, result, display, .. } = completion.outcome.unwrap() else {
            panic!("WS calculation must return an analytical result");
        };
        assert_eq!(kind, AnalyticalKind::Ws);
        assert_player(&player, &fixture["expected"]["value"]["player_at_action_boundary"], "WS worker player");
        assert_action(&result, &fixture["expected"]["value"]["compiled"]["fields"]);
        assert_eq!((display.damage.as_str(), display.tp_return.as_str()), ("668", "18.3"));

        let rng = completion.context.random.clone();
        worker.start(stats_request(inputs), completion.context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::Stats { player, .. } = completion.outcome.unwrap() else { panic!("next job lost its result"); };
        assert_player(&player, &fixture["expected"]["value"]["player_at_action_boundary"], "next job player");
        assert_eq!(completion.context.random, rng, "Stats must preserve the recovered random streams");
    });
}

#[test]
fn single_ws_recovers_both_advanced_streams_and_restores_the_callers_observers() {
    bounded(|| {
        let fixture = combat("combat.019404.ws.Savage_Blade.sample.tp1000.seed11.singleTrue");
        let (inputs, mut context) = combat_inputs(&fixture);
        let retained_hit = HitEvent { kind: wsdist_core::model::AttackKind::Main,
            proc: wsdist_core::model::AttackProc::Normal, attempt: 1, landed: true, critical: false,
            physical_damage: 1918.992683330928, magical_damage: 0.0, tp_return: 75.0 };
        context.action_trace = Some(vec![retained_hit]);
        context.player_stages = Some(vec![PlayerStageSnapshot {
            stage: PlayerStage::Final, stats: serde_json::from_value(
                fixture["expected"]["value"]["player_at_action_boundary"]["stats"].clone()).unwrap(),
            wsc: Vec::new(), wyvern_bonus_attack: false,
            skills: std::array::from_fn(|index| SkillType::from_name(
                fixture["expected"]["value"]["player_at_action_boundary"]["gearset"]
                    [Slot::ALL[index].name()]["skill_type"].as_str().unwrap()).unwrap()),
        }]);
        context.calculation_trace = Some(vec![CalculationStep {
            label: "Retained observer", equation: Some("Prior = retained value"),
            formula: "prior = {prior}", values: vec![("prior", 37.5)],
        }]);
        let saved_calculation = context.calculation_trace.clone();
        let saved_trace = serde_json::to_value(context.action_trace.as_ref().unwrap()).unwrap();
        let saved_stats = serde_json::to_value(&context.player_stages.as_ref().unwrap()[0].stats).unwrap();
        let mut worker = WorkerCoordinator::new();
        worker.start(WorkRequest { quicklook: inputs.clone(), action: GuiAction::SingleWs }, context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::SingleWs { sample, events, report, .. } = completion.outcome.unwrap() else {
            panic!("single WS must not replace the analytical result labels");
        };
        let expected = native("combat.019404.ws.Savage_Blade.sample.tp1000.seed11.singleTrue");
        assert_eq!(serde_json::to_value(sample).unwrap(), expected["result"]);
        assert_eq!(serde_json::to_value(&events).unwrap(), expected["hits"]);
        assert!(!report.text.contains("Average Damage"));
        assert_eq!(report.text.contains("Missed"), events.iter().any(|hit| !hit.landed));
        let mut continuation = completion.context.random.clone();
        for (stream, key) in [(RandomStream::Primary, "primary"), (RandomStream::Pdif, "pdif")] {
            let draws = expected["random"]["continuation"][key].as_array().unwrap();
            assert_eq!(draws.len(), 4);
            for expected in draws {
                assert_eq!(continuation.uniform(stream, 0.0, 1.0).to_bits(), number(expected).to_bits(),
                    "recovered {key} must continue the native stream, not restart it");
            }
        }
        assert_eq!(serde_json::to_value(completion.context.action_trace.as_ref().unwrap()).unwrap(), saved_trace);
        assert_eq!(completion.context.player_stages.as_ref().unwrap().len(), 1);
        assert_eq!(serde_json::to_value(&completion.context.player_stages.as_ref().unwrap()[0].stats).unwrap(), saved_stats);
        assert_eq!(completion.context.calculation_trace, saved_calculation,
            "a displayed sampled calculation must not overwrite the caller's observer");

        let rng = completion.context.random.clone();
        worker.start(stats_request(inputs), completion.context).unwrap();
        let (completion, _) = finish(&mut worker);
        assert!(matches!(completion.outcome, Ok(WorkResult::Stats { .. })));
        assert_eq!(completion.context.random, rng);
    });
}

#[test]
fn none_spell_and_unresolved_ws_fail_after_the_current_h2h_prelude_and_allow_recovery() {
    bounded(|| {
        let fixture = sequence();
        let enemy_fixture = combat("combat.000359.ws.Savage_Blade.tp1000.Damage_dealt");
        for spell in [true, false] {
            let (mut inputs, mut context) = combat_inputs(&enemy_fixture);
            inputs.player = support::parse_input(&fixture["input"]["players"][0],
                &context.catalog, &context.state, "H2H prelude");
            inputs.ws_name = Arc::from(if spell { "Combo" } else { "unknown weapon skill" });
            context.random.uniform(RandomStream::Primary, 0.0, 1.0);
            context.random.uniform(RandomStream::Pdif, 0.0, 1.0);
            let rng = context.random.clone();
            context.action_trace = Some(Vec::with_capacity(7));
            context.player_stages = Some(Vec::with_capacity(5));
            context.calculation_trace = Some(Vec::with_capacity(3));
            let trace_address = context.action_trace.as_ref().unwrap().as_ptr();
            let stages_address = context.player_stages.as_ref().unwrap().as_ptr();
            let calculation_address = context.calculation_trace.as_ref().unwrap().as_ptr();
            let mut worker = WorkerCoordinator::new();
            worker.start(WorkRequest { quicklook: inputs.clone(),
                action: if spell { GuiAction::Spell } else { GuiAction::Ws } }, context).unwrap();
            let (completion, _) = finish(&mut worker);
            let Err(WorkerError::Calculation(CalcError::Legacy { category, field, .. })) = completion.outcome else {
                panic!("unselected/unresolved actions must surface a calculation error, not a partial result");
            };
            assert_eq!(category, if spell { "AssertionError" } else { "KeyError" });
            assert!(!field.is_empty(), "the rejected action must identify its field");
            assert_eq!(completion.context.state.override_for(completion.context.catalog.empty), Some(SkillType::HandToHand),
                "validation cannot skip the Quicklook model mutation");
            assert_eq!(completion.context.catalog.piece(completion.context.catalog.empty).skill_type, SkillType::None,
                "the recovered override must never mutate the shared catalog");
            assert_eq!(completion.context.random, rng, "resolution failure must not reset or draw from either stream");
            assert_eq!(completion.context.action_trace.as_ref().unwrap().as_ptr(), trace_address);
            assert_eq!(completion.context.player_stages.as_ref().unwrap().as_ptr(), stages_address);
            assert_eq!(completion.context.calculation_trace.as_ref().unwrap().as_ptr(), calculation_address);
            assert_eq!(EvaluationContext::new(catalog(), RandomSources::from_seed(0)).state.override_for(catalog().empty), None,
                "worker model state must not leak into independent sessions");

            inputs.player = support::parse_input(&fixture["input"]["players"][1],
                &completion.context.catalog, &completion.context.state, "shared Empty continuation");
            worker.start(stats_request(inputs), completion.context).unwrap();
            let (completion, _) = finish(&mut worker);
            let WorkResult::Stats { player, .. } = completion.outcome.unwrap() else { panic!("error prevented the next job"); };
            assert_player(&player, &fixture["expected"]["value"]["players"][1]["final"], "post-error source sequence");
            assert_eq!(completion.context.random, rng);
            assert_eq!(completion.context.state.override_for(completion.context.catalog.empty), Some(SkillType::HandToHand));
        }
    });
}

#[test]
fn target_stats_and_compare_build_after_the_unconditional_quicklook_prelude() {
    bounded(|| {
        let fixture = sequence();
        let ws_fixture = combat("combat.000359.ws.Savage_Blade.tp1000.Damage_dealt");
        for compare in [false, true] {
            let (mut quicklook, context) = combat_inputs(&ws_fixture);
            quicklook.player = support::parse_input(&fixture["input"]["players"][0],
                &context.catalog, &context.state, "current Quicklook H2H");
            let target = support::parse_input(&fixture["input"]["players"][1],
                &context.catalog, &context.state, "TP target");
            let action = if compare {
                let (ws, _) = combat_inputs(&ws_fixture);
                GuiAction::Compare { tp: target, ws: ws.player }
            } else { GuiAction::Stats { source: GearSource::Tp, input: target } };
            let mut worker = WorkerCoordinator::new();
            worker.start(WorkRequest { quicklook, action }, context).unwrap();
            let (completion, _) = finish(&mut worker);
            assert_eq!(completion.context.state.override_for(completion.context.catalog.empty), Some(SkillType::HandToHand));
            match completion.outcome.unwrap() {
                WorkResult::Stats { source, player, .. } => {
                    assert_eq!(source, GearSource::Tp);
                    assert_player(&player, &fixture["expected"]["value"]["players"][1]["final"], "Stats target after H2H");
                }
                WorkResult::Compare { tp_player, ws_player, outputs, .. } => {
                    assert_player(&tp_player, &fixture["expected"]["value"]["players"][1]["final"], "Compare TP target after H2H");
                    assert_player(&ws_player, &ws_fixture["expected"]["value"]["player_at_action_boundary"], "Compare distinct WS target");
                    assert_action(&outputs.ws_ws, &ws_fixture["expected"]["value"]["compiled"]["fields"]);
                    assert_eq!(outputs.ws_ws_damage, 667.0, "comparison uses source int truncation, not rounded display damage");
                    assert!(outputs.tp_spell.is_none() && outputs.ws_spell.is_none());
                }
                _ => panic!("target calculation returned a different operation"),
            }
        }
    });
}

#[test]
fn optimizer_progress_is_nonterminal_and_final_set_matches_the_frozen_complete_search() {
    bounded(|| {
        let fixture = search("search-simulation/seed11-complete-build-workers1");
        let (inputs, context) = search_inputs(&fixture);
        let action = optimize_action(&fixture, &context.catalog);
        let mut worker = WorkerCoordinator::new();
        worker.start(WorkRequest { quicklook: inputs, action }, context).unwrap();
        let (completion, progress) = finish(&mut worker);
        let WorkResult::Optimize { result, .. } = completion.outcome.unwrap() else { panic!("missing complete optimizer result"); };
        let observation = native("search-simulation/seed11-complete-build-workers1");
        let expected = &observation["result"];
        let player = result.player.as_ref().expect("captured search has a real winning player");
        assert_eq!(player.gearset, result.gearset, "winning player uses the returned gearset");
        for slot in Slot::ALL {
            let piece = completion.context.catalog.piece(result.gearset[slot.index()]);
            assert_eq!(json!([piece.original_id, piece.name2]), expected["gearset"][slot.index()],
                "worker winning gearset/{}", slot.name());
        }
        assert_eq!(serde_json::to_value(&result.legacy_output).unwrap(), expected["legacy_output"]);
        assert_eq!(result.legacy_output.len(), 3);
        assert_eq!(result.metric.to_bits(), number(&expected["metric"]).to_bits(), "complete search score");
        assert_eq!(result.feasible, expected["feasible"].as_bool().unwrap());
        assert!(result.feasible);
        assert!(progress.iter().any(|progress| progress.scored > 0), "scored batches must be visible before the final report");
        assert!(progress.windows(2).all(|pair| pair[0].scored <= pair[1].scored), "published work cannot move backwards");
        assert_eq!(progress.last().unwrap().scored, result.scored, "final progress includes the complete scored work");
    });
}

#[test]
fn cancel_after_scored_search_never_publishes_an_equippable_partial_set_and_reuses_context() {
    bounded(|| {
        let fixture = search("search-simulation/seed11-real-pool-two-workers-repeated-pools");
        let (inputs, mut context) = search_inputs(&fixture);
        context.state.set_skill(context.catalog.empty, SkillType::HandToHand);
        context.random.uniform(RandomStream::Pdif, 0.0, 1.0);
        let pdif_checkpoint = context.random.clone();
        let action = cancellation_action(&fixture, &context.catalog);
        let mut worker = WorkerCoordinator::new();
        worker.start(WorkRequest { quicklook: inputs.clone(), action }, context).unwrap();
        let progress = first_progress(&mut worker);
        assert!(progress.scored > 0 && progress.best_metric.is_finite());
        worker.cancel();
        let (completion, _) = finish(&mut worker);
        let error = completion.outcome.err().expect("cancelled search must not install any best set");
        assert!(error.is_cancelled(), "unexpected cancellation outcome: {error}");
        assert_eq!(completion.context.state.override_for(completion.context.catalog.empty), Some(SkillType::HandToHand));
        let mut expected = pdif_checkpoint;
        let mut actual = completion.context.random.clone();
        for _ in 0..4 {
            assert_eq!(actual.uniform(RandomStream::Pdif, 0.0, 1.0), expected.uniform(RandomStream::Pdif, 0.0, 1.0),
                "physical search cancellation must recover the untouched pDIF stream");
        }
        let random = completion.context.random.clone();
        worker.start(stats_request(inputs), completion.context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::Stats { player, .. } = completion.outcome.unwrap() else { panic!("cancel prevented subsequent work"); };
        assert_eq!(player.main_job, wsdist_core::model::Job::Nin);
        assert_eq!(completion.context.random, random);
    });
}

#[test]
fn full_dps_job_returns_the_actual_final_cycle_and_cancelled_shutdown_returns_no_simulation() {
    bounded(|| {
        let fixture = search("search-simulation/full7200-single-host11-compiled20240101");
        let (inputs, context) = search_inputs(&fixture);
        let tp = support::parse_input(&fixture["input"]["tp_player"], &context.catalog, &context.state, "DPS TP");
        let ws = support::parse_input(&fixture["input"]["ws_player"], &context.catalog, &context.state, "DPS WS");
        let mut worker = WorkerCoordinator::new();
        worker.start(WorkRequest { quicklook: inputs.clone(), action: GuiAction::Dps {
            tp: tp.clone(), ws: ws.clone(), plot: true } }, context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::Dps { result, .. } = completion.outcome.unwrap() else { panic!("missing completed DPS result"); };
        let observation = native("search-simulation/full7200-single-host11-compiled20240101");
        let expected = &observation["result"];
        for (key, actual) in [("final_time", result.final_time), ("final_tp", result.final_tp), ("damage", result.damage),
            ("tp_damage", result.tp_damage), ("ws_damage", result.ws_damage)] {
            assert_eq!(actual.to_bits(), number(&expected[key]).to_bits(), "{key}");
        }
        assert!(result.final_time >= 7200.0, "finish the threshold phase and subsequent WS beyond 7200 seconds");
        assert_eq!(result.total_series.last().unwrap()[0], result.final_time);
        assert_eq!(result.ws_series.last().unwrap()[0], result.final_time);
        assert_eq!(result.tp_series.last().unwrap()[0], result.final_time - 2.0);

        let mut context = completion.context;
        context.state.set_skill(context.catalog.empty, SkillType::HandToHand);
        worker.start(WorkRequest { quicklook: inputs.clone(), action: GuiAction::Dps { tp, ws, plot: false } }, context).unwrap();
        let completion = worker.shutdown().expect("shutdown must join the active simulation");
        let error = completion.outcome.err().expect("Close must not install a partial or queued DPS result");
        assert!(error.is_cancelled(), "shutdown outcome: {error}");
        assert!(!worker.is_busy());
        assert!(worker.shutdown().is_none(), "shutdown joins only the one owned worker");
        assert_eq!(completion.context.state.override_for(completion.context.catalog.empty), Some(SkillType::HandToHand));
        let rng = completion.context.random.clone();
        worker.start(stats_request(inputs), completion.context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::Stats { player, .. } = completion.outcome.unwrap() else { panic!("shutdown context could not be reused"); };
        assert_player(&player, &fixture["expected"]["value"]["players"]["tp_after_build"], "post-shutdown player");
        assert_eq!(completion.context.random, rng);
    });
}

#[test]
fn shutdown_and_drop_join_scored_search_without_draining_progress_on_a_ui_thread() {
    bounded(|| {
        let fixture = search("search-simulation/seed11-real-pool-two-workers-repeated-pools");
        for drop_worker in [false, true] {
            let (inputs, context) = search_inputs(&fixture);
            let action = cancellation_action(&fixture, &context.catalog);
            let mut worker = WorkerCoordinator::new();
            worker.start(WorkRequest { quicklook: inputs, action }, context).unwrap();
            first_progress(&mut worker);
            if drop_worker {
                drop(worker);
            } else {
                let completion = worker.shutdown().expect("active search must be joined on Close");
                let error = completion.outcome.err().expect("queued search results are not installed while closing");
                assert!(error.is_cancelled(), "shutdown outcome: {error}");
                assert!(!worker.is_busy());
                assert!(worker.poll().is_none(), "Close cannot expose queued progress as a finished result");
            }
        }
    });
}

#[test]
fn distribution_worker_returns_all_frozen_bins_and_annotations_without_leaking_its_stage_capture() {
    bounded(|| {
        let fixture = search("search-simulation/distribution20000-magical-constant-host11-compiled20240101");
        let (inputs, mut context) = search_inputs(&fixture);
        let ws = inputs.player.clone();
        context.player_stages = Some(Vec::with_capacity(5));
        let saved_address = context.player_stages.as_ref().unwrap().as_ptr();
        let mut worker = WorkerCoordinator::new();
        worker.start(WorkRequest { quicklook: inputs.clone(), action: GuiAction::Distribution { ws } }, context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::Distribution { result, presentation, report } = completion.outcome.unwrap() else {
            panic!("distribution must return a complete histogram and its target's annotations");
        };
        let expected = &fixture["expected"]["value"];
        let observation = native("search-simulation/distribution20000-magical-constant-host11-compiled20240101");
        let expected_result = &observation["result"];
        assert_eq!(result.damage.len(), 20000, "fixed-TP sampling is complete, not a partial cancelled vector");
        assert_eq!(result.tp.len(), 20000);
        for (field, actual) in [("minimum", result.minimum), ("mean", result.mean),
            ("median", result.median), ("maximum", result.maximum)] {
            assert_eq!(actual.to_bits(), number(&expected_result[field]).to_bits(), "{field}");
        }
        assert_eq!(result.histogram.counts, serde_json::from_value::<Vec<u64>>(expected_result["histogram"]["counts"].clone()).unwrap(),
            "every inclusive-edge density bin matches the native 20000-sample result");
        assert_eq!(result.histogram.edges.len(), 301);
        assert_eq!(result.histogram.density.len(), 300);
        let edges = expected_result["histogram"]["edges"].as_array().unwrap();
        assert_eq!(result.histogram.edges.len(), edges.len());
        for (actual, expected) in result.histogram.edges.iter().zip(edges) {
            assert_eq!(actual.to_bits(), number(expected).to_bits(), "distribution bin edge");
        }
        let density = expected_result["histogram"]["density"].as_array().unwrap();
        assert_eq!(result.histogram.density.len(), density.len());
        for (actual, expected) in result.histogram.density.iter().zip(density) {
            assert_eq!(actual.to_bits(), number(expected).to_bits(), "distribution density");
        }
        let source = expected["plot"]["annotations"].as_array().unwrap()[0].as_str().unwrap();
        let source_rows: Vec<_> = source.lines().map(|line| {
            let (label, value) = line.split_once('=').unwrap();
            (label.trim(), value.trim())
        }).collect();
        assert_eq!(presentation.annotations.iter().map(|row| (row.label.as_str(), row.value.as_str())).collect::<Vec<_>>(), source_rows);
        assert!(report.text.contains(&format!("{}", result.mean)), "visible report retains the raw mean alongside the integer title");
        assert_eq!(completion.context.player_stages.as_ref().unwrap().as_ptr(), saved_address);
        assert!(completion.context.player_stages.as_ref().unwrap().is_empty(), "the job's target-build stages must not contaminate its caller's observer");
        let rng = completion.context.random.clone();
        worker.start(stats_request(inputs), completion.context).unwrap();
        let (completion, _) = finish(&mut worker);
        let WorkResult::Stats { player, .. } = completion.outcome.unwrap() else { panic!("distribution context could not be reused"); };
        assert_player(&player, &expected["player"], "post-distribution target");
        assert_eq!(completion.context.random, rng);
    });
}
