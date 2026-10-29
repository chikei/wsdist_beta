use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use wsdist_core::model::{
    Abilities, ActiveEffects, CalcError, EffectBucket, EvaluationContext, GearPiece, Job,
    LegacyF64, ModelState, Player, PlayerInput, PlayerStage, RandomSource, RandomStream,
    SkillType, Slot, WscContribution,
};
use wsdist_core::{Catalog, Stat, Stats};

#[path = "../support/fixture_json.rs"]
mod fixture_json;

#[path = "../support/observation.rs"]
pub mod observation;

/// Player construction has no random operations. Any accidental future draw is
/// a protocol failure rather than a made-up variate that could hide a mismatch.
#[derive(Clone)]
pub struct RejectRandom;

impl RandomSource for RejectRandom {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        panic!("unrequested uniform: {stream:?}, {low}, {high}");
    }
    fn bounded(&mut self, upper_exclusive: usize) -> usize {
        panic!("unrequested bounded integer: {upper_exclusive}");
    }
    fn reseed(&mut self, seed: u64) {
        panic!("unrequested reseed: {seed}");
    }
}

pub fn resource_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn core_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

pub fn app_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../wsdist-app/tests/fixtures/legacy")
}

pub fn document(path: &Path) -> Value {
    fixture_json::read::<Value>(path, None).unwrap_or_else(|error| panic!("{error}"))
}

pub fn fixture(root: &Path, entry: &Value) -> Value {
    let path = root.join(entry["path"].as_str().unwrap());
    fixture_json::read::<Value>(&path, Some(entry["sha256"].as_str().unwrap()))
        .unwrap_or_else(|error| panic!("{error}"))
}

pub fn number(value: &Value) -> f64 {
    serde_json::from_value::<LegacyF64>(value.clone()).unwrap().0
}

pub fn assert_number(actual: f64, expected: &Value, path: &str) {
    let target = number(expected);
    observation::record(actual, target, expected.as_i64().is_some() || expected.as_u64().is_some(), path);
    if target.is_nan() {
        assert!(actual.is_nan(), "{path}: expected NaN, got {actual:?}");
    } else if !target.is_finite() || target == 0.0 {
        assert_eq!(actual.to_bits(), target.to_bits(), "{path}: nonfinite class or signed zero");
    } else if expected.as_i64().is_some() || expected.as_u64().is_some() {
        assert_eq!(actual, target, "{path}: integer stage");
    } else {
        let tolerance = 1e-9 + 1e-12 * actual.abs().max(target.abs());
        assert!(actual.is_finite() && (actual - target).abs() <= tolerance, "{path}: expected {target:.17}, got {actual:.17}, error {}", (actual - target).abs());
    }
}

pub fn assert_wsc(actual: &[WscContribution], expected: &Value, path: &str) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{path}: WSC presence/count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual.stat.name(), expected[0].as_str().unwrap(), "{path}[{index}]: WSC stat/order");
        assert_number(actual.percent, &expected[1], &format!("{path}[{index}].percent"));
    }
}

pub fn assert_sheet(stats: &Stats, wsc: &[WscContribution], wyvern: bool, expected: &Value, path: &str) {
    let source = &expected["stats"];
    let rows: Vec<(&str, &Value)> = if let Some(rows) = source.as_array() {
        rows.iter().map(|row| (row[0].as_str().unwrap(), &row[1])).collect()
    } else if let Some(order) = expected.get("stat_order") {
        order.as_array().unwrap().iter().map(|key| {
            let key = key.as_str().unwrap();
            (key, &source[key])
        }).collect()
    } else {
        source.as_object().unwrap().iter().map(|(key, value)| (key.as_str(), value)).collect()
    };
    assert_eq!(stats.len(), rows.len(), "{path}: sparse presence/count");
    for (index, ((stat, value), (name, expected))) in stats.ordered().zip(rows).enumerate() {
        assert_eq!(stat.name(), name, "{path}.stats[{index}]: first differing insertion-order/presence key");
        assert_number(value, expected, &format!("{path}.stats[{index}:{name}]"));
    }
    assert_wsc(wsc, &expected["wsc"], &format!("{path}.wsc"));
    assert_eq!(wyvern, expected["wyvern_bonus_attack"].as_bool().unwrap(), "{path}.wyvern_bonus_attack");
}

