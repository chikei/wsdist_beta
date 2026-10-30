#[path = "search_support/mod.rs"]
#[allow(dead_code)]
mod support;
mod optimizer_support;
#[allow(dead_code)]
mod native_support;

use std::sync::atomic::{AtomicBool, Ordering};

use wsdist_fixtures::{Value, to_value};
use wsdist_core::model::*;
use wsdist_core::optimizer::{optimize, probe::*};

use optimizer_support as adapter;
use support::FrozenCatalog;

fn fixtures(operation: &str) -> Vec<(support::IndexedCase, Value)> {
    support::index().cases.into_iter().filter(|case| case.operation == operation).map(|case| {
        let fixture = support::fixture(&support::fixture_root(), &case);
        assert_eq!(fixture["id"], case.id, "frozen index identity");
        assert_eq!(fixture["expected"]["outcome"], "result", "captured operation outcome");
        (case, fixture)
    }).collect()
}

#[test]
fn frozen_helper_walk_splitting_normalization_and_full_record_interning() {
    let cases = fixtures("optimizer_helpers");
    assert_eq!(cases.len(), 48);
    for (_, fixture) in cases {
        let label = fixture["id"].as_str().unwrap();
        let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        match input["helper"].as_str().unwrap() {
            "slot_combos" => assert_eq!(to_value(slot_combos(input["slot_count"].as_u64().unwrap() as usize,
                input["max_swap_slots"].as_u64().unwrap() as usize)).unwrap(), expected["ordered_combos"], "{label}: exact combination walk"),
            "options" => {
                let options: Option<OptimizeOptions> = wsdist_fixtures::from_value(&input["options"]).unwrap();
                let normalized = normalize_options(options.as_ref());
                adapter::assert_options(&normalized, &expected["normalized"], label);
                for index in 0..3 { adapter::assert_options(&restart_options(&normalized, index).unwrap(), &expected["restart_options"][index], label); }
            }
            "split_groups" => {
                let groups = adapter::groups(&input["groups"]);
                let shares = split_groups(&groups, input["total"].as_u64().unwrap() as usize, input["parts"].as_u64().unwrap() as usize);
                let actual = shares.iter().map(|share| share.iter().map(|group| (&group.combo, &group.candidates)).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(to_value(actual).unwrap(), expected["ordered_shares"], "{label}: ordered group/share flattening");
            }
            "intern_pieces" => {
                let catalog = FrozenCatalog::from_input(input); let engine = catalog.context(&input["seeds"]);
                let groups = input["groups"].as_array().unwrap().iter().map(|group| group.as_array().unwrap().iter().map(|gear| catalog.gear(gear)).collect::<Vec<_>>()).collect::<Vec<_>>();
                let interned = intern_pieces(&catalog.catalog, &engine.state, groups.iter().map(Vec::as_slice));
                let mapping = groups.iter().map(|group| group.iter().map(|&gear| interned.id(gear)).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(to_value(mapping).unwrap(), expected["input_to_interned_id"], "{label}: first-value interning/order");
                assert_eq!(interned.pieces.len(), expected["interned_records"].as_array().unwrap().len(), "{label}: distinct full records");
                for (&gear, record) in interned.pieces.iter().zip(expected["interned_records"].as_array().unwrap()) {
                    let record = Value::Object(Slot::ALL.iter().map(|slot| (slot.name().to_owned(), record.clone())).collect());
                    catalog.assert_gearset(&[gear; Slot::COUNT], &engine.state, &record, label);
                }
                let all = groups.into_iter().flatten().collect::<Vec<_>>();
                let equal = all.iter().map(|&a| all.iter().map(|&b| catalog.catalog.piece(a).value_equal(catalog.catalog.piece(b), &engine.state)).collect::<Vec<_>>()).collect::<Vec<_>>();
                let identical = all.iter().map(|&a| all.iter().map(|&b| a == b).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(to_value(equal).unwrap(), expected["full_equality"], "{label}: ordered jobs and order-independent numeric stats");
                assert_eq!(to_value(identical).unwrap(), expected["object_identity"], "{label}: original record identity");
            }
            helper => panic!("{label}: unhandled frozen helper {helper}"),
        }
    }
}

#[test]
fn frozen_neighborhoods_preserve_duplicates_skip_each_unchanged_axis_and_replay_cursor() {
    let cases = fixtures("optimizer_neighborhood"); assert_eq!(cases.len(), 44);
    for (entry, fixture) in cases {
        let catalog = FrozenCatalog::from_input(&fixture["input"]);
        let run = adapter::run_neighborhood(&fixture, &catalog);
        adapter::assert_neighborhood_legacy(&fixture, &run, &catalog);
        native_support::expect_native(&entry.id, &entry.source_sha256, &adapter::project_neighborhood(&run, &catalog));
    }
}

#[test]
fn frozen_legality_preserves_name_checks_record_equality_and_literal_ws_restrictions() {
    let cases = fixtures("optimizer_legality"); assert_eq!(cases.len(), 182);
    for (_, fixture) in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let engine = adapter::context(&catalog, &input["seeds"]);
        let rules = adapter::legality(&catalog, input, &engine.state, label);
        let gearset = catalog.gearset(&input["player"]["gearset"]);
        assert_eq!(Some(rules.is_valid(&gearset, &catalog.catalog, &engine.state)), expected["valid"].as_bool(), "{label}: legality result");
        assert_eq!(to_value(jse_ear_names()).unwrap(), expected["jse_ears"], "{label}: literal JSE restrictions");
        catalog.assert_gearset(&gearset, &engine.state, &expected["effective_gearset"], label);
        assert_eq!(engine.random.uniforms() + engine.random.bounded + engine.random.reseeds, 0, "{label}: legality consumes no randomness");
    }
}

#[test]
fn frozen_fast_dt_gate_full_player_verification_type_ii_and_aftermath_scores() {
    let cases = fixtures("optimizer_score"); assert_eq!(cases.len(), 58);
    for (_, fixture) in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = adapter::context(&catalog, &input["seeds"]);
        let before = engine.state.clone(); let mut trace = Some(SearchTrace::default());
        let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
        let slots = input["score_slots"].as_array().unwrap().iter().map(|slot| Slot::from_name(slot.as_str().unwrap()).unwrap().index()).collect::<Vec<_>>();
        let groups = adapter::product_groups(&slots, &pools);
        let metrics = ScoreScratch::new(&ctx).score(&ctx, &ids, &groups, adapter::thresholds(&input["thresholds"]), &mut engine, &AtomicBool::new(false), &mut trace).unwrap();
        assert_eq!(to_value(&groups).unwrap(), expected["result"]["groups"], "{label}: score candidate ordering");
        adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
        assert_eq!(Some(ctx.fast_gate), expected["result"]["fast_gate"].as_bool(), "{label}: aftermath fast-gate eligibility");
        adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
        adapter::assert_draw_count(&engine.random, &expected["evidence"], label);
        adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
    }
}

#[test]
fn frozen_first_improvement_passes_cover_batch_replay_seed11_ties_modulo_and_unused_draws() {
    let cases = fixtures("optimizer_pass"); assert_eq!(cases.len(), 28);
    for (entry, fixture) in cases {
        let catalog = FrozenCatalog::from_input(&fixture["input"]);
        let run = adapter::run_pass(&fixture, &catalog);
        adapter::assert_pass_legacy(&fixture, &run, &catalog);
        native_support::expect_native(&entry.id, &entry.source_sha256, &adapter::project_pass(&run, &catalog));
    }
}

#[test]
fn frozen_builds_preserve_initialization_stages_restarts_final_earring_order_and_sentinels() {
    let cases = fixtures("optimizer_build"); assert_eq!(cases.len(), 30);
    for (entry, fixture) in cases {
        let catalog = FrozenCatalog::from_input(&fixture["input"]);
        let run = adapter::run_build(&fixture, &catalog);
        adapter::assert_build_legacy(&fixture, &run, &catalog);
        native_support::expect_native(&entry.id, &entry.source_sha256, &adapter::project_build(&run, &catalog));
    }
}

#[test]
fn frozen_pool_dispatches_preserve_ordered_shares_and_local_boundaries() {
    let cases = fixtures("optimizer_dispatch"); assert_eq!(cases.len(), 8);
    for (_, fixture) in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = adapter::context(&catalog, &input["seeds"]); let mut trace = Some(SearchTrace::default());
        let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
        let mut mapper = adapter::mapper(input, &expected["evidence"], ctx.clone(), ids, &mut trace);
        let head = Slot::Head.index(); let available = pools[head].iter().copied().filter(|&id| id != ids[head]).collect::<Vec<_>>();
        let total = input["dispatch_count"].as_u64().unwrap() as usize;
        let groups = vec![Group { combo: Tuple::new(&[head]), candidates: (0..total).map(|index| Tuple::new(&[available[index % available.len()]])).collect() }];
        let metrics = mapper.score(&ids, &groups, total, adapter::thresholds(&input["thresholds"]), &mut engine,
            &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut trace).unwrap();
        assert_eq!(to_value(&groups).unwrap(), expected["result"]["groups"], "{label}: original dispatch payload"); adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
        adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
        adapter::assert_draw_count(&engine.random, &expected["evidence"], label);
    }
}

