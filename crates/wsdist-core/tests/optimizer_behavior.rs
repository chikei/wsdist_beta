#[path = "search_support/mod.rs"]
#[allow(dead_code)]
mod support;
mod optimizer_support;

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use wsdist_core::model::*;
use wsdist_core::optimizer::{optimize, probe::*};

use optimizer_support as adapter;
use support::{FrozenCatalog, assert_number, document, number};

fn fixtures(operation: &str) -> Vec<Value> {
    support::index().cases.into_iter().filter(|case| case.operation == operation).map(|case| {
        let fixture = document(&support::fixture_root().join(case.path));
        assert_eq!(fixture["id"], case.id, "frozen index identity");
        assert_eq!(fixture["expected"]["outcome"], "result", "captured operation outcome");
        fixture
    }).collect()
}

fn assert_options(actual: &OptimizeOptions, expected: &Value, label: &str) {
    assert_eq!(actual.iterations, expected["iterations"].as_i64().unwrap(), "{label}: pass limit");
    assert_eq!(actual.max_swap_slots, expected["max_swap_slots"].as_i64().unwrap(), "{label}: swap limit");
    assert_eq!(actual.restart_count, expected["restart_count"].as_i64().unwrap(), "{label}: restart count");
    assert_eq!(actual.seed.map(u64::from), expected["seed"].as_u64(), "{label}: seed");
    assert_number(actual.dt_step, &expected["dt_step"], label);
    assert_eq!(actual.workers, expected["workers"].as_i64().unwrap(), "{label}: requested workers");
}