pub fn assert_metadata(player: &Player, expected: &Value, path: &str) {
    assert_eq!(player.main_job.name(), expected["main_job"].as_str().unwrap(), "{path}.main_job");
    assert_eq!(player.sub_job.map_or("none", Job::name), expected["sub_job"].as_str().unwrap(), "{path}.sub_job");
    assert_eq!(i64::from(player.master_level), expected["master_level"].as_i64().unwrap(), "{path}.master_level");
    assert_eq!(i64::from(player.main_job_level), expected["main_job_level"].as_i64().unwrap(), "{path}.main_job_level");
    assert_eq!(i64::from(player.sub_job_level), expected["sub_job_level"].as_i64().unwrap(), "{path}.sub_job_level");
}

pub fn assert_piece(piece: &GearPiece, state: Option<&ModelState>, expected: &Value, path: &str) {
    assert_eq!(u64::from(piece.original_id), expected["id"].as_u64().unwrap(), "{path}.id");
    assert_eq!(piece.name.as_ref(), expected["name"].as_str().unwrap(), "{path}.name");
    assert_eq!(piece.name2.as_ref(), expected["name2"].as_str().unwrap(), "{path}.name2");
    assert_eq!(piece.jobs.iter().map(|job| job.name()).collect::<Vec<_>>(), expected["jobs"].as_array().unwrap().iter().map(|job| job.as_str().unwrap()).collect::<Vec<_>>(), "{path}.jobs/order");
    assert_eq!(piece.kind.name(), expected["type"].as_str().unwrap(), "{path}.type");
    let skill = state.map_or(piece.skill_type, |state| state.skill(piece));
    assert_eq!(skill.name(), expected["skill_type"].as_str().unwrap(), "{path}.skill_type");
    assert_number(piece.dmg, &expected["dmg"], &format!("{path}.dmg"));
    assert_number(piece.delay, &expected["delay"], &format!("{path}.delay"));
    assert_eq!(piece.rank, expected["rank"].as_i64(), "{path}.rank");
    match (piece.wsc, expected["wsc"].as_array()) {
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.stat.name(), expected[0].as_str().unwrap(), "{path}.wsc.stat");
            assert_number(actual.percent, &expected[1], &format!("{path}.wsc.percent"));
        }
        (None, None) => assert!(expected["wsc"].is_null(), "{path}.wsc absence"),
        _ => panic!("{path}.wsc presence"),
    }
    let expected_stats = expected["stats"].as_object().unwrap();
    assert_eq!(piece.stats.len(), expected_stats.len(), "{path}.stats presence/count");
    for (index, (actual, (name, expected))) in piece.stats.iter().zip(expected_stats).enumerate() {
        assert_eq!(actual.stat.name(), name, "{path}.stats[{index}]: order");
        assert_number(actual.value, expected, &format!("{path}.stats[{name}]"));
        assert_eq!(json!(actual.literal), *expected, "{path}.stats[{name}]: original numeric literal");
    }
}

pub fn parse_input(value: &Value, catalog: &Catalog, state: &ModelState, path: &str) -> PlayerInput {
    let main_job = Job::parse_player_name(value["main_job"].as_str().unwrap()).unwrap();
    let sub_name = value["sub_job"].as_str().unwrap();
    let sub_job = if sub_name.eq_ignore_ascii_case("none") { None } else { Some(Job::parse_player_name(sub_name).unwrap()) };
    let source_gear = value["gearset"].as_object().unwrap();
    assert_eq!(source_gear.keys().map(String::as_str).collect::<Vec<_>>(), Slot::ALL.iter().map(|slot| slot.name()).collect::<Vec<_>>(), "{path}.gearset: ordered slot contract");
    let gearset = std::array::from_fn(|index| {
        let slot = Slot::ALL[index];
        let record = &source_gear[slot.name()];
        let original = record.get("original").unwrap_or(record);
        let id = catalog.by_original_id(original["id"].as_u64().unwrap() as u32).unwrap();
        assert_piece(catalog.piece(id), None, original, &format!("{path}.gearset.{}.original", slot.name()));
        if let Some(effective) = record.get("effective") {
            assert_piece(catalog.piece(id), Some(state), effective, &format!("{path}.gearset.{}.effective", slot.name()));
        }
        id
    });
    let mut effects = ActiveEffects::default();
    for (name, values) in value["buffs"].as_object().unwrap() {
        let mut stats = Stats::default();
        for (name, value) in values.as_object().unwrap() {
            // Metadata values can be strings in the frozen ordered-source case.
            if matches!(name.as_str(), "Name" | "Name2" | "Type" | "DMG" | "Delay" | "Jobs" | "Skill Type" | "Rank") { continue; }
            stats.set(Stat::from_name(name).unwrap_or_else(|| panic!("{path}.buffs: unknown {name}")), number(value));
        }
        effects.buckets.push(EffectBucket { name: Arc::from(name.as_str()), stats });
    }
    PlayerInput {
        main_job, sub_job, master_level: value["master_level"].as_i64().unwrap().try_into().unwrap(),
        gearset, effects: Arc::new(effects),
        abilities: Abilities::from_legacy(value["abilities"].as_object().unwrap()).unwrap(),
    }
}

