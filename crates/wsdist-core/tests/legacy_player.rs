use std::collections::HashSet;
use std::sync::Arc;

use wsdist_core::model::{EvaluationContext, Job, Player, build_player, get_skill_accuracy};
use wsdist_core::Catalog;

#[path = "legacy_player/support.rs"]
mod support;

use support::*;

#[test]
fn every_frozen_player_build_preserves_sparse_stages_and_legacy_errors() {
    let fixtures = core_fixtures();
    let index = document(&fixtures.join("players.json"));
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let immutable_skills: Vec<_> = catalog.pieces().iter().map(|piece| piece.skill_type).collect();
    let mut ctx = EvaluationContext::new(catalog.clone(), RejectRandom);
    ctx.player_stages = Some(Vec::with_capacity(5));
    let mut player = Player::new([catalog.empty; 16]);
    let mut count = 0;
    let mut errors = 0;
    let mut jobs = HashSet::new();
    let mut job_ml_sub = HashSet::new();
    let mut catalog_occurrences = HashSet::new();
    let mut foods = HashSet::new();
    for entry in index["cases"].as_array().unwrap().iter().filter(|entry| entry["operation"] == "player_build") {
        let case = fixture(&fixtures, entry);
        let name = case["id"].as_str().unwrap();
        // Every single-build oracle case starts with an independently imported
        // catalog. Reusing storage must not carry sparse keys or H2H overrides.
        ctx.state.reset(catalog.pieces().len());
        let before = effective_skills(&catalog, &ctx.state);
        let input = parse_input(&case["input"], &catalog, &ctx.state, name);
        let result = build_player(&input, &mut ctx, &mut player);
        compare_build(&result, &player, &ctx, &before, &case["expected"]["value"], name);
        count += 1;
        errors += usize::from(result.is_err());
        jobs.insert(input.main_job);
        if name.starts_with("player.jobs.") {
            job_ml_sub.insert((input.main_job, input.master_level, input.sub_job));
        }
        if let Some(id) = case.get("catalog_piece_id") {
            catalog_occurrences.insert((id.as_u64().unwrap(), case["valid_slot"].as_str().unwrap().to_owned()));
        }
        if let Some(food) = case["input"].get("food_piece") { foods.insert(food["id"].as_u64().unwrap()); }
    }
    assert_eq!(count, 5943, "complete frozen player_build family");
    assert_eq!(errors, 9, "captured Fencer/Smite KeyErrors remain errors");
    assert_eq!(jobs.len(), 22, "all main jobs");
    for &job in Job::ALL {
        for ml in [0, 4, 5, 30, 50] {
            for sub in Job::ALL.iter().copied().map(Some).chain([None]) {
                if sub != Some(job) { assert!(job_ml_sub.contains(&(job, ml, sub)), "missing {job:?}/{sub:?}/ML{ml}"); }
            }
        }
        for ml in [-1, 51] { assert!(job_ml_sub.contains(&(job, ml, None)), "missing {job:?}/ML clamp {ml}"); }
    }
    for piece in catalog.pieces() {
        if catalog.foods().contains(&piece.id) {
            assert!(foods.contains(&u64::from(piece.original_id)), "missing food {}", piece.name2);
        } else {
            assert!(catalog_occurrences.iter().any(|(id, _)| *id == u64::from(piece.original_id)), "missing declared/listed/generated piece {}", piece.name2);
        }
    }
    for slot in wsdist_core::Slot::ALL {
        for &id in catalog.slot(*slot) {
            assert!(catalog_occurrences.contains(&(u64::from(catalog.piece(id).original_id), slot.name().to_owned())), "missing listed occurrence {}/{}", slot.name(), catalog.piece(id).name2);
        }
    }
    assert_eq!(catalog.pieces().iter().map(|piece| piece.skill_type).collect::<Vec<_>>(), immutable_skills, "Arc catalog is immutable");
}

#[test]
fn all_frozen_h2h_sequences_preserve_original_piece_identity_and_isolation() {
    let fixtures = core_fixtures();
    let index = document(&fixtures.join("players.json"));
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let immutable_skills: Vec<_> = catalog.pieces().iter().map(|piece| piece.skill_type).collect();
    let mut count = 0;
    for entry in index["cases"].as_array().unwrap().iter().filter(|entry| entry["operation"] == "player_sequence") {
        let case = fixture(&fixtures, entry);
        let name = case["id"].as_str().unwrap();
        assert_eq!(case["input"]["state_policy"], "shared_catalog_identity");
        let mut ctx = EvaluationContext::new(catalog.clone(), RejectRandom);
        ctx.player_stages = Some(Vec::with_capacity(5));
        let mut player = Player::new([catalog.empty; 16]);
        let expected = case["expected"]["value"]["players"].as_array().unwrap();
        let inputs = case["input"]["players"].as_array().unwrap();
        assert_eq!(inputs.len(), expected.len());
        for (index, (input, expected)) in inputs.iter().zip(expected).enumerate() {
            let path = format!("{name}.players[{index}]");
            let before = effective_skills(&catalog, &ctx.state);
            let input = parse_input(input, &catalog, &ctx.state, &path);
            let result = build_player(&input, &mut ctx, &mut player);
            compare_build(&result, &player, &ctx, &before, expected, &path);
        }
        // Reading the same Arc from another context must still see the frozen
        // metadata, including Empty shared by ranged/ammo/armor slots.
        let isolated = EvaluationContext::new(catalog.clone(), RejectRandom);
        assert_eq!(effective_skills(&catalog, &isolated.state), immutable_skills, "{name}: context isolation");
        assert_eq!(catalog.pieces().iter().map(|piece| piece.skill_type).collect::<Vec<_>>(), immutable_skills, "{name}: Arc catalog immutability");
        count += 1;
    }
    assert_eq!(count, 3, "complete frozen player_sequence family");
}

