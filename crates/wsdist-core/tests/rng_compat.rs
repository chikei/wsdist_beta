mod rng_support;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde_json::json;
use wsdist_core::model::{CalcError, RandomSource, RandomStream};
use wsdist_core::rng::{RandomSources, ScriptedTape, TapeRecord};

use RandomStream::{NumbaPython, NumpyHost};

#[test]
fn all_eight_frozen_mt_stream_cases_match_words_results_consumption_and_independence() {
    let summary = rng_support::replay_all(&rng_support::fixture_root());
    assert_eq!(summary.cases, 8);
    assert_eq!(summary.twist_crossing_runs, 8);
}

#[test]
fn host_invalid_uniform_range_rejects_before_drawing_but_compiled_arithmetic_draws() {
    let mut random = RandomSources::from_seeds(11, 20240101);
    let before = random.clone();
    let error = random.try_uniform(NumpyHost, -f64::MAX, f64::MAX).unwrap_err();
    assert!(matches!(error, CalcError::Legacy { category: "OverflowError", .. }));
    assert_eq!(random, before, "host invalid interval must not consume either stream");

    let mut expected = random.clone();
    expected.next_u32(NumbaPython);
    expected.next_u32(NumbaPython);
    assert_eq!(random.try_uniform(NumbaPython, -f64::MAX, f64::MAX).unwrap(), f64::INFINITY);
    assert_eq!(random, expected, "compiled uniform still consumes exactly two words");
}

#[test]
fn tape_replays_typed_operations_with_clone_local_cursors() {
    let records: Arc<[TapeRecord]> = vec![
        TapeRecord::uniform(NumpyHost, 0.0, 1.0, 0.25),
        TapeRecord::bounded(NumpyHost, 3, 2),
        TapeRecord::reseed(NumpyHost, 11),
        TapeRecord::uniform(NumbaPython, -0.0, 1.0, 0.75),
    ].into();
    let mut tape = ScriptedTape::from_records(records);
    assert!(tape.assert_finished().is_err(), "unread evidence must be rejected");
    assert_eq!(tape.uniform(NumpyHost, 0.0, 1.0), 0.25);
    let mut cloned = tape.clone();
    assert_eq!(tape.bounded_numpy(3), 2);
    assert_eq!(cloned.position(), 1, "advancing a clone must not advance the other cursor");
    assert_eq!(cloned.bounded_numpy(3), 2);
    for replay in [&mut tape, &mut cloned] {
        replay.reseed_numpy(11);
        assert_eq!(replay.uniform(NumbaPython, -0.0, 1.0), 0.75);
        replay.assert_finished().unwrap();
    }
    assert!(tape.try_uniform(NumbaPython, -0.0, 1.0).is_err(), "an extra draw must not fall back to a generator");
    assert!(cloned.try_bounded(NumpyHost, 3).is_err());
}

#[test]
fn tape_rejects_wrong_stream_parameter_bits_operation_order_and_reseeds_without_advancing() {
    let mut tape = ScriptedTape::new(vec![
        TapeRecord::uniform(NumpyHost, -0.0, 1.0, 0.125),
        TapeRecord::bounded(NumpyHost, 7, 6),
        TapeRecord::reseed(NumbaPython, 20240101),
    ]);
    assert!(tape.try_uniform(NumbaPython, -0.0, 1.0).is_err());
    assert!(tape.try_uniform(NumpyHost, 0.0, 1.0).is_err(), "signed-zero parameters are distinct");
    assert!(tape.try_uniform(NumpyHost, -0.0, f64::from_bits(1.0_f64.to_bits() + 1)).is_err());
    assert!(tape.try_bounded(NumpyHost, 7).is_err(), "reordered operations must be rejected");
    assert_eq!(tape.position(), 0);
    assert_eq!(tape.try_uniform(NumpyHost, -0.0, 1.0).unwrap(), 0.125);
    assert!(tape.try_bounded(NumbaPython, 7).is_err());
    assert!(tape.try_bounded(NumpyHost, 8).is_err());
    assert!(tape.try_uniform(NumpyHost, -0.0, 1.0).is_err(), "a repeated draw is an order violation");
    assert_eq!(tape.position(), 1);
    assert_eq!(tape.try_bounded(NumpyHost, 7).unwrap(), 6);
    assert!(tape.try_reseed(NumpyHost, 20240101).is_err());
    assert!(tape.try_reseed(NumbaPython, 11).is_err());
    assert_eq!(tape.position(), 2);
    assert!(tape.assert_finished().is_err(), "omitting the reseed leaves unused evidence");
    tape.try_reseed(NumbaPython, 20240101).unwrap();
    tape.assert_finished().unwrap();
}

