#[allow(dead_code)]
mod combat_support;
#[allow(dead_code)]
mod round_support;

use std::sync::Arc;

use wsdist_core::Catalog;
use wsdist_core::actions::evaluate_round;
use wsdist_core::model::{ActionResult, CalculationStep, EvaluationContext, Metric};
use wsdist_core::rng::ScriptedTape;

use combat_support::{boundary_enemy, build_boundary, fixture_root, number, resource_root};
use round_support::assert_close;

fn operand(step: &CalculationStep, name: &str) -> f64 {
    step.values.iter().find_map(|(key, value)| (*key == name).then_some(*value))
        .unwrap_or_else(|| panic!("missing captured operand {name}"))
}

fn step_with_operands<'a>(trace: &'a [CalculationStep], names: &[&str]) -> &'a CalculationStep {
    trace.iter().find(|step| names.iter().all(|name| step.values.iter().any(|(key, _)| key == name)))
        .expect("captured numerical calculation stage")
}

fn assert_reconciled(trace: &[CalculationStep], result: &ActionResult) {
    let totals = step_with_operands(trace, &["melee", "kick", "daken", "zanshin", "physical", "magical", "damage"]);
    assert_close("captured physical damage", operand(totals, "physical"), result.physical_damage);
    assert_close("captured magical damage", operand(totals, "magical"), result.magical_damage);
    assert_close("captured total damage", operand(totals, "damage"), result.damage);
    assert_close("physical contribution sum", operand(totals, "melee") + operand(totals, "kick")
        + operand(totals, "daken") + operand(totals, "zanshin"), result.physical_damage);
    assert_close("physical and magical sum", operand(totals, "physical") + operand(totals, "magical"), result.damage);

    let tp = step_with_operands(trace, &["attack_tp", "regain_tp", "tp"]);
    assert_close("captured TP", operand(tp, "tp"), result.tp_return);
    assert_close("attack and regain TP sum", operand(tp, "attack_tp") + operand(tp, "regain_tp"), result.tp_return);

    let timing = step_with_operands(trace, &["threshold", "starting_tp", "rounds", "seconds", "time"]);
    assert_close("captured round duration", operand(timing, "seconds"), result.seconds_per_round.unwrap());
    assert_close("captured time to WS", operand(timing, "time"), result.time_to_ws.unwrap());
    assert_close("continuous rounds to WS", operand(timing, "rounds"),
        (operand(timing, "threshold") - operand(timing, "starting_tp")) / result.tp_return);
    assert_close("round duration times rounds", operand(timing, "seconds") * operand(timing, "rounds"),
        result.time_to_ws.unwrap());

    let dps = step_with_operands(trace, &["damage", "seconds", "dps"]);
    assert_close("captured DPS", operand(dps, "dps"), result.damage / result.seconds_per_round.unwrap());
    let metric = step_with_operands(trace, &["value", "invert"]);
    assert_close("captured selected metric", operand(metric, "value"), result.metric);
    assert_eq!(operand(metric, "invert"), f64::from(result.invert));
}

#[test]
fn analytical_round_tracing_preserves_frozen_results_and_exact_random_protocol() {
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());

    for (path, expected_draws) in [
        ("combat/018452.bitcode.zst", 0),
        ("combat/018467.bitcode.zst", 0),
        ("combat/018482.bitcode.zst", 0),
        ("combat/019146.bitcode.zst", 2),
    ] {
        let case = wsdist_fixtures::read_value(&fixture_root().join(path), None).unwrap();
        let compiled = &case["expected"]["value"]["compiled"];
        let tape = wsdist_fixtures::from_value::<ScriptedTape>(&compiled["draws"]).unwrap();
        let mut prepared = EvaluationContext::new(Arc::clone(&catalog), tape.clone());
        let player = build_boundary(&case, &mut prepared).unwrap();
        let enemy = boundary_enemy(&case);
        assert_eq!(prepared.random.position(), 0, "{path}: building consumes no action draws");
        let action = &case["input"]["action"];
        let starting_tp = number(&action["starting_tp"]);
        let threshold = number(&action["ws_threshold"]);

        for metric_name in ["Damage dealt", "TP return", "DPS", "TP > Damage", "Damage > TP", "Time to WS", "Damage Dealt"] {
            let mut baseline = EvaluationContext::new(Arc::clone(&catalog), tape.clone());
            baseline.state = prepared.state.clone();
            let mut traced = EvaluationContext::new(Arc::clone(&catalog), tape.clone());
            traced.state = prepared.state.clone();
            traced.calculation_trace = Some(Vec::new());
            traced.action_trace = Some(Vec::new());
            let metric = Metric::new(metric_name);
            let plain = evaluate_round(&player, &enemy, starting_tp, threshold, &metric, &mut baseline).unwrap();
            let observed = evaluate_round(&player, &enemy, starting_tp, threshold, &metric, &mut traced).unwrap();

            assert_eq!(wsdist_fixtures::to_value(&observed).unwrap(), wsdist_fixtures::to_value(&plain).unwrap(),
                "{path} / {metric_name}: tracing changes an action result");
            assert!(baseline.calculation_trace.is_none());
            assert!(traced.action_trace.as_ref().unwrap().is_empty());
            baseline.random.assert_finished().unwrap();
            traced.random.assert_finished().unwrap();
            assert_eq!(baseline.random.position(), expected_draws, "{path} / {metric_name}");
            assert_eq!(traced.random.position(), expected_draws, "{path} / {metric_name}");

            let expected = &compiled["fields"];
            for (field, actual) in [
                ("damage", observed.damage),
                ("physical_damage", observed.physical_damage),
                ("magical_damage", observed.magical_damage),
                ("tp_return", observed.tp_return),
                ("seconds_per_round", observed.seconds_per_round.unwrap()),
                ("seconds_per_ws", observed.time_to_ws.unwrap()),
            ] {
                assert_close(field, actual, number(&expected[field]));
            }
            if metric_name == action["metric"].as_str().unwrap() {
                assert_close("frozen metric", observed.metric, number(&expected["metric"]));
                assert_eq!(i64::from(observed.invert), expected["invert"].as_i64().unwrap());
            }
            let trace = traced.calculation_trace.as_ref().unwrap();
            assert_reconciled(trace, &observed);
            let hits = trace.iter().find(|step| step.values.len() == 5
                && ["main", "sub", "kick", "daken", "zanshin"].iter()
                    .all(|name| step.values.iter().any(|(key, _)| key == name)))
                .expect("captured expected landed-hit values");
            for (key, legacy_key) in [("main", "main_hits"), ("sub", "sub_hits"),
                ("kick", "kickattack_hits"), ("daken", "daken_hits"), ("zanshin", "zanshin_hits")] {
                assert_close(key, operand(hits, key), number(&compiled["final_action_locals"][legacy_key]));
            }
        }
    }
}
