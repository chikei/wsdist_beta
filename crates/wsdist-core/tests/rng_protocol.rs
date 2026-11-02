use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;
use wsdist_core::model::{RandomSource, RandomStream};
use wsdist_core::rng::{RandomSources, ScriptedTape, TapeRecord};
use wsdist_fixtures::{Number as FixtureNumber, Value as FixtureValue};

use RandomStream::{Pdif, Primary};

#[test]
fn seeded_streams_are_repeatable_distinct_and_independent() {
    let mut first = RandomSources::from_seed(11);
    let mut second = RandomSources::from_seed(11);
    let primary: [u64; 16] = std::array::from_fn(|_| first.uniform(Primary, 0.0, 1.0).to_bits());
    let repeated_primary: [u64; 16] = std::array::from_fn(|_| second.uniform(Primary, 0.0, 1.0).to_bits());
    let pdif: [u64; 16] = std::array::from_fn(|_| first.uniform(Pdif, 0.0, 1.0).to_bits());
    let repeated_pdif: [u64; 16] = std::array::from_fn(|_| second.uniform(Pdif, 0.0, 1.0).to_bits());
    assert_eq!(primary, repeated_primary);
    assert_eq!(pdif, repeated_pdif);
    assert_ne!(primary, pdif);

    let mut untouched_pdif = first.clone();
    for _ in 0..32 { first.uniform(Primary, 0.0, 1.0); }
    for _ in 0..16 {
        assert_eq!(first.uniform(Pdif, 0.0, 1.0).to_bits(), untouched_pdif.uniform(Pdif, 0.0, 1.0).to_bits());
    }
}

#[test]
fn reseeding_resets_both_native_streams() {
    let mut random = RandomSources::from_seed(11);
    for stream in [Primary, Pdif] {
        for _ in 0..16 { random.uniform(stream, 0.0, 1.0); }
    }
    random.reseed(11);
    assert_eq!(random, RandomSources::from_seed(11));
}

#[test]
fn bounded_draws_stay_in_range_and_singleton_ranges_consume_nothing() {
    let mut random = RandomSources::from_seed(11);
    let before = random.clone();
    assert_eq!(random.bounded(1), 0);
    assert_eq!(random, before);
    for upper in [2, 3, 7, 1_usize << 40] {
        for _ in 0..1024 { assert!(random.bounded(upper) < upper); }
    }
}

#[test]
#[should_panic(expected = "bounded random range must be nonempty")]
fn bounded_empty_ranges_panic() {
    RandomSources::from_seed(11).bounded(0);
}

#[test]
fn native_uniforms_use_half_open_intervals_and_consume_zero_width_draws() {
    let mut random = RandomSources::from_seed(11);
    for _ in 0..1024 { assert!((10.0..200.0).contains(&random.uniform(Primary, 10.0, 200.0))); }
    let before = random.clone();
    assert_eq!(random.uniform(Primary, 1.0, 1.0), 1.0);
    assert_ne!(random, before, "zero-width uniform must still consume a draw");
}

#[test]
fn seed_zero_uniform_bits_pin_the_chacha8_key_layout_and_stream_ids() {
    let mut random = RandomSources::from_seed(0);
    let primary: [u64; 4] = std::array::from_fn(|_| random.uniform(Primary, 0.0, 1.0).to_bits());
    let pdif: [u64; 4] = std::array::from_fn(|_| random.uniform(Pdif, 0.0, 1.0).to_bits());
    assert_eq!(primary, [0x3fea_c80b_f125_fde0, 0x3fe4_34a1_23fd_170b, 0x3fcd_bfcd_6761_8740, 0x3fbe_1a71_ef88_e118]);
    assert_eq!(pdif, [0x3fe9_40d0_6f36_6971, 0x3fea_29d1_a9ef_9a86, 0x3fd0_9a6f_b073_3b68, 0x3fe3_6b3a_f41d_95c1]);
}