#[test]
fn frozen_skill_accuracy_windows_keep_each_integer_stage() {
    let fixtures = core_fixtures();
    let index = document(&fixtures.join("players.json"));
    let entry = index["cases"].as_array().unwrap().iter().find(|entry|
        entry["id"] == "kernel.create_player.get_skill_accuracy.skill_windows").unwrap();
    let case = fixture(&fixtures, entry);
    let inputs = case["input"]["rows"].as_array().unwrap();
    let expected = case["expected"]["value"]["rows"].as_array().unwrap();
    assert_eq!(inputs.len(), expected.len());
    for (input, expected) in inputs.iter().zip(expected) {
        let path = format!("{}.{}", case["id"].as_str().unwrap(), input["label"].as_str().unwrap());
        assert_eq!(input["label"], expected["label"], "{path}: row order");
        let actual = get_skill_accuracy(number(&input["args"][0]));
        match expected["outcome"].as_str().unwrap() {
            "success" => assert_number(actual.unwrap_or_else(|error| panic!("{path}: {error}")), &expected["result"], &path),
            "error" => match actual {
                Err(wsdist_core::CalcError::Legacy { category, message, .. }) => {
                    assert_eq!(category, expected["exception"]["class"].as_str().unwrap(), "{path}: error category");
                    assert_eq!(message, expected["exception"]["message"].as_str().unwrap(), "{path}: error message");
                }
                result => panic!("{path}: expected captured error, got {result:?}"),
            },
            other => panic!("{path}: unknown frozen outcome {other}"),
        }
    }
}

#[test]
fn all_23_profiles_build_their_actual_captured_effective_inputs_without_qt() {
    let fixtures = app_fixtures();
    let index = document(&fixtures.join("ui.json"));
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let immutable_skills: Vec<_> = catalog.pieces().iter().map(|piece| piece.skill_type).collect();
    let mut profiles = 0;
    let mut builds = 0;
    for entry in index["cases"].as_array().unwrap().iter().filter(|entry| entry["operation"] == "ui.profile") {
        let case = fixture(&fixtures, entry);
        let name = case["id"].as_str().unwrap();
        let mut ctx = EvaluationContext::new(catalog.clone(), RejectRandom);
        ctx.player_stages = Some(Vec::with_capacity(5));
        let mut player = Player::new([catalog.empty; 16]);
        for expected in case["expected"]["value"]["player_builds"].as_array().unwrap() {
            let path = format!("{name}.player_builds[{}].{}", expected["index"], expected["phase"].as_str().unwrap());
            let input = parse_input(&expected["input"], &catalog, &ctx.state, &path);
            let result = build_player(&input, &mut ctx, &mut player);
            assert_error(&result, expected.get("error"), &path);
            compare_profile_build(&player, &ctx, expected, &path);
            builds += 1;
        }
        profiles += 1;
    }
    assert_eq!(profiles, 23, "all shipped profile objects, including default");
    assert_eq!(builds, 577, "every actual GUI/profile construction, with its cached effective inputs and shared state");
    assert_eq!(catalog.pieces().iter().map(|piece| piece.skill_type).collect::<Vec<_>>(), immutable_skills, "Arc catalog is immutable");
}

