use std::path::Path;
use std::sync::LazyLock;

use wsdist_fixtures::{Value, to_value};
use wsdist_core::model::{CandidatePools, GearId, Gearset, LegacyF64, Player, RandomSource, RandomStream, SimilarSwap, Slot};
use wsdist_core::optimizer::probe::{LegacyEvaluation, SearchTrace, TraceEvent};
use wsdist_core::rng::RandomSources;

use crate::support::FrozenCatalog;

#[derive(Clone, Debug)]
pub struct CountingRandom {
    pub inner: RandomSources,
    pub primary_uniforms: usize,
    pub pdif_uniforms: usize,
    pub bounded: usize,
    pub reseeds: usize,
}

impl CountingRandom {
    pub fn seeded(seed: u64) -> Self {
        Self { inner: RandomSources::from_seed(seed), primary_uniforms: 0, pdif_uniforms: 0, bounded: 0, reseeds: 0 }
    }

    pub fn uniforms(&self) -> usize { self.primary_uniforms + self.pdif_uniforms }
}

impl RandomSource for CountingRandom {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        match stream {
            RandomStream::Primary => self.primary_uniforms += 1,
            RandomStream::Pdif => self.pdif_uniforms += 1,
        }
        self.inner.uniform(stream, low, high)
    }

    fn bounded(&mut self, upper_exclusive: usize) -> usize {
        self.bounded += 1;
        self.inner.bounded(upper_exclusive)
    }

    fn reseed(&mut self, seed: u64) {
        self.reseeds += 1;
        self.inner.reseed(seed);
    }
}

pub fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(fields.into_iter().map(|(key, value)| (key.to_owned(), value)).collect())
}

pub fn num(value: f64) -> Value { to_value(LegacyF64(value)).unwrap() }

pub fn nums(values: &[f64]) -> Value { Value::Array(values.iter().copied().map(num).collect()) }

pub fn piece_value(cat: &FrozenCatalog, id: GearId) -> Value {
    let piece = cat.catalog.piece(id);
    Value::Array(vec![Value::from(piece.original_id), Value::String(piece.name2.to_string())])
}

pub fn gearset_value(cat: &FrozenCatalog, gearset: &Gearset) -> Value {
    Value::Array(Slot::ALL.iter().map(|slot| piece_value(cat, gearset[slot.index()])).collect())
}

pub fn pools_value(cat: &FrozenCatalog, pools: &CandidatePools) -> Value {
    Value::Array(pools.iter().map(|pool| Value::Array(pool.iter().map(|&id| piece_value(cat, id)).collect())).collect())
}

pub fn player_value(cat: &FrozenCatalog, player: &Player) -> Value {
    object([
        ("main_job", Value::from(player.main_job.name())),
        ("sub_job", player.sub_job.map_or(Value::Null, |job| Value::from(job.name()))),
        ("gearset", gearset_value(cat, &player.gearset)),
        ("stats", to_value(&player.stats).unwrap()),
    ])
}

pub fn random_value(random: &CountingRandom) -> Value {
    let mut continuation = random.inner.clone();
    let primary: Vec<_> = (0..4).map(|_| num(continuation.uniform(RandomStream::Primary, 0.0, 1.0))).collect();
    let pdif: Vec<_> = (0..4).map(|_| num(continuation.uniform(RandomStream::Pdif, 0.0, 1.0))).collect();
    object([
        ("primary_uniforms", Value::from(random.primary_uniforms)),
        ("pdif_uniforms", Value::from(random.pdif_uniforms)),
        ("bounded", Value::from(random.bounded)),
        ("reseeds", Value::from(random.reseeds)),
        ("continuation", object([("primary", Value::Array(primary)), ("pdif", Value::Array(pdif))])),
    ])
}

pub fn evaluation_value(evaluation: &LegacyEvaluation) -> Value {
    object([
        ("metric", num(evaluation.metric)),
        ("output", nums(&evaluation.output)),
        ("invert", Value::from(evaluation.invert)),
        ("decimals", to_value(evaluation.decimals).unwrap()),
        ("nondecimals", to_value(evaluation.nondecimals).unwrap()),
    ])
}

pub fn similar_value(cat: &FrozenCatalog, similar: &[SimilarSwap]) -> Value {
    Value::Array(similar.iter().map(|swap| object([
        ("slot", Value::from(swap.slot.name())),
        ("gear", piece_value(cat, swap.gear)),
        ("metric", num(swap.metric)),
        ("difference_percent", num(swap.difference_percent)),
    ])).collect())
}