#[test]
fn tape_replays_typed_operations_with_clone_local_cursors() {
    let records: Arc<[TapeRecord]> = vec![
        TapeRecord::uniform(Primary, 0.0, 1.0, 0.25),
        TapeRecord::bounded(Primary, 3, 2),
        TapeRecord::reseed(Primary, 11),
        TapeRecord::uniform(Pdif, -0.0, 1.0, 0.75),
    ].into();
    let mut tape = ScriptedTape::from_records(records);
    assert!(tape.assert_finished().is_err(), "unread evidence must be rejected");
    assert_eq!(tape.uniform(Primary, 0.0, 1.0), 0.25);
    let mut cloned = tape.clone();
    assert_eq!(tape.bounded(3), 2);
    assert_eq!(cloned.position(), 1, "advancing a clone must not advance the other cursor");
    assert_eq!(cloned.bounded(3), 2);
    for replay in [&mut tape, &mut cloned] {
        replay.reseed(11);
        assert_eq!(replay.uniform(Pdif, -0.0, 1.0), 0.75);
        replay.assert_finished().unwrap();
    }
    assert!(tape.try_uniform(Pdif, -0.0, 1.0).is_err(), "an extra draw must not fall back to a generator");
    assert!(cloned.try_bounded(Primary, 3).is_err());
}

#[test]
fn tape_rejects_wrong_stream_parameter_bits_operation_order_and_reseeds_without_advancing() {
    let mut tape = ScriptedTape::new(vec![
        TapeRecord::uniform(Primary, -0.0, 1.0, 0.125),
        TapeRecord::bounded(Primary, 7, 6),
        TapeRecord::reseed(Pdif, 20240101),
    ]);
    assert!(tape.try_uniform(Pdif, -0.0, 1.0).is_err());
    assert!(tape.try_uniform(Primary, 0.0, 1.0).is_err(), "signed-zero parameters are distinct");
    assert!(tape.try_uniform(Primary, -0.0, f64::from_bits(1.0_f64.to_bits() + 1)).is_err());
    assert!(tape.try_bounded(Primary, 7).is_err(), "reordered operations must be rejected");
    assert_eq!(tape.position(), 0);
    assert_eq!(tape.try_uniform(Primary, -0.0, 1.0).unwrap(), 0.125);
    assert!(tape.try_bounded(Pdif, 7).is_err());
    assert!(tape.try_bounded(Primary, 8).is_err());
    assert!(tape.try_uniform(Primary, -0.0, 1.0).is_err(), "a repeated draw is an order violation");
    assert_eq!(tape.position(), 1);
    assert_eq!(tape.try_bounded(Primary, 7).unwrap(), 6);
    assert!(tape.try_reseed(Primary, 20240101).is_err());
    assert!(tape.try_reseed(Pdif, 11).is_err());
    assert_eq!(tape.position(), 2);
    assert!(tape.assert_finished().is_err(), "omitting the reseed leaves unused evidence");
    tape.try_reseed(Pdif, 20240101).unwrap();
    tape.assert_finished().unwrap();
}

#[test]
fn infallible_trait_calls_panic_on_replay_protocol_errors() {
    let mut exhausted = ScriptedTape::new(Vec::new());
    assert!(catch_unwind(AssertUnwindSafe(|| exhausted.uniform(Primary, 0.0, 1.0))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| exhausted.bounded(1))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| exhausted.reseed(0))).is_err());

    let mut mismatched = ScriptedTape::new(vec![TapeRecord::uniform(Primary, 0.0, 1.0, 0.5)]);
    assert!(catch_unwind(AssertUnwindSafe(|| mismatched.uniform(Pdif, 0.0, 1.0))).is_err());
    assert_eq!(mismatched.position(), 0);
    assert_eq!(mismatched.uniform(Primary, 0.0, 1.0), 0.5);
    mismatched.assert_finished().unwrap();
}

