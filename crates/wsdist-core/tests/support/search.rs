use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};
use wsdist_core::model::*;
use wsdist_core::optimizer::{optimize, probe::*};

use crate::optimizer_support as adapter;
use crate::support::{self, FrozenCatalog, assert_number, number};

fn assert_options(actual: &OptimizeOptions, expected: &Value, label: &str) {
    assert_eq!(actual.iterations, expected["iterations"].as_i64().unwrap(), "{label}: iterations");
    assert_eq!(actual.max_swap_slots, expected["max_swap_slots"].as_i64().unwrap(), "{label}: swaps");
    assert_eq!(actual.restart_count, expected["restart_count"].as_i64().unwrap(), "{label}: restarts");
    assert_eq!(actual.seed.map(u64::from), expected["seed"].as_u64(), "{label}: seed");
    assert_number(actual.dt_step, &expected["dt_step"], label);
    assert_eq!(actual.workers, expected["workers"].as_i64().unwrap(), "{label}: workers");
}

fn replay_pass(input: &Value, expected: &Value, catalog: &FrozenCatalog,
    engine: &mut EvaluationContext<adapter::RecordedRandom>, baseline: Option<&ModelState>, label: &str) {
    let before = baseline.cloned().unwrap_or_else(|| engine.state.clone());
    let mut trace = Some(SearchTrace::default());
    let (ctx, ids, pools) = adapter::score_context(catalog, input, &engine.state, label);
    let (mut mapper, workers) = adapter::mapper(catalog, input, &expected["evidence"], ctx.clone(), ids, engine, &mut trace, label);
    let swappable = pools.iter().enumerate().filter_map(|(index, pool)| (!pool.is_empty()).then_some(index)).collect::<Vec<_>>();
    let combos = slot_combos(swappable.len(), input["max_swap_slots"].as_u64().unwrap() as usize).iter().map(|combo|
        Tuple::new(&combo.as_slice().iter().map(|&axis| swappable[axis]).collect::<Vec<_>>())).collect();
    let mut neighborhood = Neighborhood::new(combos, pools, &mut trace);
    let result = optimize_pass(&ctx, &mut mapper, &mut neighborhood, PassInput {
        ids, metric: number(&input["initial_metric"]), output: adapter::floats(&input["initial_output"]), invert: 1,
        next_best_percent: number(&input["next_best_percent"]), thresholds: adapter::thresholds(&input["thresholds"]),
        stage: 1, pass: 1, scored_before: 0, verbose_swaps: input["verbose_swaps"].as_bool().unwrap(),
    }, engine, &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut |_| {}, &mut trace).unwrap();
    let output = &expected["result"];
    assert_eq!(json!(ids), output["initial_ids"], "{label}: initial ids");
    assert_eq!(json!(result.ids), output["winning_ids"], "{label}: exact winner");
    assert_number(result.metric, &output["legacy_metric"], label);
    adapter::assert_floats(&result.output, &output["legacy_output"], label);
    catalog.assert_gearset(&ctx.gearset(&result.ids), &engine.state, &output["winning_gearset"], label);
    adapter::assert_similar(&result.similar, &output["similar_swaps"], catalog, label);
    assert_eq!(json!(neighborhood.position()), output["final_cursor"], "{label}: final cursor");
    assert_eq!(adapter::orders_value(neighborhood.orders()), output["pool_orders"], "{label}: independent shuffled pool orders");
    assert_eq!(result.accepted.len() as u64, output["observed_accepted_candidates"].as_u64().unwrap(), "{label}: accepted moves");
    adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], catalog, label);
    adapter::assert_draws(&engine.random, &expected["evidence"], label);
    adapter::assert_workers(&mapper, &workers, catalog, label);
    adapter::assert_changes(catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
}