#[test]
fn frozen_helper_walk_splitting_normalization_and_full_record_interning() {
    let cases = fixtures("optimizer_helpers");
    assert_eq!(cases.len(), 48);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap();
        let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        match input["helper"].as_str().unwrap() {
            "slot_combos" => assert_eq!(json!(slot_combos(input["slot_count"].as_u64().unwrap() as usize,
                input["max_swap_slots"].as_u64().unwrap() as usize)), expected["ordered_combos"], "{label}: exact combination walk"),
            "options" => {
                let options: Option<OptimizeOptions> = serde_json::from_value(input["options"].clone()).unwrap();
                let normalized = normalize_options(options.as_ref());
                assert_options(&normalized, &expected["normalized"], label);
                for index in 0..3 { assert_options(&restart_options(&normalized, index).unwrap(), &expected["restart_options"][index], label); }
            }
            "split_groups" => {
                let groups = adapter::groups(&input["groups"]);
                let shares = split_groups(&groups, input["total"].as_u64().unwrap() as usize, input["parts"].as_u64().unwrap() as usize);
                let actual = shares.iter().map(|share| share.iter().map(|group| json!([group.combo, group.candidates])).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(json!(actual), expected["ordered_shares"], "{label}: ordered group/share flattening");
            }
            "intern_pieces" => {
                let catalog = FrozenCatalog::from_input(input); let engine = catalog.context(&input["seeds"]);
                let groups = input["groups"].as_array().unwrap().iter().map(|group| group.as_array().unwrap().iter().map(|gear| catalog.gear(gear)).collect::<Vec<_>>()).collect::<Vec<_>>();
                let interned = intern_pieces(&catalog.catalog, &engine.state, groups.iter().map(Vec::as_slice));
                let mapping = groups.iter().map(|group| group.iter().map(|&gear| interned.id(gear)).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(json!(mapping), expected["input_to_interned_id"], "{label}: first-value interning/order");
                assert_eq!(interned.pieces.len(), expected["interned_records"].as_array().unwrap().len(), "{label}: distinct full records");
                for (&gear, record) in interned.pieces.iter().zip(expected["interned_records"].as_array().unwrap()) {
                    let record = Value::Object(Slot::ALL.iter().map(|slot| (slot.name().to_owned(), record.clone())).collect());
                    catalog.assert_gearset(&[gear; Slot::COUNT], &engine.state, &record, label);
                }
                let all = groups.into_iter().flatten().collect::<Vec<_>>();
                let equal = all.iter().map(|&a| all.iter().map(|&b| catalog.catalog.piece(a).value_equal(catalog.catalog.piece(b), &engine.state)).collect::<Vec<_>>()).collect::<Vec<_>>();
                let identical = all.iter().map(|&a| all.iter().map(|&b| a == b).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(json!(equal), expected["full_equality"], "{label}: ordered jobs and order-independent numeric stats");
                assert_eq!(json!(identical), expected["object_identity"], "{label}: original record identity");
            }
            helper => panic!("{label}: unhandled frozen helper {helper}"),
        }
    }
}

#[test]
fn frozen_neighborhoods_preserve_duplicates_skip_each_unchanged_axis_and_replay_cursor() {
    let cases = fixtures("optimizer_neighborhood"); assert_eq!(cases.len(), 44);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let combos = if input["combos"].is_null() { slot_combos(input["pools"].as_array().unwrap().len(), input["max_swap_slots"].as_u64().unwrap() as usize) }
            else { input["combos"].as_array().unwrap().iter().map(adapter::tuple).collect() };
        let pools: Vec<Vec<usize>> = input["pools"].as_array().unwrap().iter().map(adapter::integers).collect();
        let base = adapter::integers(&input["base_ids"]);
        let mut random = adapter::RecordedRandom::seeded(&input["seeds"]);
        let mut trace = Some(SearchTrace::default());
        let mut neighborhood = Neighborhood::new(combos.clone(), pools, &mut trace);
        let mut candidates = Vec::new();
        while !neighborhood.exhausted() {
            let (groups, count) = neighborhood.take(&base, input["batch"].as_u64().unwrap() as usize, &mut random, &mut trace);
            for group in groups { for ids in group.candidates { candidates.push(json!([group.combo, ids])); } }
            if count == 0 { break; }
        }
        assert_eq!(json!(combos), expected["combos"], "{label}: combo walk");
        assert_eq!(json!(candidates), expected["ordered_candidates"], "{label}: candidate order/duplicates");
        assert_eq!(json!(neighborhood.position()), expected["exhausted_cursor"], "{label}: exhausted cursor");
        neighborhood.seek(Cursor { combo_index: 0, odometer: Vec::new(), entered: false }, &mut trace);
        let (prefix, count) = neighborhood.take(&base, input["rewind_after"].as_u64().unwrap() as usize, &mut random, &mut trace);
        assert_eq!(json!(prefix), expected["rewind_prefix"], "{label}: rewind prefix");
        assert_eq!(count as u64, expected["rewind_prefix_count"].as_u64().unwrap(), "{label}: rewind count");
        assert_eq!(json!(neighborhood.position()), expected["replay_start"], "{label}: replay cursor");
        let (tail, count) = neighborhood.take(&base, 100000, &mut random, &mut trace);
        assert_eq!(json!(tail), expected["replayed_tail"], "{label}: replay without reshuffling");
        assert_eq!(count as u64, expected["replay_count"].as_u64().unwrap(), "{label}: replay count");
        let catalog = FrozenCatalog::from_input(input);
        adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
        adapter::assert_draws(&random, &expected["evidence"], label);
    }
}

#[test]
fn frozen_legality_preserves_name_checks_record_equality_and_literal_ws_restrictions() {
    let cases = fixtures("optimizer_legality"); assert_eq!(cases.len(), 182);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let engine = catalog.context(&input["seeds"]);
        let rules = adapter::legality(&catalog, input, &engine.state, label);
        let gearset = catalog.gearset(&input["player"]["gearset"]);
        assert_eq!(Some(rules.is_valid(&gearset, &catalog.catalog, &engine.state)), expected["valid"].as_bool(), "{label}: legality result");
        assert_eq!(json!(jse_ear_names()), expected["jse_ears"], "{label}: literal JSE restrictions");
        catalog.assert_gearset(&gearset, &engine.state, &expected["effective_gearset"], label);
        support::assert_numpy_state(&engine.random, &expected["evidence"]["numpy_host_final_state"], label);
    }
}