#[test]
fn tape_json_decoding_preserves_signed_zero_and_tagged_nonfinite_parameters_and_returns() {
    let records: serde_json::Value = serde_json::from_str(r#"[
        {"stream":"NumpyHost","operation":"uniform","parameters":[-0.0,{"nonfinite":"+Infinity"}],"value":{"nonfinite":"NaN"}},
        {"stream":"NumbaPython","operation":"uniform","low":{"nonfinite":"-Infinity"},"high":{"nonfinite":"+Infinity"},"value":-0.0},
        {"stream":"NumbaPython","operation":"uniform","parameters":[{"nonfinite":"NaN"},1.0],"value":{"nonfinite":"-Infinity"}},
        {"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":2},
        {"stream":"NumbaPython","operation":"reseed","parameters":[4294967295],"value":null},
        {"stream":"NumpyHost","operation":"reseed","parameters":[4294967296]},
        {"stream":"NumpyHost","operation":"reseed","parameters":[18446744073709551615]}
    ]"#).unwrap();
    let native = wsdist_fixtures::to_value(&records).unwrap();
    for mut tape in [
        ScriptedTape::deserialize(&records).unwrap(),
        wsdist_fixtures::from_value::<ScriptedTape>(&native).unwrap(),
    ] {
        assert!(tape.try_uniform(Primary, 0.0, f64::INFINITY).is_err());
        assert!(tape.try_uniform(Primary, -0.0, f64::INFINITY).unwrap().is_nan());
        assert_eq!(tape.try_uniform(Pdif, f64::NEG_INFINITY, f64::INFINITY).unwrap().to_bits(), (-0.0_f64).to_bits());
        assert_eq!(tape.try_uniform(Pdif, f64::NAN, 1.0).unwrap(), f64::NEG_INFINITY);
        assert_eq!(tape.try_bounded(Primary, 3).unwrap(), 2);
        tape.try_reseed(Pdif, u64::from(u32::MAX)).unwrap();
        tape.reseed(4_294_967_296);
        tape.reseed(u64::MAX);
        tape.assert_finished().unwrap();
    }
}

#[test]
fn malformed_or_unsupported_tape_evidence_is_rejected_at_decode() {
    let invalid = [
        json!({}),
        json!([null]),
        json!([{"stream":"NumpyHost","operation":"unsupported","value":0.5}]),
        json!([{"stream":"Unknown","operation":"uniform","parameters":[0,1],"value":0.5}]),
        json!([{"stream":"Primary","operation":"uniform","parameters":[0,1],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1]}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1,2],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1],"low":0,"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1],"high":1,"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[0,1],"value":true}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":[{"nonfinite":"Infinity"},1],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"uniform","parameters":["0",1],"value":0.5}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[0],"value":0}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":3}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":-1}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3.0],"value":2}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":2.0}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[true],"value":0}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":false}]),
        json!([{"stream":"NumpyHost","operation":"bounded","parameters":[3],"upper_exclusive":3,"value":2}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[-1]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[1.5]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[1.0]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[true]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[null]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":["1"]}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[1],"value":0}]),
        json!([{"stream":"NumpyHost","operation":"reseed","parameters":[1],"seed":1}]),
    ];
    for records in invalid {
        assert!(ScriptedTape::deserialize(&records).is_err(), "accepted JSON {records}");
        let native = wsdist_fixtures::to_value(&records).unwrap();
        assert!(wsdist_fixtures::from_value::<ScriptedTape>(&native).is_err(), "accepted native {records}");
    }
}

#[test]
fn invalid_typed_bounded_results_cannot_be_returned_or_consumed() {
    for record in [TapeRecord::bounded(Primary, 0, 0), TapeRecord::bounded(Primary, 3, 3)] {
        let mut tape = ScriptedTape::new(vec![record]);
        let upper = match record { TapeRecord::Bounded { upper_exclusive, .. } => upper_exclusive, _ => unreachable!() };
        assert!(tape.try_bounded(Primary, upper).is_err());
        assert_eq!(tape.position(), 0);
        assert!(tape.assert_finished().is_err());
    }
}