/// Case-level entry point over the same production probes and adapters as
/// optimizer_behavior, including its logical-worker replay contract.
pub fn replay(case: &Value) {
    let label = case["id"].as_str().unwrap();
    let input = &case["input"];
    let expected = &case["expected"]["value"];
    match case["operation"].as_str().unwrap() {
        "optimizer_helpers" => match input["helper"].as_str().unwrap() {
            "slot_combos" => assert_eq!(json!(slot_combos(input["slot_count"].as_u64().unwrap() as usize,
                input["max_swap_slots"].as_u64().unwrap() as usize)), expected["ordered_combos"], "{label}: combination walk"),
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
                assert_eq!(json!(actual), expected["ordered_shares"], "{label}: ordered chunk flattening");
            }
            "intern_pieces" => {
                let catalog = FrozenCatalog::from_input(input);
                let engine = catalog.context(&input["seeds"]);
                let groups = input["groups"].as_array().unwrap().iter().map(|group| group.as_array().unwrap().iter().map(|gear| catalog.gear(gear)).collect::<Vec<_>>()).collect::<Vec<_>>();
                let interned = intern_pieces(&catalog.catalog, &engine.state, groups.iter().map(Vec::as_slice));
                let mapping = groups.iter().map(|group| group.iter().map(|&gear| interned.id(gear)).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(json!(mapping), expected["input_to_interned_id"], "{label}: full-record interning order");
                assert_eq!(interned.pieces.len(), expected["interned_records"].as_array().unwrap().len(), "{label}: distinct full records");
                for (&gear, record) in interned.pieces.iter().zip(expected["interned_records"].as_array().unwrap()) {
                    let record = Value::Object(Slot::ALL.iter().map(|slot| (slot.name().to_owned(), record.clone())).collect());
                    catalog.assert_gearset(&[gear; Slot::COUNT], &engine.state, &record, label);
                }
                let all = groups.into_iter().flatten().collect::<Vec<_>>();
                let equal = all.iter().map(|&a| all.iter().map(|&b| catalog.catalog.piece(a).value_equal(catalog.catalog.piece(b), &engine.state)).collect::<Vec<_>>()).collect::<Vec<_>>();
                let identical = all.iter().map(|&a| all.iter().map(|&b| a == b).collect::<Vec<_>>()).collect::<Vec<_>>();
                assert_eq!(json!(equal), expected["full_equality"], "{label}: ordered jobs, absent versus zero");
                assert_eq!(json!(identical), expected["object_identity"], "{label}: source object identity");
            }
            helper => panic!("{label}: unhandled helper {helper}"),
        },
        "optimizer_neighborhood" => {
            let combos = if input["combos"].is_null() { slot_combos(input["pools"].as_array().unwrap().len(), input["max_swap_slots"].as_u64().unwrap() as usize) }
                else { input["combos"].as_array().unwrap().iter().map(adapter::tuple).collect() };
            let pools: Vec<_> = input["pools"].as_array().unwrap().iter().map(adapter::integers).collect();
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
            assert_eq!(json!(combos), expected["combos"], "{label}: combination order");
            assert_eq!(json!(candidates), expected["ordered_candidates"], "{label}: every ordered candidate/duplicate");
            assert_eq!(json!(neighborhood.position()), expected["exhausted_cursor"], "{label}: exhausted cursor");
            neighborhood.seek(Cursor { combo_index: 0, odometer: Vec::new(), entered: false }, &mut trace);
            let (prefix, count) = neighborhood.take(&base, input["rewind_after"].as_u64().unwrap() as usize, &mut random, &mut trace);
            assert_eq!(json!(prefix), expected["rewind_prefix"], "{label}: rewound prefix");
            assert_eq!(count as u64, expected["rewind_prefix_count"].as_u64().unwrap(), "{label}: prefix count");
            assert_eq!(json!(neighborhood.position()), expected["replay_start"], "{label}: replay cursor");
            let (tail, count) = neighborhood.take(&base, 100000, &mut random, &mut trace);
            assert_eq!(json!(tail), expected["replayed_tail"], "{label}: replayed tail without reshuffle");
            assert_eq!(count as u64, expected["replay_count"].as_u64().unwrap(), "{label}: replay count");
            let catalog = FrozenCatalog::from_input(input);
            adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
            adapter::assert_draws(&random, &expected["evidence"], label);
        }
        "optimizer_legality" => {
            let catalog = FrozenCatalog::from_input(input);
            let engine = catalog.context(&input["seeds"]);
            let rules = adapter::legality(&catalog, input, &engine.state, label);
            let gearset = catalog.gearset(&input["player"]["gearset"]);
            assert_eq!(Some(rules.is_valid(&gearset, &catalog.catalog, &engine.state)), expected["valid"].as_bool(), "{label}: name/value legality");
            assert_eq!(json!(jse_ear_names()), expected["jse_ears"], "{label}: earring restrictions");
            catalog.assert_gearset(&gearset, &engine.state, &expected["effective_gearset"], label);
            support::assert_numpy_state(&engine.random, &expected["evidence"]["numpy_host_final_state"], label);
        }
        "optimizer_score" => {
            let catalog = FrozenCatalog::from_input(input);
            let mut engine = adapter::context(&catalog, &input["seeds"]);
            let before = engine.state.clone();
            let mut trace = Some(SearchTrace::default());
            let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
            let slots = input["score_slots"].as_array().unwrap().iter().map(|slot| Slot::from_name(slot.as_str().unwrap()).unwrap().index()).collect::<Vec<_>>();
            let groups = adapter::product_groups(&slots, &pools);
            let metrics = ScoreScratch::new(&ctx).score(&ctx, &ids, &groups, adapter::thresholds(&input["thresholds"]), &mut engine, &AtomicBool::new(false), &mut trace).unwrap();
            assert_eq!(json!(groups), expected["result"]["groups"], "{label}: score order");
            adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
            assert_eq!(Some(ctx.fast_gate), expected["result"]["fast_gate"].as_bool(), "{label}: aftermath fast gate");
            adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
            adapter::assert_draws(&engine.random, &expected["evidence"], label);
            adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
        }
        "optimizer_pass" => {
            let catalog = FrozenCatalog::from_input(input);
            let mut engine = adapter::context(&catalog, &input["seeds"]);
            replay_pass(input, expected, &catalog, &mut engine, None, label);
        }
        "optimizer_build" => {
            let catalog = FrozenCatalog::from_input(input);
            let mut engine = adapter::context(&catalog, &input["seeds"]);
            let before = engine.state.clone();
            let mut trace = Some(SearchTrace::default());
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
            assert_eq!(result.restarts as i64, normalize_options(Some(&request.options)).restart_count, "{label}: completed restarts");
            let events = expected["evidence"]["ordered_events"].as_array().unwrap();
            assert_eq!(result.stages, events.iter().filter(|event| event["kind"] == "dt_stage").count(), "{label}: completed stages");
            assert_eq!(result.passes, events.iter().filter(|event| event["kind"] == "pass_start").count(), "{label}: completed passes");
            assert_eq!(result.scored, events.iter().filter(|event| event["kind"] == "mapper_scores").map(|event| event["total"].as_u64().unwrap() as usize).sum::<usize>(), "{label}: scored count");
            if label.contains("seed11-complete-build") {
                let mut engine = catalog.context(&input["seeds"]);
                let request = adapter::request(&catalog, input, &engine.state, label);
                let result = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap();
                adapter::assert_build(&result, expected, &catalog, &engine.state, label);
                assert_number(result.metric, &json!(3244.8594084927004), label);
                for (slot, name) in [(Slot::Head, "Hattori Zukin +2"), (Slot::Hands, "Hattori Tekko +2"), (Slot::Feet, "Hattori Kyahan +2")] {
                    assert_eq!(catalog.catalog.piece(result.gearset[slot.index()]).name2.as_ref(), name, "{label}: public optimizer winner");
                }
                support::assert_numpy_state(&engine.random, &expected["evidence"]["numpy_host_final_state"], label);
            }
        }
        "optimizer_dispatch" => {
            let catalog = FrozenCatalog::from_input(input);
            let mut engine = adapter::context(&catalog, &input["seeds"]);
            let mut trace = Some(SearchTrace::default());
            let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
            let (mut mapper, workers) = adapter::mapper(&catalog, input, &expected["evidence"], ctx.clone(), ids, &engine, &mut trace, label);
            let head = Slot::Head.index();
            let available = pools[head].iter().copied().filter(|&id| id != ids[head]).collect::<Vec<_>>();
            let total = input["dispatch_count"].as_u64().unwrap() as usize;
            let groups = vec![Group { combo: Tuple::new(&[head]), candidates: (0..total).map(|index| Tuple::new(&[available[index % available.len()]])).collect() }];
            let metrics = mapper.score(&ids, &groups, total, adapter::thresholds(&input["thresholds"]), &mut engine,
                &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut trace).unwrap();
            assert_eq!(json!(groups), expected["result"]["groups"], "{label}: original dispatch payload");
            adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
            adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
            adapter::assert_draws(&engine.random, &expected["evidence"], label);
            adapter::assert_workers(&mapper, &workers, &catalog, label);
            if total == 192 && label == "search-simulation/real-pool-local-dispatch-boundary-192" {
                for workers in [1, 2, 4] {
                    let mut engine = catalog.context(&input["seeds"]);
                    let mut mapper = Mapper::new(ctx.clone(), ids, MapperConfig::production(workers, 20_000), &mut None);
                    let scores = mapper.score(&ids, &groups, total, adapter::thresholds(&input["thresholds"]), &mut engine,
                        &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut None).unwrap();
                    adapter::assert_floats(&scores, &expected["result"]["metrics"], label);
                    support::assert_numpy_state(&engine.random, &expected["evidence"]["numpy_host_final_state"], label);
                }
            }
        }
        "optimizer_sequence" => {
            let catalog = FrozenCatalog::from_input(input);
            let steps = input["steps"].as_array().unwrap();
            let mut engine = adapter::context(&catalog, &steps[0]["seeds"]);
            let before = engine.state.clone();
            for (index, step) in steps.iter().enumerate() {
                engine.random = adapter::RecordedRandom::seeded(&step["seeds"]);
                replay_pass(step, &expected["steps"][index], &catalog, &mut engine, Some(&before), &format!("{label}.step[{index}]"));
            }
            adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
            let isolated = adapter::context(&catalog, &steps[0]["seeds"]);
            assert_eq!(engine.state.skill(catalog.catalog.piece(catalog.catalog.empty)), SkillType::HandToHand, "{label}: shared Empty override");
            assert_eq!(isolated.state.skill(catalog.catalog.piece(catalog.catalog.empty)), SkillType::None, "{label}: independent session");
        }
        operation => panic!("{label}: unsupported search operation {operation}"),
    }
}