#[test]
fn frozen_fast_dt_gate_full_player_verification_type_ii_and_aftermath_scores() {
    let cases = fixtures("optimizer_score"); assert_eq!(cases.len(), 58);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = adapter::context(&catalog, &input["seeds"]);
        let before = engine.state.clone(); let mut trace = Some(SearchTrace::default());
        let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
        let slots = input["score_slots"].as_array().unwrap().iter().map(|slot| Slot::from_name(slot.as_str().unwrap()).unwrap().index()).collect::<Vec<_>>();
        let groups = adapter::product_groups(&slots, &pools);
        let metrics = ScoreScratch::new(&ctx).score(&ctx, &ids, &groups, adapter::thresholds(&input["thresholds"]), &mut engine, &AtomicBool::new(false), &mut trace).unwrap();
        assert_eq!(json!(groups), expected["result"]["groups"], "{label}: score candidate ordering");
        adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
        assert_eq!(Some(ctx.fast_gate), expected["result"]["fast_gate"].as_bool(), "{label}: aftermath fast-gate eligibility");
        adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
        adapter::assert_draws(&engine.random, &expected["evidence"], label);
        adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
    }
}

fn replay_pass(input: &Value, expected: &Value, catalog: &FrozenCatalog, engine: &mut EvaluationContext<adapter::RecordedRandom>,
    baseline: Option<&ModelState>, label: &str) {
    let before = baseline.cloned().unwrap_or_else(|| engine.state.clone()); let mut trace = Some(SearchTrace::default());
    let (ctx, ids, pools) = adapter::score_context(catalog, input, &engine.state, label);
    let (mut mapper, workers) = adapter::mapper(catalog, input, &expected["evidence"], ctx.clone(), ids, engine, &mut trace, label);
    let swappable = pools.iter().enumerate().filter_map(|(index, pool)| (!pool.is_empty()).then_some(index)).collect::<Vec<_>>();
    let combos = slot_combos(swappable.len(), input["max_swap_slots"].as_u64().unwrap() as usize).iter().map(|combo|
        Tuple::new(&combo.as_slice().iter().map(|&axis| swappable[axis]).collect::<Vec<_>>())).collect();
    let mut neighborhood = Neighborhood::new(combos, pools, &mut trace);
    let result = optimize_pass(&ctx, &mut mapper, &mut neighborhood, PassInput { ids, metric: number(&input["initial_metric"]),
        output: adapter::floats(&input["initial_output"]), invert: 1, next_best_percent: number(&input["next_best_percent"]),
        thresholds: adapter::thresholds(&input["thresholds"]), stage: 1, pass: 1, scored_before: 0,
        verbose_swaps: input["verbose_swaps"].as_bool().unwrap() }, engine, &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut |_| {}, &mut trace).unwrap();
    let output = &expected["result"];
    assert_eq!(json!(ids), output["initial_ids"], "{label}: initial interned ids");
    assert_eq!(json!(result.ids), output["winning_ids"], "{label}: exact winner");
    assert_number(result.metric, &output["legacy_metric"], label);
    adapter::assert_floats(&result.output, &output["legacy_output"], label);
    catalog.assert_gearset(&ctx.gearset(&result.ids), &engine.state, &output["winning_gearset"], label);
    adapter::assert_similar(&result.similar, &output["similar_swaps"], catalog, label);
    assert_eq!(json!(neighborhood.position()), output["final_cursor"], "{label}: winner cursor");
    assert_eq!(adapter::orders_value(neighborhood.orders()), output["pool_orders"], "{label}: cached independent pool orders");
    assert_eq!(result.accepted.len() as u64, output["observed_accepted_candidates"].as_u64().unwrap(), "{label}: exact accepted moves");
    adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], catalog, label);
    adapter::assert_draws(&engine.random, &expected["evidence"], label);
    adapter::assert_workers(&mapper, &workers, catalog, label);
    adapter::assert_changes(catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
}