#[test]
fn infallible_trait_calls_panic_on_replay_protocol_errors() {
    let mut exhausted = ScriptedTape::new(Vec::new());
    assert!(catch_unwind(AssertUnwindSafe(|| exhausted.uniform(NumpyHost, 0.0, 1.0))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| exhausted.bounded_numpy(1))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| exhausted.reseed_numpy(0))).is_err());

    let mut mismatched = ScriptedTape::new(vec![TapeRecord::uniform(NumpyHost, 0.0, 1.0, 0.5)]);
    assert!(catch_unwind(AssertUnwindSafe(|| mismatched.uniform(NumbaPython, 0.0, 1.0))).is_err());
    assert_eq!(mismatched.position(), 0);
    assert_eq!(mismatched.uniform(NumpyHost, 0.0, 1.0), 0.5);
    mismatched.assert_finished().unwrap();
}

#[test]
fn tape_json_decoding_preserves_signed_zero_and_tagged_nonfinite_parameters_and_returns() {
    let records: serde_json::Value = serde_json::from_str(r#"[
        {"stream":"NumpyHost","operation":"uniform","parameters":[-0.0,{"nonfinite":"+Infinity"}],"value":{"nonfinite":"NaN"}},
        {"stream":"NumbaPython","operation":"uniform","low":{"nonfinite":"-Infinity"},"high":{"nonfinite":"+Infinity"},"value":-0.0},
        {"stream":"NumbaPython","operation":"uniform","parameters":[{"nonfinite":"NaN"},1.0],"value":{"nonfinite":"-Infinity"}},
        {"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":2},
        {"stream":"NumbaPython","operation":"reseed","parameters":[4294967295],"value":null}
    ]"#).unwrap();
    let mut tape = ScriptedTape::from_json_records(&records).unwrap();
    assert!(tape.try_uniform(NumpyHost, 0.0, f64::INFINITY).is_err());
    assert!(tape.try_uniform(NumpyHost, -0.0, f64::INFINITY).unwrap().is_nan());
    assert_eq!(tape.try_uniform(NumbaPython, f64::NEG_INFINITY, f64::INFINITY).unwrap().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(tape.try_uniform(NumbaPython, f64::NAN, 1.0).unwrap(), f64::NEG_INFINITY);
    assert_eq!(tape.try_bounded(NumpyHost, 3).unwrap(), 2);
    tape.try_reseed(NumbaPython, u32::MAX).unwrap();
    tape.assert_finished().unwrap();
}

#[test]
fn malformed_or_unsupported_tape_evidence_is_rejected_at_decode() {
    let invalid = [
        json!({}),
        json!([null]),
        json!([{"stream":"NumpyHost","operation":"random53","value":0.5}]),
        json!([{"stream":"Unknown","operation":"uniform","parameters":[0,1],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1]}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1,2],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1],"low":0,"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[{"nonfinite":"Infinity"},1],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":["0",1],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[0],"value":0}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":3}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":-1}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[4294967296_u64]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[1],"value":0}]),
    ];
    for records in invalid { assert!(ScriptedTape::from_json_records(&records).is_err(), "accepted {records}"); }
}

#[test]
fn invalid_typed_bounded_results_cannot_be_returned_or_consumed() {
    for record in [TapeRecord::bounded(NumpyHost, 0, 0), TapeRecord::bounded(NumpyHost, 3, 3)] {
        let mut tape = ScriptedTape::new(vec![record]);
        let upper = match record { TapeRecord::Bounded { upper_exclusive, .. } => upper_exclusive, _ => unreachable!() };
        assert!(tape.try_bounded(NumpyHost, upper).is_err());
        assert_eq!(tape.position(), 0);
        assert!(tape.assert_finished().is_err());
    }
}
