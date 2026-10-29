#[path = "search_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[allow(dead_code)]
mod optimizer_support;
#[allow(dead_code)]
mod native_support;

use std::sync::atomic::AtomicBool;

use wsdist_core::model::{CalculationStep, OptimizeResult, Slot};
use wsdist_core::optimizer::probe::{SearchTrace, TraceEvent, optimize_raw};

use optimizer_support as adapter;

fn value(step: &CalculationStep, name: &str) -> f64 {
    step.values.iter().find(|(key, _)| *key == name).unwrap().1
}

fn step<'a>(result: &'a OptimizeResult, label: &str) -> &'a CalculationStep {
    result.calculation.iter().find(|step| step.label == label).unwrap()
}

fn traced_fixture(id: &str) -> (support::FrozenCatalog, OptimizeResult, SearchTrace) {
    let entry = support::index().cases.into_iter().find(|case| case.id == id).unwrap();
    let fixture = support::fixture(&support::fixture_root(), &entry);
    let input = &fixture["input"];
    let catalog = support::FrozenCatalog::from_input(input);
    let baseline_run = adapter::run_build(&fixture, &catalog);
    adapter::assert_build_legacy(&fixture, &baseline_run, &catalog);
    native_support::expect_native(id, &entry.sha256, &adapter::project_build(&baseline_run, &catalog));
    let baseline = baseline_run.result.as_ref().unwrap();
    assert!(baseline.calculation.is_empty());
    assert!(baseline_run.engine.calculation_trace.is_none());

    let observer = CalculationStep { label: "Caller observer", equation: Some("Caller = prior value"),
        formula: "caller = {caller}", values: vec![("caller", 17.0)] };
    let mut engine = adapter::context(&catalog, &input["seeds"]);
    engine.calculation_trace = Some(vec![observer.clone()]);
    let mut trace = Some(SearchTrace::default());
    let result = optimize_raw(&baseline_run.request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut trace).unwrap();

    assert_eq!(adapter::project_build_result(&result, &catalog), adapter::project_build_result(baseline, &catalog),
        "display tracing must preserve the native build result");
    assert_eq!(result.gearset, baseline.gearset);
    assert_eq!(result.metric, baseline.metric);
    assert_eq!(result.legacy_output, baseline.legacy_output);
    assert_eq!(result.termination, baseline.termination);
    assert_eq!(result.accepted.len(), baseline.accepted.len());
    assert_eq!((result.stages, result.passes, result.scored), (baseline.stages, baseline.passes, baseline.scored));
    assert_eq!(native_support::random_value(&engine.random), native_support::random_value(&baseline_run.engine.random),
        "display tracing must not change RNG continuation");
    assert_eq!(trace.as_ref().unwrap().events.len(), baseline_run.trace.events.len(),
        "display tracing must not add action evaluations or search events");
    let retained_observer = engine.calculation_trace.as_ref().unwrap();
    assert_eq!(retained_observer.len(), 1, "candidate evaluations must not write into the caller observer");
    assert_eq!(retained_observer.as_slice(), std::slice::from_ref(&observer),
        "candidate scoring must preserve the caller's complete equation observer");
    assert!(result.calculation.iter().all(|step| step.label != observer.label));
    (catalog, result, trace.unwrap())
}

#[test]
fn calculations_follow_the_earliest_restart_winner_without_overwriting_the_caller_observer() {
    let (_, result, trace) = traced_fixture("search-simulation/restart-earliest-score-tie");
    let comparisons: Vec<_> = trace.events.iter().filter_map(|event| match event {
        TraceEvent::RestartComparison { restart, improved, .. } => Some((*restart, *improved)),
        _ => None,
    }).collect();
    assert_eq!(comparisons, [(0, true), (1, false), (2, false)]);
    assert_eq!(value(step(&result, "Post-reorder restart selection"), "restart"), 1.0,
        "the trace must follow the retained first restart, not the last evaluated tie");
    let output = step(&result, "Action evaluation output");
    assert_eq!(value(output, "damage"), result.legacy_output[0]);
    assert_eq!(value(output, "tp_return"), result.legacy_output[1]);
    assert_eq!(value(step(&result, "Restart winner score"), "score"), result.metric);
    assert_eq!(result.calculation.iter().filter(|step| step.label == "Action evaluation output").count(), 2,
        "retain one accepted output and its separate restart comparison, not all candidates/restarts");
}

#[test]
fn calculations_retain_pre_reorder_damage_and_tp_with_the_scored_earring_provenance() {
    let (catalog, result, _) = traced_fixture("search-simulation/final-jse-reorder-without-output-rescore");
    let output = step(&result, "Action evaluation output");
    assert_eq!(value(output, "damage"), result.legacy_output[0]);
    assert_eq!(value(output, "tp_return"), result.legacy_output[1]);
    assert_eq!(value(step(&result, "Single-start search score"), "score"), result.metric);
    let provenance = step(&result, "Retained output provenance");
    assert_ne!(value(provenance, "scored_ear1"), value(provenance, "scored_ear2"));
    assert_eq!(value(provenance, "scored_ear1"), value(provenance, "final_ear2"));
    assert_eq!(value(provenance, "scored_ear2"), value(provenance, "final_ear1"));
    assert_eq!(value(provenance, "final_ear1"), f64::from(catalog.catalog.piece(result.gearset[Slot::Ear1.index()]).original_id));
    assert_eq!(value(provenance, "final_ear2"), f64::from(catalog.catalog.piece(result.gearset[Slot::Ear2.index()]).original_id));
    assert_eq!(result.calculation.iter().filter(|step| step.label == "Action evaluation output").count(), 1);
}
