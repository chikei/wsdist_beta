use std::sync::atomic::AtomicBool;
use std::sync::LazyLock;

use serde_json::{Value, json};
use wsdist_core::model::*;
use wsdist_core::optimizer::{optimize, probe::*};

use crate::optimizer_support as adapter;
use crate::native_support;
use crate::support::{self, FrozenCatalog};

fn expect_native(case: &Value, actual: &Value) {
    static INDEX: LazyLock<support::FixtureIndex> = LazyLock::new(support::index);
    let id = case["id"].as_str().unwrap();
    let entry = INDEX.cases.iter().find(|entry| entry.id == id).unwrap();
    native_support::expect_native(id, &entry.sha256, actual);
}

/// Case-level entry point over the same production probes and shared native
/// observations as optimizer_behavior and native golden regeneration.
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
                adapter::assert_options(&normalized, &expected["normalized"], label);
                for index in 0..3 { adapter::assert_options(&restart_options(&normalized, index).unwrap(), &expected["restart_options"][index], label); }
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
            let catalog = FrozenCatalog::from_input(input);
            let run = adapter::run_neighborhood(case, &catalog);
            adapter::assert_neighborhood_legacy(case, &run, &catalog);
            expect_native(case, &adapter::project_neighborhood(&run, &catalog));
        }
        "optimizer_legality" => {
            let catalog = FrozenCatalog::from_input(input);
            let engine = adapter::context(&catalog, &input["seeds"]);
            let rules = adapter::legality(&catalog, input, &engine.state, label);
            let gearset = catalog.gearset(&input["player"]["gearset"]);
            assert_eq!(Some(rules.is_valid(&gearset, &catalog.catalog, &engine.state)), expected["valid"].as_bool(), "{label}: name/value legality");
            assert_eq!(json!(jse_ear_names()), expected["jse_ears"], "{label}: earring restrictions");
            catalog.assert_gearset(&gearset, &engine.state, &expected["effective_gearset"], label);
            assert_eq!(engine.random.uniforms() + engine.random.bounded + engine.random.reseeds, 0, "{label}: legality consumes no random calls");
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
            adapter::assert_draw_count(&engine.random, &expected["evidence"], label);
            adapter::assert_changes(&catalog, &before, &engine.state, &expected["catalog_metadata_changes"], label);
        }
        "optimizer_pass" => {
            let catalog = FrozenCatalog::from_input(input);
            let run = adapter::run_pass(case, &catalog);
            adapter::assert_pass_legacy(case, &run, &catalog);
            expect_native(case, &adapter::project_pass(&run, &catalog));
        }
        "optimizer_build" => {
            let catalog = FrozenCatalog::from_input(input);
            let run = adapter::run_build(case, &catalog);
            adapter::assert_build_legacy(case, &run, &catalog);
            let observation = adapter::project_build(&run, &catalog);
            expect_native(case, &observation);
            if label.contains("seed11-complete-build") {
                let mut engine = catalog.context(&input["seeds"]);
                let request = adapter::request(&catalog, input, &engine.state, label);
                let result = optimize(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}).unwrap();
                native_support::assert_json_eq(&adapter::project_build_result(&result, &catalog), &observation["result"], label);
                assert_eq!(engine.random, run.engine.random.inner, "{label}: public optimizer random continuation");
            }
        }
        "optimizer_dispatch" => {
            let catalog = FrozenCatalog::from_input(input);
            let mut engine = adapter::context(&catalog, &input["seeds"]);
            let mut trace = Some(SearchTrace::default());
            let (ctx, ids, pools) = adapter::score_context(&catalog, input, &engine.state, label);
            let mut mapper = adapter::mapper(input, &expected["evidence"], ctx.clone(), ids, &mut trace);
            let head = Slot::Head.index();
            let available = pools[head].iter().copied().filter(|&id| id != ids[head]).collect::<Vec<_>>();
            let total = input["dispatch_count"].as_u64().unwrap() as usize;
            let groups = vec![Group { combo: Tuple::new(&[head]), candidates: (0..total).map(|index| Tuple::new(&[available[index % available.len()]])).collect() }];
            let metrics = mapper.score(&ids, &groups, total, adapter::thresholds(&input["thresholds"]), &mut engine,
                &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut trace).unwrap();
            assert_eq!(json!(groups), expected["result"]["groups"], "{label}: original dispatch payload");
            adapter::assert_floats(&metrics, &expected["result"]["metrics"], label);
            adapter::assert_trace(trace.as_ref().unwrap(), &expected["evidence"], &catalog, label);
            adapter::assert_draw_count(&engine.random, &expected["evidence"], label);
            if total == 192 && label == "search-simulation/real-pool-local-dispatch-boundary-192" {
                for workers in [1, 2, 4] {
                    let mut engine = catalog.context(&input["seeds"]);
                    let before = engine.random.clone();
                    let mut mapper = Mapper::new(ctx.clone(), ids, MapperConfig::production(workers, 20_000), &mut None);
                    let scores = mapper.score(&ids, &groups, total, adapter::thresholds(&input["thresholds"]), &mut engine,
                        &mut ScoreScratch::new(&ctx), &AtomicBool::new(false), &mut None).unwrap();
                    adapter::assert_floats(&scores, &expected["result"]["metrics"], label);
                    assert_eq!(engine.random, before, "{label}: worker draws remain off coordinator");
                }
            }
        }
        "optimizer_sequence" => {
            let catalog = FrozenCatalog::from_input(input);
            let run = adapter::run_sequence(case, &catalog);
            adapter::assert_sequence_legacy(case, &run, &catalog);
            expect_native(case, &adapter::project_sequence(&run, &catalog));
        }
        operation => panic!("{label}: unsupported search operation {operation}"),
    }
}