#[test]
fn frozen_first_improvement_passes_cover_batch_replay_seed11_ties_modulo_and_unused_draws() {
    let cases = fixtures("optimizer_pass"); assert_eq!(cases.len(), 28);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = adapter::context(&catalog, &input["seeds"]);
        replay_pass(input, &fixture["expected"]["value"], &catalog, &mut engine, None, label);
    }
}

#[test]
fn frozen_builds_preserve_initialization_stages_restarts_final_earring_order_and_sentinels() {
    let cases = fixtures("optimizer_build"); assert_eq!(cases.len(), 30);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = adapter::context(&catalog, &input["seeds"]);
        let before = engine.state.clone(); let mut trace = Some(SearchTrace::default());
        let mut request = adapter::raw_request(&catalog, input, &engine.state, label);
        if request.options.workers == 0 {
            request.automatic_workers = Some(expected["evidence"]["ordered_events"].as_array().unwrap().iter()
                .find(|event| event["kind"] == "resolved_workers").unwrap()["resolved"].as_u64().unwrap() as usize);
        }
        let result = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut trace).unwrap();
        adapter::assert_build(&result, expected, &catalog, &engine.state, label);
        adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
        adapter::assert_draws(&engine.random, &expected["evidence"], label);
        adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
        assert_eq!(result.restarts as i64, normalize_options(Some(&request.options)).restart_count, "{label}: completed restart count");
        let events = expected["evidence"]["ordered_events"].as_array().unwrap();
        assert_eq!(result.stages, events.iter().filter(|event| event["kind"] == "dt_stage").count(), "{label}: completed stages");
        assert_eq!(result.passes, events.iter().filter(|event| event["kind"] == "pass_start").count(), "{label}: completed passes");
        let scored: usize = events.iter().filter(|event| event["kind"] == "mapper_scores").map(|event| event["total"].as_u64().unwrap() as usize).sum();
        assert_eq!(result.scored, scored, "{label}: ordered scored candidate count");
    }
}

#[test]
fn frozen_pool_dispatches_replay_real_worker_warmup_ordered_shares_and_local_boundaries() {
    let cases = fixtures("optimizer_dispatch"); assert_eq!(cases.len(), 8);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = adapter::context(&catalog, &input["seeds"]); let mut trace = Some(SearchTrace::default());
        let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
        let (mut mapper, workers) = adapter::mapper(&catalog, input, &expected["evidence"], ctx.clone(), ids, &engine, &mut trace, label);
        let head = Slot::Head.index(); let available = pools[head].iter().copied().filter(|&id| id != ids[head]).collect::<Vec<_>>();
        let total = input["dispatch_count"].as_u64().unwrap() as usize;
        let groups = vec![Group { combo: Tuple::new(&[head]), candidates: (0..total).map(|index| Tuple::new(&[available[index % available.len()]])).collect() }];
        let metrics = mapper.score(&ids, &groups, total, adapter::thresholds(&input["thresholds"]), &mut engine,
            &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut trace).unwrap();
        assert_eq!(json!(groups), expected["result"]["groups"], "{label}: original dispatch payload"); adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
        adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
        adapter::assert_draws(&engine.random, &expected["evidence"], label);
        adapter::assert_workers(&mapper, &workers, &catalog, label);
    }
}

#[test]
fn frozen_h2h_sequence_preserves_shared_empty_metadata_without_cross_session_mutation() {
    let cases = fixtures("optimizer_sequence"); assert_eq!(cases.len(), 1);
    for fixture in cases {
        let label = fixture["id"].as_str().unwrap(); let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
        let catalog = FrozenCatalog::from_input(input);
        let steps = input["steps"].as_array().unwrap(); let mut engine = adapter::context(&catalog, &steps[0]["seeds"]);
        let before = engine.state.clone();
        for (index, step) in steps.iter().enumerate() {
            engine.random = adapter::RecordedRandom::seeded(&step["seeds"]);
            replay_pass(step, &expected["steps"][index], &catalog, &mut engine, Some(&before), &format!("{label}.step[{index}]"));
        }
        adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
        let isolated = adapter::context(&catalog, &steps[0]["seeds"]);
        let empty = catalog.catalog.by_symbol("Empty").unwrap();
        assert_eq!(engine.state.skill(catalog.catalog.piece(empty)), SkillType::HandToHand, "{label}: sequential shared Empty override");
        assert_eq!(isolated.state.skill(catalog.catalog.piece(empty)), SkillType::None, "{label}: independent session override boundary");
    }
}

