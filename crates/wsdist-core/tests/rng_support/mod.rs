use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};
use serde_json::Value;
use wsdist_core::model::{CalcError, LegacyF64, RandomSource, RandomStream};
use wsdist_core::rng::RandomSources;

#[path = "../support/fixture_json.rs"]
mod fixture_json;

#[derive(Deserialize)]
struct Index { cases: Vec<IndexedCase> }

#[derive(Deserialize)]
struct IndexedCase { id: String, operation: String, path: String }

#[derive(Deserialize)]
pub struct Fixture {
    pub id: String,
    input: Inputs,
    expected: Expected,
}

#[derive(Deserialize)]
struct Inputs { host_seed: u32, numba_seed: u32, events: Vec<Event> }

#[derive(Deserialize)]
struct Expected { outcome: String, value: Evidence }

#[derive(Deserialize)]
struct Evidence {
    initial_state: States,
    final_state: States,
    events: Vec<Record>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Operation { Random53, Uniform, Bounded, Choice, Shuffle, Reseed }

#[derive(Deserialize)]
struct Event {
    #[serde(deserialize_with = "stream")]
    stream: RandomStream,
    operation: Operation,
    #[serde(default = "one")]
    count: usize,
    low: Option<LegacyF64>,
    high: Option<LegacyF64>,
    upper_exclusive: Option<usize>,
    values: Option<Vec<i64>>,
    seed: Option<u32>,
}

#[derive(Deserialize)]
struct Record {
    index: usize,
    input: Event,
    outcome: String,
    output: Value,
    state_before: States,
    state_after: States,
    consumed_tempered_u32: Option<Vec<u32>>,
    consumed_u32_count: Option<usize>,
    exception: Option<Exception>,
}

#[derive(Deserialize)]
struct Exception { class: String, message: String }

#[derive(Deserialize)]
struct States {
    #[serde(rename = "NumpyHost")]
    host: Snapshot,
    #[serde(rename = "NumbaPython")]
    compiled: Snapshot,
}

#[derive(Deserialize)]
struct Snapshot { words: Vec<u32>, position: usize }

#[derive(Default, Debug)]
pub struct Summary {
    pub cases: usize,
    pub events: usize,
    pub errors: usize,
    pub raw_words: usize,
    pub twist_crossing_runs: usize,
}

fn one() -> usize { 1 }

fn stream<'de, D: Deserializer<'de>>(deserializer: D) -> Result<RandomStream, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "NumpyHost" => Ok(RandomStream::NumpyHost),
        "NumbaPython" => Ok(RandomStream::NumbaPython),
        name => Err(serde::de::Error::custom(format!("unknown RNG stream {name:?}"))),
    }
}

pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

fn document<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    fixture_json::read(path, None).unwrap_or_else(|error| panic!("{error}"))
}

pub fn fixtures(root: &Path) -> Vec<Fixture> {
    let index: Index = document(&root.join("players.json"));
    index.cases.into_iter().filter(|entry| entry.operation == "rng").map(|entry| {
        let fixture: Fixture = document(&root.join(entry.path));
        assert_eq!(fixture.id, entry.id, "RNG fixture index identity");
        fixture
    }).collect()
}

enum Output { Floats(Vec<f64>), Integers(Vec<i64>), Empty }

fn execute(random: &mut RandomSources, event: &Event) -> Result<Output, CalcError> {
    match event.operation {
        Operation::Random53 => Ok(Output::Floats((0..event.count).map(|_| random.random53(event.stream)).collect())),
        Operation::Uniform => {
            let low = event.low.expect("uniform low").0;
            let high = event.high.expect("uniform high").0;
            Ok(Output::Floats((0..event.count).map(|_| random.uniform(event.stream, low, high)).collect()))
        }
        Operation::Bounded => {
            let upper = event.upper_exclusive.expect("bounded upper");
            let mut output = Vec::with_capacity(event.count);
            for _ in 0..event.count {
                let value = if event.stream == RandomStream::NumpyHost && upper != 0 {
                    random.bounded_numpy(upper)
                } else {
                    random.bounded(event.stream, upper)?
                };
                output.push(value as i64);
            }
            Ok(Output::Integers(output))
        }
        Operation::Choice => {
            let values = event.values.as_ref().expect("choice values");
            let mut output = Vec::with_capacity(event.count);
            for _ in 0..event.count { output.push(values[random.choose_index(values.len())?]); }
            Ok(Output::Integers(output))
        }
        Operation::Shuffle => {
            let mut values = event.values.clone().expect("shuffle values");
            random.shuffle(event.stream, &mut values);
            Ok(Output::Integers(values))
        }
        Operation::Reseed => {
            let seed = event.seed.expect("reseed seed");
            if event.stream == RandomStream::NumpyHost { random.reseed_numpy(seed); }
            else { random.reseed(event.stream, seed); }
            Ok(Output::Empty)
        }
    }
}

fn assert_output(label: &str, actual: Output, expected: &Value) {
    match actual {
        Output::Floats(actual) => {
            let expected: Vec<LegacyF64> = serde_json::from_value(expected.clone()).expect("captured floats");
            assert_eq!(actual.len(), expected.len(), "{label}: float draw count");
            for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
                assert_eq!(actual.to_bits(), expected.0.to_bits(), "{label}: draw {index}: {actual:?} != {:?}", expected.0);
            }
        }
        Output::Integers(actual) => {
            let expected: Vec<i64> = serde_json::from_value(expected.clone()).expect("captured integers");
            assert_eq!(actual, expected, "{label}: integer draws/permutation");
        }
        Output::Empty => assert_eq!(expected, &serde_json::json!([]), "{label}: reseed return"),
    }
}