pub fn effective_skills(catalog: &Catalog, state: &ModelState) -> Vec<SkillType> {
    catalog.pieces().iter().map(|piece| state.skill(piece)).collect()
}

pub fn assert_error(result: &Result<(), CalcError>, expected: Option<&Value>, path: &str) {
    match (result, expected) {
        (Ok(()), None) => {}
        (Err(CalcError::Legacy { operation, field, category, message }), Some(expected)) => {
            assert_eq!(*operation, "build_player", "{path}: operation");
            assert!(!field.is_empty(), "{path}: error field");
            assert_eq!(*category, expected["class"].as_str().unwrap(), "{path}: error category");
            assert_eq!(message, expected["message"].as_str().unwrap(), "{path}: error message");
        }
        (actual, target) => panic!("{path}: outcome differs, actual {actual:?}, expected {target:?}"),
    }
}

pub fn compare_build(
    result: &Result<(), CalcError>, player: &Player, ctx: &EvaluationContext<RejectRandom>,
    before_skills: &[SkillType], expected: &Value, path: &str,
) {
    assert_error(result, expected.get("exception"), path);
    assert_eq!(result.is_ok(), expected["outcome"] == "success", "{path}: outcome");
    let events: Vec<_> = expected["events"].as_array().unwrap().iter().filter(|event| event.get("stage").is_some()).collect();
    let stages = ctx.player_stages.as_ref().unwrap();
    assert_eq!(stages.len(), events.len() + usize::from(result.is_ok()), "{path}: completed/exceptional outer stages");
    let initial = &expected["resolved_input"]["gearset"];
    let mut skills: [SkillType; Slot::COUNT] = std::array::from_fn(|index| {
        SkillType::from_name(initial[Slot::ALL[index].name()]["skill_type"].as_str().unwrap()).unwrap()
    });
    for (index, (actual, event)) in stages.iter().zip(&events).enumerate() {
        let stage = event["stage"].as_str().unwrap();
        assert_eq!(actual.stage.name(), stage, "{path}.stages[{index}]: stage order");
        for mutation in event["gear_mutations"].as_array().unwrap() {
            let id = mutation["id"].as_u64().unwrap();
            let after = SkillType::from_name(mutation["after"]["skill_type"].as_str().unwrap()).unwrap();
            for slot in Slot::ALL {
                if initial[slot.name()]["id"].as_u64().unwrap() == id { skills[slot.index()] = after; }
            }
        }
        assert_eq!(actual.skills, skills, "{path}.{stage}: effective skills/original-piece identity");
        assert_sheet(&actual.stats, &actual.wsc, actual.wyvern_bonus_attack, &event["raw"], &format!("{path}.{stage}"));
        assert_metadata(player, &event["raw"], &format!("{path}.{stage}"));
    }
    assert_sheet(&player.stats, &player.wsc, player.wyvern_bonus_attack, &expected["final"], &format!("{path}.final"));
    assert_metadata(player, &expected["final"], &format!("{path}.final"));
    if result.is_ok() {
        let final_stage = stages.last().unwrap();
        assert_eq!(final_stage.stage, PlayerStage::Final, "{path}: final sorted stage");
        assert_eq!(final_stage.skills, skills, "{path}.final: effective skills");
        assert_sheet(&final_stage.stats, &final_stage.wsc, final_stage.wyvern_bonus_attack, &expected["final"], &format!("{path}.final_snapshot"));
    }
    for slot in Slot::ALL {
        assert_piece(ctx.catalog.piece(player.gearset[slot.index()]), Some(&ctx.state), &expected["effective_gearset"][slot.name()], &format!("{path}.effective_gearset.{}", slot.name()));
    }
    let changed: Vec<_> = ctx.catalog.pieces().iter().filter(|piece| before_skills[piece.id.index()] != ctx.state.skill(piece)).collect();
    let expected_changes = expected["catalog_skill_type_changes"].as_array().unwrap();
    assert_eq!(changed.len(), expected_changes.len(), "{path}: catalog skill changes");
    for (piece, expected) in changed.iter().zip(expected_changes) {
        assert_eq!(u64::from(piece.original_id), expected["id"].as_u64().unwrap(), "{path}: changed identity/order");
        assert_eq!(before_skills[piece.id.index()].name(), expected["before"].as_str().unwrap(), "{path}: changed skill before");
        assert_eq!(ctx.state.skill(piece).name(), expected["after"].as_str().unwrap(), "{path}: changed skill after");
        assert_piece(piece, Some(&ctx.state), &expected["after_piece"], &format!("{path}.changed_piece.{}", piece.original_id));
        let sharing: Vec<_> = Slot::ALL.iter().filter(|slot| player.gearset[slot.index()] == piece.id).map(|slot| slot.name()).collect();
        assert_eq!(sharing, expected["equipped_slots_sharing_identity"].as_array().unwrap().iter().map(|slot| slot.as_str().unwrap()).collect::<Vec<_>>(), "{path}: shared slots");
    }
}

