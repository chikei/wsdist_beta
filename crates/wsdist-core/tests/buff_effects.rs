mod buffs_support;

use std::collections::HashSet;
use std::sync::Arc;
use wsdist_core::buffs::{aggregate_buffs, build_enemy};
use wsdist_core::model::{ActiveEffects, BuffInputs, CalcError, EnemyInput};
use wsdist_core::{Catalog, Stat, Stats};
use buffs_support::{assert_stats, controls, enemy, fixture, read_json, root, states, ui_fixtures};

#[test]
fn frozen_effective_controls_reproduce_every_distinct_buff_and_enemy_state() {
    let catalog = Catalog::load(&root()).unwrap();
    let fixtures = ui_fixtures();
    let index = read_json(&fixtures.join("ui.json"));
    assert_eq!(index["schema_version"], 3, "UI fixture storage schema");
    let mut seen = HashSet::new();
    let mut compared = 0;
    for entry in index["cases"].as_array().unwrap() {
        let case = fixture(&fixtures, entry);
        let id = case["id"].as_str().unwrap();
        let mut snapshots = Vec::new();
        states(&case["expected"], &mut snapshots);
        for (ordinal, state) in snapshots.into_iter().enumerate() {
            let input = controls(state);
            let fingerprint = serde_json::to_string(&(&input,&state["enemy_input"],&state["aggregate_buffs"],&state["normalized_enemy"])).unwrap();
            if !seen.insert(fingerprint) { continue; }
            let at = format!("{id}.state[{ordinal}]");
            let actual = aggregate_buffs(&input,&catalog).unwrap_or_else(|error| panic!("{at}: {error}"));
            let expected = &state["aggregate_buffs"];
            assert_eq!(actual.buckets.iter().map(|bucket|bucket.name.as_ref()).collect::<Vec<_>>(),expected["active_buffs"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),"{at}: bucket order");
            for bucket in &actual.buckets { assert_stats(&bucket.stats,&expected["active_buffs"][bucket.name.as_ref()],&format!("{at}.{}",bucket.name)); }
            assert_stats(&actual.enemy_debuffs,&expected["combined_debuffs"],&format!("{at}.debuffs"));
            if state["enemy_input"].is_object() {
                let normalized = build_enemy(&enemy(state),&actual).unwrap_or_else(|error| panic!("{at}: {error}"));
                assert_stats(&normalized.stats,&state["normalized_enemy"],&format!("{at}.enemy"));
            } else {
                assert!(state.get("gather_player_inputs_error").is_some(), "{at}: absent enemy must be a recorded rejected UI input");
            }
            compared+=1;
        }
    }
    println!("Replayed {compared} distinct captured effective buff/enemy states");
}

#[test]
fn parsing_is_conditional_and_empty_rolls_and_bad_potency_keep_legacy_errors() {
    let catalog=Catalog::load(&root()).unwrap();
    let mut input=BuffInputs::default();
    input.song_bonus=Arc::from("Songs +not-an-integer");
    input.roll_bonus=Arc::from("Rolls +bad");
    input.bubble_bonus=Arc::from("Geomancy +bad");
    let effects=aggregate_buffs(&input,&catalog).unwrap();
    assert_eq!(effects.buckets[0].stats.ordered().collect::<Vec<_>>(),[(Stat::Attack,0.0),(Stat::RangedAttack,0.0)]);
    input.geo_enabled=true;
    assert_legacy(aggregate_buffs(&input,&catalog).unwrap_err(),"ValueError","invalid literal for int() with base 10: 'bad'");
    input.geo_enabled=false;
    input.cor_enabled=true;
    input.rolls[0].name=Arc::from("");
    assert_legacy(aggregate_buffs(&input,&catalog).unwrap_err(),"IndexError","list index out of range");
    input.cor_enabled=false;
    input.geo_enabled=true;
    input.bubble_bonus=Arc::from("Geomancy +0");
    input.bubbles[0]=Arc::from("Indi-Frailty");
    input.bubble_potency=Arc::from("12.5");
    assert_legacy(aggregate_buffs(&input,&catalog).unwrap_err(),"ValueError","invalid literal for int() with base 10: '12.5'");
}

#[test]
fn decimal_integer_text_keeps_unicode_underscores_exact_division_and_limits() {
    let catalog=Catalog::load(&root()).unwrap();
    let mut input=BuffInputs::default();
    input.geo_enabled=true;
    input.bubbles[0]=Arc::from("Indi-Frailty");
    input.bubble_bonus=Arc::from("Geomancy +٠");
    for text in ["١_٢٣", "+１２３", " 123 "] {
        input.bubble_potency=Arc::from(text);
        let effects=aggregate_buffs(&input,&catalog).unwrap();
        assert_eq!(effects.enemy_debuffs.get(Stat::Defense),0.148*1.23);
    }
    input.bubble_potency=Arc::from("-١٢");
    assert_eq!(aggregate_buffs(&input,&catalog).unwrap().enemy_debuffs.get(Stat::Defense),0.0);
    input.bubble_potency=Arc::from(format!("1{}","0".repeat(309)));
    assert_eq!(aggregate_buffs(&input,&catalog).unwrap().enemy_debuffs.get(Stat::Defense),0.148*1e307);
    input.bubble_potency=Arc::from("9".repeat(4301));
    assert_legacy(aggregate_buffs(&input,&catalog).unwrap_err(),"ValueError","Exceeds the limit (4300 digits) for integer string conversion: value has 4301 digits; use sys.set_int_max_str_digits() to increase the limit");
}

#[test]
fn enemy_preparation_retains_base_defense_floors_and_missing_key_errors() {
    let mut stats=Stats::default();
    stats.set(Stat::Defense,100.0);stats.set(Stat::MagicDefense,10.0);stats.set(Stat::MagicDtPct,-25.0);
    let input=EnemyInput{name:Arc::from("enemy"),location:Arc::from("zone"),stats};
    let mut effects=ActiveEffects::default();
    effects.enemy_debuffs.set(Stat::Defense,1.5);effects.enemy_debuffs.set(Stat::MagicDefense,100.0);
    let output=build_enemy(&input,&effects).unwrap();
    assert_eq!(output.stats.get(Stat::BaseDefense),100.0);
    assert_eq!(output.stats.get(Stat::Defense),1.0);
    assert_eq!(output.stats.get(Stat::MagicDefense),-50.0);
    assert!(!output.stats.contains(Stat::MagicDtPct));
    assert_eq!(output.stats.get(Stat::MagicDamageTaken),-25.0);
    let mut missing=input.clone();missing.stats.remove(Stat::MagicDtPct);
    assert_legacy(build_enemy(&missing,&effects).unwrap_err(),"KeyError","'Magic DT%'");
    effects.enemy_debuffs.set(Stat::Evasion,5.0);
    assert_legacy(build_enemy(&input,&effects).unwrap_err(),"KeyError","'Evasion'");
}

fn assert_legacy(error:CalcError,category:&str,message:&str) {
    match error { CalcError::Legacy{category:actual,message:actual_message,..} => { assert_eq!(actual,category);assert_eq!(actual_message,message); },other => panic!("expected legacy error, got {other}") }
}