// Compare public captured state evidence to real public raw-word draws. The
// crate's private state is never read, reconstructed, or imported. Tempering
// only the already-captured visible suffix avoids a second test-side RNG.
fn assert_snapshot(label: &str, random: &RandomSources, stream: RandomStream, snapshot: &Snapshot) {
    assert_eq!(snapshot.words.len(), 624, "{label}: captured state width");
    assert!(snapshot.position <= 624, "{label}: captured cursor");
    let mut probe = random.clone();
    for (offset, &word) in snapshot.words[snapshot.position..].iter().enumerate() {
        let mut expected = word;
        expected ^= expected >> 11;
        expected ^= (expected << 7) & 0x9d2c_5680;
        expected ^= (expected << 15) & 0xefc6_0000;
        expected ^= expected >> 18;
        assert_eq!(probe.next_u32(stream), expected, "{label}: {stream:?} visible state word {}", snapshot.position + offset);
    }
}

fn assert_states(label: &str, random: &RandomSources, states: &States) {
    assert_snapshot(label, random, RandomStream::NumpyHost, &states.host);
    assert_snapshot(label, random, RandomStream::NumbaPython, &states.compiled);
}

fn assert_event_input(label: &str, input: &Event, recorded: &Event) {
    assert_eq!(input.stream, recorded.stream, "{label}: stream");
    assert_eq!(input.operation, recorded.operation, "{label}: operation");
    assert_eq!(input.count, recorded.count, "{label}: count");
    assert_eq!(input.low.map(|value| value.0.to_bits()), recorded.low.map(|value| value.0.to_bits()), "{label}: low");
    assert_eq!(input.high.map(|value| value.0.to_bits()), recorded.high.map(|value| value.0.to_bits()), "{label}: high");
    assert_eq!(input.upper_exclusive, recorded.upper_exclusive, "{label}: upper");
    assert_eq!(input.values, recorded.values, "{label}: values");
    assert_eq!(input.seed, recorded.seed, "{label}: seed");
}

pub fn replay(fixture: &Fixture) -> Summary {
    assert_eq!(fixture.expected.outcome, "result", "{}: captured RNG outcome", fixture.id);
    let mut random = RandomSources::from_seeds(fixture.input.host_seed, fixture.input.numba_seed);
    assert_states(&fixture.id, &random, &fixture.expected.value.initial_state);
    assert_eq!(fixture.input.events.len(), fixture.expected.value.events.len(), "{}: operation count", fixture.id);
    let mut summary = Summary { cases: 1, ..Summary::default() };
    for (index, (event, record)) in fixture.input.events.iter().zip(&fixture.expected.value.events).enumerate() {
        let label = format!("{} event {index} {:?} {:?}", fixture.id, event.stream, event.operation);
        assert_eq!(record.index, index, "{label}: index");
        assert_event_input(&label, event, &record.input);
        assert_states(&label, &random, &record.state_before);
        let mut raw_only = random.clone();
        if event.operation == Operation::Reseed {
            raw_only.reseed(event.stream, event.seed.expect("reseed seed"));
        } else {
            let words = record.consumed_tempered_u32.as_ref().expect("captured raw MT words");
            assert_eq!(Some(words.len()), record.consumed_u32_count, "{label}: captured raw word count");
            for (offset, &word) in words.iter().enumerate() {
                assert_eq!(raw_only.next_u32(event.stream), word, "{label}: tempered raw word {offset}");
            }
            summary.raw_words += words.len();
        }
        match (record.outcome.as_str(), execute(&mut random, event)) {
            ("success", Ok(actual)) => assert_output(&label, actual, &record.output),
            ("error", Err(CalcError::Legacy { category, message, .. })) => {
                let exception = record.exception.as_ref().expect("captured RNG exception");
                assert_eq!(category, exception.class, "{label}: error category");
                assert_eq!(message, exception.message, "{label}: error message");
                assert_eq!(record.output, serde_json::json!([]), "{label}: rejected before first draw");
                summary.errors += 1;
            }
            (expected, actual) => panic!("{label}: expected {expected}, got {}", match actual {
                Ok(_) => "success".to_owned(), Err(error) => error.to_string(),
            }),
        }
        // Equality includes both complete private states via Mt's public Eq
        // contract: exactly the captured consumption, and no inactive mutation.
        assert_eq!(random, raw_only, "{label}: exact consumption and independent stream state");
        assert_states(&label, &random, &record.state_after);
        summary.events += 1;
        if event.operation == Operation::Uniform && event.count == 315 {
            assert_eq!(record.consumed_u32_count, Some(630), "{label}: full twist crossing");
            summary.twist_crossing_runs += 1;
        }
    }
    assert_states(&fixture.id, &random, &fixture.expected.value.final_state);
    summary
}

pub fn replay_all(root: &Path) -> Summary {
    let fixtures = fixtures(root);
    let expected_ids = [
        "rng.two_streams.seed0", "rng.two_streams.seed1", "rng.two_streams.seed11", "rng.two_streams.seed20240101",
        "rng.independence.host0.compiled1", "rng.independence.host1.compiled11",
        "rng.independence.host11.compiled20240101", "rng.independence.host20240101.compiled0",
    ];
    assert_eq!(fixtures.iter().map(|fixture| fixture.id.as_str()).collect::<Vec<_>>(), expected_ids, "all eight frozen RNG cases");
    let mut total = Summary::default();
    for fixture in &fixtures {
        let current = replay(fixture);
        total.cases += current.cases;
        total.events += current.events;
        total.errors += current.errors;
        total.raw_words += current.raw_words;
        total.twist_crossing_runs += current.twist_crossing_runs;
    }
    total
}
