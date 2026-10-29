use std::path::Path;
use std::sync::LazyLock;

use serde_json::{Value, json};
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

pub fn num(value: f64) -> Value { serde_json::to_value(LegacyF64(value)).unwrap() }

pub fn nums(values: &[f64]) -> Value { Value::Array(values.iter().copied().map(num).collect()) }

pub fn piece_value(cat: &FrozenCatalog, id: GearId) -> Value {
    let piece = cat.catalog.piece(id);
    json!([piece.original_id, piece.name2])
}

pub fn gearset_value(cat: &FrozenCatalog, gearset: &Gearset) -> Value {
    Value::Array(Slot::ALL.iter().map(|slot| piece_value(cat, gearset[slot.index()])).collect())
}

pub fn pools_value(cat: &FrozenCatalog, pools: &CandidatePools) -> Value {
    Value::Array(pools.iter().map(|pool| Value::Array(pool.iter().map(|&id| piece_value(cat, id)).collect())).collect())
}

pub fn player_value(cat: &FrozenCatalog, player: &Player) -> Value {
    json!({
        "main_job": player.main_job.name(),
        "sub_job": player.sub_job.map(|job| job.name()),
        "gearset": gearset_value(cat, &player.gearset),
        "stats": serde_json::to_value(&player.stats).unwrap(),
    })
}

pub fn random_value(random: &CountingRandom) -> Value {
    let mut continuation = random.inner.clone();
    let primary: Vec<_> = (0..4).map(|_| num(continuation.uniform(RandomStream::Primary, 0.0, 1.0))).collect();
    let pdif: Vec<_> = (0..4).map(|_| num(continuation.uniform(RandomStream::Pdif, 0.0, 1.0))).collect();
    json!({
        "primary_uniforms": random.primary_uniforms,
        "pdif_uniforms": random.pdif_uniforms,
        "bounded": random.bounded,
        "reseeds": random.reseeds,
        "continuation": {"primary": primary, "pdif": pdif},
    })
}

pub fn evaluation_value(evaluation: &LegacyEvaluation) -> Value {
    json!({"metric": num(evaluation.metric), "output": nums(&evaluation.output),
        "invert": evaluation.invert, "decimals": evaluation.decimals, "nondecimals": evaluation.nondecimals})
}

pub fn similar_value(cat: &FrozenCatalog, similar: &[SimilarSwap]) -> Value {
    Value::Array(similar.iter().map(|swap| json!({
        "slot": swap.slot.name(), "gear": piece_value(cat, swap.gear),
        "metric": num(swap.metric), "difference_percent": num(swap.difference_percent),
    })).collect())
}