pub fn trace_value(trace: &SearchTrace, cat: &FrozenCatalog) -> Value {
    Value::Array(trace.events.iter().map(|event| {
        let mut value = match event {
            TraceEvent::Neighborhood { combos, pools } => object([
                ("combos", to_value(combos).unwrap()), ("pools", to_value(pools).unwrap()),
            ]),
            TraceEvent::Shuffle { before, after } => object([
                ("before", to_value(before).unwrap()), ("after", to_value(after).unwrap()),
            ]),
            TraceEvent::Choice { pool, selected, .. } => object([
                ("pool", Value::Array(pool.iter().map(|&id| piece_value(cat, id)).collect())),
                ("selected", piece_value(cat, *selected)),
            ]),
            TraceEvent::PoolOrders { index, combo, orders } => object([
                ("index", to_value(index).unwrap()), ("combo", to_value(combo).unwrap()), ("orders", to_value(orders).unwrap()),
            ]),
            TraceEvent::Take { base, limit, before, after, groups, total } => object([
                ("base", to_value(base).unwrap()), ("limit", to_value(limit).unwrap()),
                ("before", to_value(before).unwrap()), ("after", to_value(after).unwrap()),
                ("groups", to_value(groups).unwrap()), ("total", to_value(total).unwrap()),
            ]),
            TraceEvent::Seek { before, after } => object([
                ("before", to_value(before).unwrap()), ("after", to_value(after).unwrap()),
            ]),
            TraceEvent::Legality { valid } => object([("valid", Value::from(*valid))]),
            TraceEvent::FastDtRejection { combo, items, achieved, thresholds } => object([
                ("combo", to_value(combo).unwrap()), ("items", to_value(items).unwrap()),
                ("achieved", nums(achieved)), ("thresholds", nums(thresholds)),
            ]),
            TraceEvent::FullPlayerDt { combo, items, achieved, thresholds, player, .. } => object([
                ("combo", to_value(combo).unwrap()), ("items", to_value(items).unwrap()),
                ("achieved", nums(achieved)), ("thresholds", nums(thresholds)),
                ("player", player_value(cat, player)),
            ]),
            TraceEvent::Metric { player, evaluation, .. } => object([
                ("player", player_value(cat, player)), ("evaluation", evaluation_value(evaluation)),
            ]),
            TraceEvent::Rescore { ids, evaluation } => object([
                ("ids", to_value(ids).unwrap()), ("evaluation", evaluation_value(evaluation)),
            ]),
            TraceEvent::ScoreChunk { base, groups, thresholds, metrics } => object([
                ("base", to_value(base).unwrap()), ("groups", to_value(groups).unwrap()),
                ("thresholds", nums(thresholds)), ("metrics", nums(metrics)),
            ]),
            TraceEvent::MapperSelection { workers, candidate_estimate, parallel, width, floor, ceiling } => object([
                ("workers", to_value(workers).unwrap()), ("candidate_estimate", to_value(candidate_estimate).unwrap()),
                ("parallel", Value::from(*parallel)), ("width", to_value(width).unwrap()),
                ("floor", to_value(floor).unwrap()), ("ceiling", to_value(ceiling).unwrap()),
            ]),
            TraceEvent::MapperScores { base, groups, total, thresholds, metrics, parallel } => object([
                ("base", to_value(base).unwrap()), ("groups", to_value(groups).unwrap()),
                ("total", to_value(total).unwrap()), ("thresholds", nums(thresholds)),
                ("metrics", nums(metrics)), ("parallel", Value::from(*parallel)),
            ]),
            TraceEvent::PassStart { ids, metric, thresholds } => object([
                ("ids", to_value(ids).unwrap()), ("metric", num(*metric)), ("thresholds", nums(thresholds)),
            ]),
            TraceEvent::Accepted { before, before_metric, combo, items, metric, batch, scanned, total, rewind } => object([
                ("before", to_value(before).unwrap()), ("before_metric", num(*before_metric)),
                ("combo", to_value(combo).unwrap()), ("items", to_value(items).unwrap()),
                ("metric", num(*metric)), ("batch", to_value(batch).unwrap()),
                ("scanned", to_value(scanned).unwrap()), ("total", to_value(total).unwrap()),
                ("rewind", to_value(rewind).unwrap()),
            ]),
            TraceEvent::PassReturn { ids, metric, output, cursor, similar } => object([
                ("ids", to_value(ids).unwrap()), ("metric", num(*metric)), ("output", nums(output)),
                ("cursor", to_value(cursor).unwrap()), ("similar", similar_value(cat, similar)),
            ]),
            TraceEvent::DtStage { thresholds, prior_achieved, ids, unchanged } => object([
                ("thresholds", nums(thresholds)), ("prior_achieved", nums(prior_achieved)),
                ("ids", to_value(ids).unwrap()), ("unchanged", Value::from(*unchanged)),
            ]),
            TraceEvent::DtStageEnd { achieved, next_thresholds, unchanged, metric } => object([
                ("achieved", nums(achieved)), ("next_thresholds", nums(next_thresholds)),
                ("unchanged", Value::from(*unchanged)), ("metric", num(*metric)),
            ]),
            TraceEvent::UnsatisfiedDt { achieved, unchanged } => object([
                ("achieved", nums(achieved)), ("unchanged", Value::from(*unchanged)),
            ]),
            TraceEvent::RestartComparison { restart, metric, incumbent, improved } => object([
                ("restart", to_value(restart).unwrap()), ("metric", num(*metric)),
                ("incumbent", num(*incumbent)), ("improved", Value::from(*improved)),
            ]),
            TraceEvent::BuildReturn { ids, metric, output, invert, achieved_before_reorder, unchanged, starting, filtered, player, .. } => object([
                ("ids", to_value(ids).unwrap()), ("metric", num(*metric)), ("output", nums(output)),
                ("invert", Value::from(*invert)), ("achieved_before_reorder", nums(achieved_before_reorder)),
                ("unchanged", Value::from(*unchanged)), ("starting", gearset_value(cat, starting)),
                ("filtered", pools_value(cat, filtered)), ("player", player_value(cat, player)),
            ]),
        };
        value.as_object_mut().unwrap().insert("kind".to_owned(), Value::from(crate::optimizer_support::event_kind(event)));
        value
    }).collect())
}

