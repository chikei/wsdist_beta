#[path = "search_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[allow(dead_code)]
mod optimizer_support;

use std::sync::atomic::AtomicBool;

use wsdist_core::model::{CalculationStep, OptimizeResult, Slot};
use wsdist_core::optimizer::probe::{SearchTrace, TraceEvent, optimize_raw};

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
    let mut baseline_engine = catalog.context(&input["seeds"]);
    let request = optimizer_support::raw_request(&catalog, input, &baseline_engine.state, id);
    let mut baseline_trace = Some(SearchTrace::default());
    let baseline = optimize_raw(&request, &mut baseline_engine, &AtomicBool::new(false),
        &mut |_| {}, &mut baseline_trace).unwrap();
    assert!(baseline.calculation.is_empty());
    assert!(baseline_engine.calculation_trace.is_none());

    let observer = CalculationStep { label: "Caller observer", formula: "caller = {caller}", values: vec![("caller", 17.0)] };
    let mut engine = catalog.context(&input["seeds"]);
    engine.calculation_trace = Some(vec![observer.clone()]);
    let mut trace = Some(SearchTrace::default());
    let result = optimize_raw(&request, &mut engine, &AtomicBool::new(false), &mut |_| {}, &mut trace).unwrap();

    optimizer_support::assert_build(&result, &fixture["expected"]["value"], &catalog, &engine.state, id);
    assert_eq!(result.gearset, baseline.gearset);
    assert_eq!(result.metric, baseline.metric);
    assert_eq!(result.legacy_output, baseline.legacy_output);
    assert_eq!(result.termination, baseline.termination);
    assert_eq!(result.accepted.len(), baseline.accepted.len());
    assert_eq!((result.stages, result.passes, result.scored), (baseline.stages, baseline.passes, baseline.scored));
    assert_eq!(engine.random, baseline_engine.random, "display tracing must not change RNG continuation");
    assert_eq!(trace.as_ref().unwrap().events.len(), baseline_trace.as_ref().unwrap().events.len(),
        "display tracing must not add action evaluations or search events");
    let retained_observer = engine.calculation_trace.as_ref().unwrap();
    assert_eq!(retained_observer.len(), 1, "candidate evaluations must not write into the caller observer");
    assert_eq!(retained_observer[0].label, observer.label);
    assert_eq!(retained_observer[0].formula, observer.formula);
    assert_eq!(retained_observer[0].values, observer.values);
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