#[test]
fn frozen_h2h_sequence_preserves_shared_empty_metadata_without_cross_session_mutation() {
    let cases = fixtures("optimizer_sequence"); assert_eq!(cases.len(), 1);
    for (entry, fixture) in cases {
        let catalog = FrozenCatalog::from_input(&fixture["input"]);
        let run = adapter::run_sequence(&fixture, &catalog);
        adapter::assert_sequence_legacy(&fixture, &run, &catalog);
        native_support::expect_native(&entry.id, &entry.source_sha256, &adapter::project_sequence(&run, &catalog));
    }
}

fn build_fixture(id: &str) -> (support::IndexedCase, Value) {
    fixtures("optimizer_build").into_iter().find(|(entry, _)| entry.id == id).unwrap()
}

#[test]
fn production_optimizer_matches_native_seed11_build_across_worker_counts() {
    let mut results = Vec::new();
    for id in ["search-simulation/seed11-complete-build-workers1", "search-simulation/seed11-complete-build-workers2",
        "search-simulation/seed11-complete-build-workers0"] {
        let (entry, fixture) = build_fixture(id); let input = &fixture["input"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
        let request = adapter::request(&catalog, input, &engine.state, id);
        let result = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let baseline_run = adapter::run_build(&fixture, &catalog);
        adapter::assert_build_legacy(&fixture, &baseline_run, &catalog);
        let baseline = adapter::project_build(&baseline_run, &catalog);
        native_support::expect_native(id, &entry.source_sha256, &baseline);
        let actual = adapter::project_build_result(&result, &catalog);
        assert_eq!(actual, baseline["result"], "{id}: public optimizer matches shared native build");
        results.push(actual);
    }
    assert_eq!(results[0], results[1], "explicit worker counts retain identical native results");
    assert_eq!(results[0], results[2], "automatic workers retain the same native result");
}

#[test]
fn cancellation_before_start_and_after_a_bounded_batch_never_returns_a_partial_best_set() {
    let (_, fixture) = build_fixture("search-simulation/seed11-complete-build-workers1"); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
    let request = adapter::request(&catalog, input, &engine.state, "cancellation");
    let before = engine.random.clone();
    let error = optimize(&request, &mut engine, &AtomicBool::new(true), &mut |_| {}).unwrap_err();
    assert!(matches!(error, CalcError::Interrupted { operation: "optimize" }));
    assert_eq!(engine.random, before, "pre-cancelled search consumes no random state");
    let stop = AtomicBool::new(false); let mut batches = 0;
    let error = optimize(&request, &mut engine, &stop, &mut |progress| { if progress.scored > 0 { batches += 1; stop.store(true, Ordering::Relaxed); } }).unwrap_err();
    assert!(matches!(error, CalcError::Interrupted { operation: "optimize" }));
    assert_eq!(batches, 1, "cancelled at the first completed bounded score batch");
}

#[test]
fn raw_helper_unknown_ws_is_a_scoring_error_not_a_fake_id_or_rejected_candidate() {
    let (_, fixture) = fixtures("optimizer_pass").remove(0); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
    let player = catalog.player(&input["player"], &engine.state, "unknown raw WS");
    let gear = catalog.gear(&input["candidate_pools"]["head"][1]);
    let pieces = intern_pieces(&catalog.catalog, &engine.state, [player.gearset.as_slice(), &[gear]]);
    let mut action = adapter::raw_action(&input["request"]); action.ws_name = "".into(); action.ws_type = WsType::None;
    let rules = LegalityRules::new(player.main_job, player.sub_job, "weapon skill", "", "", &[(SkillType::Katana, vec!["".into()])], &[]);
    let ctx = ScoreContext::new(player.clone(), support::enemy(&input["enemy"]), action, rules, pieces, &catalog.catalog, true);
    let groups = [Group { combo: Tuple::new(&[Slot::Head.index()]), candidates: vec![Tuple::new(&[ctx.pieces.id(gear)])] }];
    let error = ScoreScratch::new(&ctx).score(&ctx, &ctx.ids(&player.gearset), &groups, [200.0; 2], &mut engine, &AtomicBool::new(false), &mut None).unwrap_err();
    support::assert_error(&error, &Value::Object([("class".into(), Value::from("KeyError")), ("message".into(), Value::from("''"))].into_iter().collect()), "raw empty WS lookup");
    let ctx = std::sync::Arc::new(ctx);
    let ids = ctx.ids(&player.gearset);
    let groups = [Group { combo: Tuple::new(&[Slot::Head.index()]), candidates: vec![Tuple::new(&[ctx.pieces.id(gear)]); 192] }];
    let mut mapper = Mapper::new(ctx.clone(), ids, MapperConfig::production(2, 20_000), &mut None);
    let error = mapper.score(&ids, &groups, 192, [200.0; 2], &mut engine,
        &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut None).unwrap_err();
    support::assert_error(&error, &Value::Object([("class".into(), Value::from("KeyError")), ("message".into(), Value::from("''"))].into_iter().collect()), "pool warmup/scoring lookup error");
}

#[test]
fn actual_rayon_workers_preserve_captured_ordered_deterministic_scores() {
    let (_, fixture) = fixtures("optimizer_dispatch").into_iter().find(|(entry, _)|
        entry.id == "search-simulation/real-pool-local-dispatch-boundary-192").unwrap();
    let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
    let catalog = FrozenCatalog::from_input(input);
    for workers in [1, 2, 4] {
        let mut engine = catalog.context(&input["seeds"]);
        let before = engine.random.clone();
        let (ctx, ids, _) = adapter::score_context(&catalog, input, &engine.state, "real Rayon dispatch");
        let groups = adapter::groups(&expected["result"]["groups"]);
        let mut mapper = Mapper::new(ctx.clone(), ids, MapperConfig::production(workers, 20_000), &mut None);
        assert_eq!(mapper.is_parallel(), workers > 1, "source pool candidate threshold");
        let metrics = mapper.score(&ids, &groups, 192, adapter::thresholds(&input["thresholds"]), &mut engine,
            &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut None).unwrap();
        adapter::assert_floats(&metrics, &expected["result"]["metrics"], "real ordered Rayon scores");
        assert_eq!(engine.random, before, "worker draws remain off coordinator");
    }
}

#[test]
fn consumer_metadata_distinguishes_no_candidate_output_from_unsatisfied_dt() {
    for (id, termination, feasible) in [
        ("search-simulation/complete-empty-frozen-build", OptimizeTermination::NoWinner, true),
        ("search-simulation/unsatisfied-dt-three-unchanged-stages", OptimizeTermination::UnchangedDtStages, false),
    ] {
        let (entry, fixture) = build_fixture(id); let input = &fixture["input"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
        let request = adapter::request(&catalog, input, &engine.state, id);
        let result = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let baseline_run = adapter::run_build(&fixture, &catalog);
        adapter::assert_build_legacy(&fixture, &baseline_run, &catalog);
        let baseline = adapter::project_build(&baseline_run, &catalog);
        native_support::expect_native(id, &entry.source_sha256, &baseline);
        assert_eq!(adapter::project_build_result(&result, &catalog), baseline["result"],
            "{id}: public optimizer matches shared native build");
        assert_eq!(result.termination, termination, "{id}: explicit termination metadata");
        assert_eq!(result.feasible, feasible, "{id}: no fabricated feasibility");
    }
}

#[test]
fn optimizer_cast_dispatch_keeps_divine_and_barrage_legacy_key_errors() {
    let (_, fixture) = build_fixture("search-simulation/metric-spell-Damage dealt"); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input);
    for name in ["Banish", "Barrage"] {
        let mut engine = catalog.context(&input["seeds"]);
        let mut request = adapter::raw_request(&catalog, input, &engine.state, "optimizer unsupported cast");
        request.action.spell_name = name.into();
        request.action.spell_type = SpellType::DivineMagic;
        let error = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut None).unwrap_err();
        support::assert_error(&error, &Value::Object([("class".into(), Value::from("KeyError")), ("message".into(), Value::from("''"))].into_iter().collect()), name);
    }
}