pub fn compare_profile_build(player: &Player, ctx: &EvaluationContext<RejectRandom>, expected: &Value, path: &str) {
    let stages = ctx.player_stages.as_ref().unwrap();
    assert_eq!(stages.len(), 5, "{path}: completed player stages");
    for (index, (label, stage)) in [
        ("after_add_base_stats", PlayerStage::Base),
        ("after_add_gear_stats", PlayerStage::Gear),
        ("after_add_buffs", PlayerStage::Buffs),
        ("after_finalize_stats", PlayerStage::FinalizeBeforeSort),
    ].into_iter().enumerate() {
        let target = expected["stages"].as_array().unwrap().iter().find(|snapshot| snapshot["stage"] == label).unwrap();
        let actual = &stages[index];
        assert_eq!(actual.stage, stage, "{path}: stage order");
        assert_sheet(&actual.stats, &actual.wsc, actual.wyvern_bonus_attack, target, &format!("{path}.{label}"));
        assert_metadata(player, target, &format!("{path}.{label}"));
        let skills: [SkillType; Slot::COUNT] = std::array::from_fn(|index| {
            SkillType::from_name(target["gearset"][Slot::ALL[index].name()]["effective"]["skill_type"].as_str().unwrap()).unwrap()
        });
        assert_eq!(actual.skills, skills, "{path}.{label}: effective skills");
    }
    let final_sheet = &expected["final"];
    assert_metadata(player, final_sheet, &format!("{path}.final"));
    assert_sheet(&player.stats, &player.wsc, player.wyvern_bonus_attack, final_sheet, &format!("{path}.final"));
    assert_eq!(stages[4].stage, PlayerStage::Final, "{path}: final sorted stage");
    assert_sheet(&stages[4].stats, &stages[4].wsc, stages[4].wyvern_bonus_attack, final_sheet, &format!("{path}.final_snapshot"));
    let skills: [SkillType; Slot::COUNT] = std::array::from_fn(|index| {
        SkillType::from_name(final_sheet["gearset"][Slot::ALL[index].name()]["effective"]["skill_type"].as_str().unwrap()).unwrap()
    });
    assert_eq!(stages[4].skills, skills, "{path}.final_snapshot: effective skills");
    for slot in Slot::ALL {
        assert_piece(ctx.catalog.piece(player.gearset[slot.index()]), Some(&ctx.state), &final_sheet["gearset"][slot.name()]["effective"], &format!("{path}.final.gearset.{}", slot.name()));
    }
}