pub const NATIVE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/native");

pub fn expect_native(id: &str, legacy_sha256: &str, actual: &Value) {
    static INDEX: LazyLock<Value> = LazyLock::new(|| {
        let index = wsdist_fixtures::read_json_value(&Path::new(NATIVE_ROOT).join("index.json")).expect("native golden index");
        assert_eq!(index["schema_version"].as_u64(), Some(3), "native fixture storage schema");
        index
    });
    let entry = INDEX["cases"].as_array().unwrap().iter().find(|entry| entry["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("no native golden for {id}; regenerate with examples/regenerate_native_goldens"));
    assert_eq!(entry["legacy_sha256"].as_str().unwrap(), legacy_sha256, "{id}: native golden legacy provenance");
    let doc = wsdist_fixtures::read_value(&Path::new(NATIVE_ROOT).join(entry["path"].as_str().unwrap()),
        Some(entry["content_xxh3_128"].as_str().unwrap())).unwrap();
    assert_eq!(doc["id"], entry["id"], "{id}: native golden identity");
    assert_eq!(doc["legacy_case"]["path"], entry["legacy_path"], "{id}: native golden legacy path");
    assert_eq!(doc["legacy_case"]["sha256"], entry["legacy_sha256"], "{id}: native golden legacy source SHA-256");
    assert_json_eq(actual, &doc["observation"], id);
}

pub fn assert_json_eq(actual: &Value, expected: &Value, label: &str) {
    fn compare(actual: &Value, expected: &Value, label: &str, pointer: &str) {
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => {
                for (key, actual) in actual {
                    let pointer = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                    let expected = expected.get(key).unwrap_or_else(|| panic!("{label}{pointer}: actual {actual}, expected <missing>"));
                    compare(actual, expected, label, &pointer);
                }
                for (key, expected) in expected {
                    if !actual.contains_key(key) {
                        let pointer = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                        panic!("{label}{pointer}: actual <missing>, expected {expected}");
                    }
                }
            }
            (Value::Array(actual), Value::Array(expected)) => {
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    compare(actual, expected, label, &format!("{pointer}/{index}"));
                }
                if actual.len() != expected.len() {
                    let index = actual.len().min(expected.len());
                    panic!("{label}{pointer}/{index}: actual {:?}, expected {:?}", actual.get(index), expected.get(index));
                }
            }
            _ => assert_eq!(actual, expected, "{label}{pointer}"),
        }
    }
    compare(actual, expected, label, "");
}