fn build_fixture(id: &str) -> Value {
    fixtures("optimizer_build").into_iter().find(|fixture| fixture["id"] == id).unwrap()
}

#[test]
fn production_optimizer_returns_exact_seed11_set_metric_and_raw_output() {
    for id in ["search-simulation/seed11-complete-build-workers1", "search-simulation/seed11-complete-build-workers2",
        "search-simulation/seed11-complete-build-workers0"] {
        let fixture = build_fixture(id); let input = &fixture["input"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
        let request = adapter::request(&catalog, input, &engine.state, id);
        let result = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap();
        adapter::assert_build(&result, &fixture["expected"]["value"], &catalog, &engine.state, id);
        assert_number(result.metric, &json!(3244.8594084927004), "production exact seed11 build metric");
        for (slot, name) in [(Slot::Head, "Hattori Zukin +2"), (Slot::Hands, "Hattori Tekko +2"), (Slot::Feet, "Hattori Kyahan +2")] {
            assert_eq!(catalog.catalog.piece(result.gearset[slot.index()]).name2.as_ref(), name, "{id}: exact {}", slot.name());
        }
        support::assert_numpy_state(&engine.random, &fixture["expected"]["value"]["evidence"]["numpy_host_final_state"], id);
    }
}

#[test]
fn cancellation_before_start_and_after_a_bounded_batch_never_returns_a_partial_best_set() {
    let fixture = build_fixture("search-simulation/seed11-complete-build-workers1"); let input = &fixture["input"];
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
    let fixture = fixtures("optimizer_pass").remove(0); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
    let player = catalog.player(&input["player"], &engine.state, "unknown raw WS");
    let gear = catalog.gear(&input["candidate_pools"]["head"][1]);
    let pieces = intern_pieces(&catalog.catalog, &engine.state, [player.gearset.as_slice(), &[gear]]);
    let mut action = adapter::raw_action(&input["request"]); action.ws_name = "".into(); action.ws_type = WsType::None;
    let rules = LegalityRules::new(player.main_job, player.sub_job, "weapon skill", "", "", &[(SkillType::Katana, vec!["".into()])], &[]);
    let ctx = ScoreContext::new(player.clone(), support::enemy(&input["enemy"]), action, rules, pieces, &catalog.catalog, true);
    let groups = [Group { combo: Tuple::new(&[Slot::Head.index()]), candidates: vec![Tuple::new(&[ctx.pieces.id(gear)])] }];
    let error = ScoreScratch::new(&ctx).score(&ctx, &ctx.ids(&player.gearset), &groups, [200.0; 2], &mut engine, &AtomicBool::new(false), &mut None).unwrap_err();
    support::assert_error(&error, &json!({"class":"KeyError","message":"''"}), "raw empty WS lookup");
    let ctx = std::sync::Arc::new(ctx);
    let ids = ctx.ids(&player.gearset);
    let groups = [Group { combo: Tuple::new(&[Slot::Head.index()]), candidates: vec![Tuple::new(&[ctx.pieces.id(gear)]); 192] }];
    let mut mapper = Mapper::new(ctx.clone(), ids, MapperConfig::production(2, 20_000), &mut None);
    let error = mapper.score(&ids, &groups, 192, [200.0; 2], &mut engine,
        &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut None).unwrap_err();
    support::assert_error(&error, &json!({"class":"KeyError","message":"''"}), "pool warmup/scoring lookup error");
}

#[test]
fn actual_rayon_workers_preserve_captured_ordered_deterministic_scores() {
    let fixture = fixtures("optimizer_dispatch").into_iter().find(|fixture|
        fixture["id"] == "search-simulation/real-pool-local-dispatch-boundary-192").unwrap();
    let input = &fixture["input"]; let expected = &fixture["expected"]["value"];
    let catalog = FrozenCatalog::from_input(input);
    for workers in [1, 2, 4] {
        let mut engine = catalog.context(&input["seeds"]);
        let (ctx, ids, _) = adapter::score_context(&catalog, input, &engine.state, "real Rayon dispatch");
        let groups = adapter::groups(&expected["result"]["groups"]);
        let mut mapper = Mapper::new(ctx.clone(), ids, MapperConfig::production(workers, 20_000), &mut None);
        assert_eq!(mapper.is_parallel(), workers > 1, "source pool candidate threshold");
        let metrics = mapper.score(&ids, &groups, 192, adapter::thresholds(&input["thresholds"]), &mut engine,
            &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut None).unwrap();
        adapter::assert_floats(&metrics, &expected["result"]["metrics"], "real ordered Rayon scores");
        support::assert_numpy_state(&engine.random, &expected["evidence"]["numpy_host_final_state"], "worker draws remain off coordinator");
    }
}

#[test]
fn consumer_metadata_distinguishes_no_candidate_output_from_unsatisfied_dt() {
    for (id, termination, feasible) in [
        ("search-simulation/complete-empty-frozen-build", OptimizeTermination::NoWinner, true),
        ("search-simulation/unsatisfied-dt-three-unchanged-stages", OptimizeTermination::UnchangedDtStages, false),
    ] {
        let fixture = build_fixture(id); let input = &fixture["input"];
        let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
        let request = adapter::request(&catalog, input, &engine.state, id);
        let result = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap();
        adapter::assert_build(&result, &fixture["expected"]["value"], &catalog, &engine.state, id);
        assert_eq!(result.termination, termination, "{id}: explicit termination metadata");
        assert_eq!(result.feasible, feasible, "{id}: no fabricated feasibility");
    }
}

#[test]
fn optimizer_cast_dispatch_keeps_divine_and_barrage_legacy_key_errors() {
    let fixture = build_fixture("search-simulation/metric-spell-Damage dealt"); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input);
    for name in ["Banish", "Barrage"] {
        let mut engine = catalog.context(&input["seeds"]);
        let mut request = adapter::raw_request(&catalog, input, &engine.state, "optimizer unsupported cast");
        request.action.spell_name = name.into();
        request.action.spell_type = SpellType::DivineMagic;
        let error = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut None).unwrap_err();
        support::assert_error(&error, &json!({"class":"KeyError","message":"''"}), name);
    }
}