pub fn trace_value(trace: &SearchTrace, cat: &FrozenCatalog) -> Value {
    Value::Array(trace.events.iter().map(|event| {
        let mut value = match event {
            TraceEvent::Neighborhood { combos, pools } => json!({"combos": combos, "pools": pools}),
            TraceEvent::Shuffle { before, after } => json!({"before": before, "after": after}),
            TraceEvent::Choice { pool, selected, .. } => json!({
                "pool": pool.iter().map(|&id| piece_value(cat, id)).collect::<Vec<_>>(), "selected": piece_value(cat, *selected),
            }),
            TraceEvent::PoolOrders { index, combo, orders } => json!({"index": index, "combo": combo, "orders": orders}),
            TraceEvent::Take { base, limit, before, after, groups, total } => json!({
                "base": base, "limit": limit, "before": before, "after": after, "groups": groups, "total": total,
            }),
            TraceEvent::Seek { before, after } => json!({"before": before, "after": after}),
            TraceEvent::Legality { valid } => json!({"valid": valid}),
            TraceEvent::FastDtRejection { combo, items, achieved, thresholds } => json!({
                "combo": combo, "items": items, "achieved": nums(achieved), "thresholds": nums(thresholds),
            }),
            TraceEvent::FullPlayerDt { combo, items, achieved, thresholds, player, .. } => json!({
                "combo": combo, "items": items, "achieved": nums(achieved), "thresholds": nums(thresholds),
                "player": player_value(cat, player),
            }),
            TraceEvent::Metric { player, evaluation, .. } => json!({
                "player": player_value(cat, player), "evaluation": evaluation_value(evaluation),
            }),
            TraceEvent::Rescore { ids, evaluation } => json!({"ids": ids, "evaluation": evaluation_value(evaluation)}),
            TraceEvent::ScoreChunk { base, groups, thresholds, metrics } => json!({
                "base": base, "groups": groups, "thresholds": nums(thresholds), "metrics": nums(metrics),
            }),
            TraceEvent::MapperSelection { workers, candidate_estimate, parallel, width, floor, ceiling } => json!({
                "workers": workers, "candidate_estimate": candidate_estimate, "parallel": parallel,
                "width": width, "floor": floor, "ceiling": ceiling,
            }),
            TraceEvent::MapperScores { base, groups, total, thresholds, metrics, parallel } => json!({
                "base": base, "groups": groups, "total": total, "thresholds": nums(thresholds),
                "metrics": nums(metrics), "parallel": parallel,
            }),
            TraceEvent::PassStart { ids, metric, thresholds } => json!({"ids": ids, "metric": num(*metric), "thresholds": nums(thresholds)}),
            TraceEvent::Accepted { before, before_metric, combo, items, metric, batch, scanned, total, rewind } => json!({
                "before": before, "before_metric": num(*before_metric), "combo": combo, "items": items,
                "metric": num(*metric), "batch": batch, "scanned": scanned, "total": total, "rewind": rewind,
            }),
            TraceEvent::PassReturn { ids, metric, output, cursor, similar } => json!({
                "ids": ids, "metric": num(*metric), "output": nums(output), "cursor": cursor, "similar": similar_value(cat, similar),
            }),
            TraceEvent::DtStage { thresholds, prior_achieved, ids, unchanged } => json!({
                "thresholds": nums(thresholds), "prior_achieved": nums(prior_achieved), "ids": ids, "unchanged": unchanged,
            }),
            TraceEvent::DtStageEnd { achieved, next_thresholds, unchanged, metric } => json!({
                "achieved": nums(achieved), "next_thresholds": nums(next_thresholds), "unchanged": unchanged, "metric": num(*metric),
            }),
            TraceEvent::UnsatisfiedDt { achieved, unchanged } => json!({"achieved": nums(achieved), "unchanged": unchanged}),
            TraceEvent::RestartComparison { restart, metric, incumbent, improved } => json!({
                "restart": restart, "metric": num(*metric), "incumbent": num(*incumbent), "improved": improved,
            }),
            TraceEvent::BuildReturn { ids, metric, output, invert, achieved_before_reorder, unchanged, starting, filtered, player, .. } => json!({
                "ids": ids, "metric": num(*metric), "output": nums(output), "invert": invert,
                "achieved_before_reorder": nums(achieved_before_reorder), "unchanged": unchanged,
                "starting": gearset_value(cat, starting), "filtered": pools_value(cat, filtered), "player": player_value(cat, player),
            }),
        };
        value.as_object_mut().unwrap().insert("kind".to_owned(), json!(crate::optimizer_support::event_kind(event)));
        value
    }).collect())
}

pub const NATIVE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/native");

pub fn expect_native(id: &str, legacy_sha256: &str, actual: &Value) {
    static INDEX: LazyLock<Value> = LazyLock::new(|| {
        wsdist_fixtures::read_value(&Path::new(NATIVE_ROOT).join("index.json"), None).expect("native golden index")
    });
    let entry = INDEX["cases"].as_array().unwrap().iter().find(|entry| entry["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("no native golden for {id}; regenerate with examples/regenerate_native_goldens"));
    assert_eq!(entry["legacy_sha256"].as_str().unwrap(), legacy_sha256, "{id}: native golden legacy provenance");
    let doc = wsdist_fixtures::read_value(&Path::new(NATIVE_ROOT).join(entry["path"].as_str().unwrap()),
        Some(entry["content_xxh3_128"].as_str().unwrap())).unwrap();
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