#[test]
fn raw_build_unknown_action_keeps_its_pre_evaluation_metric_format_error() {
    let (_, fixture) = build_fixture("search-simulation/zero-stage-default-output"); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
    let mut request = adapter::raw_request(&catalog, input, &engine.state, "unknown build action");
    request.action.action_type = "".into();
    let error = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut None).unwrap_err();
    support::assert_error(&error, &Value::Object([("class".into(), Value::from("KeyError")), ("message".into(), Value::from("''"))].into_iter().collect()), "unknown build METRIC_FORMAT key");
}

#[test]
fn zero_time_reciprocal_is_a_legacy_error_in_search_and_candidate_scoring() {
    let (_, fixture) = build_fixture("search-simulation/seed11-complete-build-workers1");
    let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input);
    let expected = Value::Object([
        ("class".into(), Value::from("ZeroDivisionError")),
        ("message".into(), Value::from("0.0 cannot be raised to a negative power")),
    ].into_iter().collect());
    let mut engine = catalog.context(&input["seeds"]);
    let mut request = adapter::request(&catalog, input, &engine.state, "zero-time search");
    request.action = ActionRequest::Round { metric: Metric::new("Time to WS"), starting_tp: 0.0, threshold: 0.0 };
    let error = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    support::assert_error(&error, &expected, "zero-time search must not accept an infinite winner");

    let mut score_input = input.clone();
    score_input["request"]["action_type"] = Value::from("attack round");
    score_input["request"]["input_metric"] = Value::from("Time to WS");
    score_input["request"]["min_tp"] = Value::from(0.0);
    for mut trace in [None, Some(SearchTrace::default())] {
        let mut engine = catalog.context(&input["seeds"]);
        let (ctx, ids, _) = adapter::score_context(&catalog, &score_input, &engine.state, "zero-time score");
        let gear = catalog.gear(&input["candidate_pools"]["head"][1]);
        let groups = [Group { combo: Tuple::new(&[Slot::Head.index()]), candidates: vec![Tuple::new(&[ctx.pieces.id(gear)])] }];
        let error = ScoreScratch::new(&ctx).score(&ctx, &ids, &groups, [200.0; 2],
            &mut engine, &AtomicBool::new(false), &mut trace).unwrap_err();
        support::assert_error(&error, &expected, "hot and diagnostic candidate scoring propagate the same arithmetic error");
    }
}