#[test]
fn raw_build_unknown_action_keeps_its_pre_evaluation_metric_format_error() {
    let fixture = build_fixture("search-simulation/zero-stage-default-output"); let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input); let mut engine = catalog.context(&input["seeds"]);
    let mut request = adapter::raw_request(&catalog, input, &engine.state, "unknown build action");
    request.action.action_type = "".into();
    let error = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut None).unwrap_err();
    support::assert_error(&error, &json!({"class":"KeyError","message":"''"}), "unknown build METRIC_FORMAT key");
}

#[test]
fn zero_time_reciprocal_is_a_legacy_error_in_search_and_candidate_scoring() {
    let fixture = build_fixture("search-simulation/seed11-complete-build-workers1");
    let input = &fixture["input"];
    let catalog = FrozenCatalog::from_input(input);
    let expected = json!({"class": "ZeroDivisionError", "message": "0.0 cannot be raised to a negative power"});
    let mut engine = catalog.context(&input["seeds"]);
    let mut request = adapter::request(&catalog, input, &engine.state, "zero-time search");
    request.action = ActionRequest::Round { metric: Metric::new("Time to WS"), starting_tp: 0.0, threshold: 0.0 };
    let error = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    support::assert_error(&error, &expected, "zero-time search must not accept an infinite winner");

    let mut score_input = input.clone();
    score_input["request"]["action_type"] = json!("attack round");
    score_input["request"]["input_metric"] = json!("Time to WS");
    score_input["request"]["min_tp"] = json!(0.0);
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