#[test]
fn h2h_changes_full_value_equality_only_for_the_original_sub_identity() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::create_dir(temporary.path().join("data")).unwrap();
    for name in ["data/gear.json", "data/enemies.json", "data/buffs.json", "data/pup-attachments.json", "item_list.csv"] {
        std::fs::copy(resource_root().join(name), temporary.path().join(name)).unwrap();
    }
    let path = temporary.path().join("data/gear.json");
    let mut gear = document(&path);
    let empty_id = gear["symbols"]["Empty"].as_u64().unwrap();
    let new_id = gear["pieces"].as_array().unwrap().iter().map(|piece| piece["id"].as_u64().unwrap()).max().unwrap() + 1;
    let mut distinct_empty = gear["pieces"].as_array().unwrap().iter().find(|piece| piece["id"].as_u64().unwrap() == empty_id).unwrap().clone();
    distinct_empty["id"] = serde_json::json!(new_id);
    gear["pieces"].as_array_mut().unwrap().push(distinct_empty);
    gear["symbols"]["DistinctEmptyIdentity"] = serde_json::json!(new_id);
    std::fs::write(&path, serde_json::to_vec(&gear).unwrap()).unwrap();
    let catalog = Arc::new(Catalog::load(temporary.path()).unwrap());
    let duplicate = catalog.by_symbol("DistinctEmptyIdentity").unwrap();
    let mut ctx = EvaluationContext::new(catalog.clone(), RejectRandom);
    let original = catalog.piece(catalog.empty);
    let separate = catalog.piece(duplicate);
    assert!(original.value_equal(separate, &ctx.state));
    let sequence = document(&core_fixtures().join("players/005942.json"));
    let input = parse_input(&sequence["input"]["players"][0], &catalog, &ctx.state, "identity-equality.h2h");
    let mut player = Player::new([catalog.empty; 16]);
    build_player(&input, &mut ctx, &mut player).unwrap();
    assert_eq!(ctx.state.skill(original), wsdist_core::SkillType::HandToHand);
    assert_eq!(ctx.state.skill(separate), wsdist_core::SkillType::None);
    assert!(!original.value_equal(separate, &ctx.state), "the H2H assignment changes full-value equality through the original identity");
    let isolated = EvaluationContext::new(catalog.clone(), RejectRandom);
    assert!(original.value_equal(separate, &isolated.state), "a separate context still resolves the two full records equally");
    assert_eq!(original.skill_type, wsdist_core::SkillType::None);
    assert_eq!(separate.skill_type, wsdist_core::SkillType::None);
}

#[test]
fn mythic_aftermath_keeps_integer_float_and_bool_index_semantics_through_json() {
    let fixtures = core_fixtures();
    let index = document(&fixtures.join("players.json"));
    let catalog = Arc::new(Catalog::load(&resource_root()).unwrap());
    let entry = |name| index["cases"].as_array().unwrap().iter().find(|entry| entry["id"] == name).unwrap();
    let tier1 = fixture(&fixtures, entry("player.aftermath.main.120.1"));
    let tier3 = fixture(&fixtures, entry("player.aftermath.main.120.3"));
    let tuple_error = serde_json::json!({
        "class": "TypeError",
        "message": "tuple indices must be integers or slices, not float"
    });
    let tier1_gear_accuracy = tier1["expected"]["value"]["events"].as_array().unwrap().iter()
        .find(|event| event.get("stage").is_some_and(|stage| stage == "after_gear")).unwrap()["raw"]["stats"]
        .as_array().unwrap().iter().find(|row| row[0] == "Accuracy").unwrap()[1].clone();
    let mut ctx = EvaluationContext::new(catalog.clone(), RejectRandom);
    ctx.player_stages = Some(Vec::with_capacity(5));
    let mut player = Player::new([catalog.empty; 16]);
    for (label, reference, literal, successful) in [
        ("integer.1", &tier1, serde_json::json!(1), true),
        ("float.1", &tier1, serde_json::json!(1.0), false),
        ("bool.true", &tier1, serde_json::json!(true), true),
        // AM3 does not index the tuple: the source's equality branch accepts 3.0.
        ("float.3", &tier3, serde_json::json!(3.0), true),
    ] {
        let mut record = reference["input"].clone();
        record["abilities"]["Aftermath"] = literal;
        let mut input = parse_input(&record, &catalog, &ctx.state, label);
        for roundtrip in [false, true] {
            if roundtrip {
                input.abilities = serde_json::from_str(&serde_json::to_string(&input.abilities).unwrap()).unwrap();
            }
            ctx.state.reset(catalog.pieces().len());
            let before = effective_skills(&catalog, &ctx.state);
            let path = format!("mythic-kind.{label}.json_roundtrip={roundtrip}");
            let result = build_player(&input, &mut ctx, &mut player);
            if successful {
                compare_build(&result, &player, &ctx, &before, &reference["expected"]["value"], &path);
            } else {
                assert_error(&result, Some(&tuple_error), &path);
                assert_number(player.stats.get(wsdist_core::Stat::Accuracy), &tier1_gear_accuracy, &format!("{path}.Accuracy"));
                let stages = ctx.player_stages.as_ref().unwrap();
                assert_eq!(stages.iter().map(|stage| stage.stage).collect::<Vec<_>>(), [
                    wsdist_core::model::PlayerStage::Base,
                    wsdist_core::model::PlayerStage::Gear,
                    wsdist_core::model::PlayerStage::Buffs,
                ], "{path}: the failed tuple access must not produce a finalized player");
            }
        }
    }
}