#[test]
fn native_tape_integer_fields_accept_signed_integers_but_not_floats_or_booleans() {
    let mut records = wsdist_fixtures::to_value(json!([
        {"stream":"NumpyHost","operation":"bounded","parameters":[3],"value":2},
        {"stream":"NumbaPython","operation":"reseed","parameters":[11]},
    ])).unwrap();
    records[0]["parameters"][0] = FixtureValue::Number(FixtureNumber::I64(3));
    records[0]["value"] = FixtureValue::Number(FixtureNumber::I64(2));
    records[1]["parameters"][0] = FixtureValue::Number(FixtureNumber::I64(11));
    let mut tape = wsdist_fixtures::from_value::<ScriptedTape>(&records).unwrap();
    assert_eq!(tape.try_bounded(Primary, 3).unwrap(), 2);
    tape.try_reseed(Pdif, 11).unwrap();
    tape.assert_finished().unwrap();

    for invalid in [
        FixtureValue::Number(FixtureNumber::I64(-1)),
        FixtureValue::Number(FixtureNumber::F64(1.0)),
        FixtureValue::Bool(true),
    ] {
        for (record, field, parameter) in [(0, "parameters", true), (0, "value", false), (1, "parameters", true)] {
            let mut malformed = records.clone();
            if parameter { malformed[record][field][0] = invalid.clone(); }
            else { malformed[record][field] = invalid.clone(); }
            assert!(wsdist_fixtures::from_value::<ScriptedTape>(&malformed).is_err(), "accepted {malformed}");
        }
    }
}

#[test]
fn tape_streaming_decoding_keeps_last_fields_and_ignores_unrelated_valid_metadata() {
    let text = r#"[
        {"stream":"Unknown","stream":"NumpyHost","operation":"unsupported","operation":"uniform",
         "parameters":[0,1],"value":"overwritten","value":0.25,
         "seed":{"irrelevant":[null,false,{"nested":[]}]},"capture":{"arguments":[1,"x"]}},
        {"stream":"NumpyHost","operation":"bounded","upper_exclusive":3,"value":1},
        {"stream":"NumbaPython","operation":"reseed","seed":11,
         "low":["unused",{"nested":true}],"value":null}
    ]"#;
    let native: wsdist_fixtures::Value = serde_json::from_str(text).unwrap();
    for mut tape in [
        serde_json::from_str::<ScriptedTape>(text).unwrap(),
        wsdist_fixtures::from_value::<ScriptedTape>(&native).unwrap(),
    ] {
        assert_eq!(tape.try_uniform(Primary, 0.0, 1.0).unwrap(), 0.25);
        assert_eq!(tape.try_bounded(Primary, 3).unwrap(), 1);
        tape.try_reseed(Pdif, 11).unwrap();
        tape.assert_finished().unwrap();
    }
}

#[test]
fn discarded_tape_fields_still_require_fully_valid_json() {
    let malformed = [
        br#"[0,"\uD800"]"#.to_vec(),
        br#"{"first":0,"ignored":"\uDC00"}"#.to_vec(),
        br#"{"first":0,"\uD800":0}"#.to_vec(),
        b"[0,\"bad\xff\"]".to_vec(),
        b"{\"first\":0,\"bad\xff\":0}".to_vec(),
        b"1e400".to_vec(),
        b"[0,1e400]".to_vec(),
        br#"{"first":0,"ignored":1e400}"#.to_vec(),
        format!("{}0{}", "[".repeat(160), "]".repeat(160)).into_bytes(),
    ];
    for field in ["capture", "seed"] {
        for value in &malformed {
            let mut text = format!(
                r#"[{{"stream":"NumpyHost","operation":"uniform","parameters":[0,1],"value":0.5,"{field}":"#
            ).into_bytes();
            text.extend_from_slice(value);
            text.extend_from_slice(b"}]");
            assert!(serde_json::from_slice::<ScriptedTape>(&text).is_err(), "accepted {text:?}");
        }
    }
}
