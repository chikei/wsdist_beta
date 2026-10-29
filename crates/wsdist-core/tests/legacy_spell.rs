#[allow(dead_code)]
mod combat_support;

use std::collections::BTreeSet;
use std::sync::Arc;

use wsdist_core::Catalog;
use wsdist_core::actions::{evaluate_spell, gui_spell_type, optimizer_spell_type, resolve_spell_name};
use wsdist_core::model::{CalcError, EvaluationContext, Metric, SpellRequest, SpellType};
use wsdist_core::rng::ScriptedTape;
use wsdist_core::stats::Stat;

use combat_support::{build_boundary, boundary_enemy, document, fixture_root, index, replay_case, resource_root};

#[test]
fn every_frozen_spell_replays_actual_player_construction_and_recorded_draws() {
    let root = fixture_root();
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("frozen catalog"));
    let fixtures = index(&root);
    let mut cases = 0;
    let mut recorded = 0;
    let mut scripted = 0;
    for entry in fixtures.cases.iter().filter(|entry| entry.operation == "combat.spell") {
        let case = document(&root.join(&entry.path));
        let counts = replay_case(&case, &catalog);
        let rejected_build = case["expected"]["value"]["status"] == "legacy_error";
        assert_eq!(counts.recorded, usize::from(!rejected_build), "{}: capture stage coverage", entry.id);
        assert_eq!(counts.hits, 0, "{}: analytical casts must not invent sampled hit events", entry.id);
        cases += 1;
        recorded += counts.recorded;
        scripted += counts.scripted;
    }
    assert_eq!(cases, 9565, "complete immutable combat.spell corpus");
    eprintln!("spell parity: {cases} frozen cases, {recorded} recorded compiled-draw replays, {scripted} strict-tape executions");
}

#[test]
fn actual_gui_names_keep_the_two_captured_dispatch_mappings() {
    let root = fixture_root();
    let fixtures = index(&root);
    let entry = fixtures.cases.iter().find(|entry| entry.operation == "combat.metadata").expect("reference tables");
    let case = document(&root.join(&entry.path));
    let mut seen = BTreeSet::new();
    for row in case["input"]["spell_dispatch"].as_array().expect("source dispatch rows") {
        let name = row["name"].as_str().unwrap();
        if !seen.insert(name) { continue; }
        // The name must have a typed request identity even when a particular
        // source dispatch ends in a captured unsupported-operation exception.
        resolve_spell_name(name).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(gui_spell_type(name).unwrap().name(), row["gui_type"].as_str().unwrap(), "{name}: GUI dispatch");
        assert_eq!(optimizer_spell_type(name).name(), row["optimizer_type"].as_str().unwrap(), "{name}: optimizer dispatch");
    }
}

#[test]
fn invalid_resist_rank_is_rejected_before_enspell_delay_access_without_rng_draws() {
    let root = fixture_root();
    let fixtures = index(&root);
    let entry = fixtures.cases.iter().find(|entry| entry.id.ends_with(".spell.EnSpell.weatherFirestorm_II")).unwrap();
    let case = document(&root.join(&entry.path));
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("frozen catalog"));
    let mut ctx = EvaluationContext::new(catalog, ScriptedTape::new(Vec::new()));
    let mut player = build_boundary(&case, &mut ctx).expect("actual player construction");
    ctx.random.assert_finished().expect("player build must not consume RNG");
    player.abilities.set_resist_rank(Arc::from("unsupported rank"));
    player.stats.remove(Stat::Delay1);
    let request = SpellRequest { spell: resolve_spell_name("EnSpell").unwrap(),
        spell_type: SpellType::ElementalMagic, metric: Metric::new("TP return") };
    let error = evaluate_spell(&player, &boundary_enemy(&case), &request, &mut ctx).unwrap_err();
    match error {
        CalcError::Legacy { operation, field, category, .. } => {
            assert_eq!(operation, "cast_spell");
            assert_eq!(field, "Enemy Resist Rank");
            assert_eq!(category, "ValueError");
        }
        error => panic!("expected a source rank-lookup exception, got {error}"),
    }
    ctx.random.assert_finished().expect("a failed cast must not consume RNG");
}

#[test]
fn unsupported_spells_do_not_receive_invented_coefficients() {
    for name in ["Firaga III", "Geohelix", "unknown spell"] {
        match resolve_spell_name(name) {
            Err(CalcError::Input { operation, field, .. }) => {
                assert_eq!(operation, "cast_spell");
                assert_eq!(field, "spell_name");
            }
            result => panic!("{name}: unsupported name unexpectedly resolved: {result:?}"),
        }
    }
}
