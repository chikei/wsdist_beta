use std::sync::Arc;

use crate::catalog::{BuffDefinition, BuffTableKind, BuffValue, Catalog, ROLL_NUMERALS};
use crate::math::python_int;
use crate::model::{ActiveEffects, Ability, BuffInputs, CalcError, EffectBucket, Enemy, EnemyInput};
use crate::stats::{Stat, Stats};

mod int_text;
use int_text::DecimalInteger;

/// Parse the same Python integer text accepted by legacy GUI calculation inputs.
pub fn parse_integer_text(input: &str, field: &'static str) -> Result<f64, CalcError> {
    DecimalInteger::parse(input, field)?.as_float()
}

pub fn aggregate_buffs(input: &BuffInputs, catalog: &Catalog) -> Result<ActiveEffects, CalcError> {
    let mut brd = Stats::default();
    let mut cor = Stats::default();
    let mut geo = Stats::default();
    let mut whm = Stats::default();
    let mut food = Stats::default();
    let mut cor_debuffs = Stats::default();
    let mut geo_debuffs = Stats::default();
    let mut whm_debuffs = Stats::default();
    let mut other_debuffs = Stats::default();
    let tables = catalog.buffs();

    if input.brd_enabled {
        for (index, name) in input.songs.iter().enumerate() {
            if let Some(song) = tables.table(BuffTableKind::Brd).get(name) {
                let bonus = DecimalInteger::parse(input.song_bonus.rsplit('+').next().unwrap(), "song_bonus")?
                    .capped(song.song_bonus_cap.expect("loaded song has a validated cap"))?;
                let soul_voice = 1.0 + f64::from(input.soul_voice);
                let marcato = if index == 0 { 1.0 + 0.5 * f64::from(input.marcato) } else { 1.0 };
                let minuet = contains_ascii_case_insensitive(name, b"minuet");
                for entry in &song.stats {
                    let BuffValue::Scaling(scaling) = entry.value else { unreachable!("loaded BRD values are Scaling") };
                    let job_gift = if minuet && matches!(entry.stat, Stat::Attack | Stat::RangedAttack) { 20.0 } else { 0.0 };
                    brd.add(entry.stat, soul_voice * marcato * (scaling.base + bonus * scaling.per_point) + job_gift);
                }
            }
        }
    }
    brd.set(Stat::Attack, python_int(brd.get(Stat::Attack), "aggregate_buffs", "brd Attack")?);
    brd.set(Stat::RangedAttack, python_int(brd.get(Stat::RangedAttack), "aggregate_buffs", "brd Ranged Attack")?);

    if input.cor_enabled {
        for (index, selection) in input.rolls.iter().enumerate() {
            let name = selection.name.split(|ch: char| ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')).find(|word| !word.is_empty()).ok_or_else(||
                CalcError::legacy("aggregate_buffs", "roll_name", "IndexError", "list index out of range"))?;
            if let Some(roll) = tables.table(BuffTableKind::Cor).get(name) {
                let bonus = DecimalInteger::parse(input.roll_bonus.rsplit('+').next().unwrap(), "roll_bonus")?.as_float()?;
                let potency = ROLL_NUMERALS.iter().position(|name| *name == selection.potency.as_ref()).ok_or_else(||
                    CalcError::legacy("aggregate_buffs", "roll_potency", "KeyError", format!("'{}'", selection.potency)))?;
                let crooked = if matches!(index, 0 | 2) { 1.0 + 0.2 * f64::from(input.crooked_cards) } else { 1.0 };
                for entry in &roll.stats {
                    let BuffValue::CorRoll(value) = entry.value else { unreachable!("loaded COR values are CorRoll") };
                    let job_bonus = f64::from(input.cor_job_bonus) * value.job_bonus;
                    cor.add(entry.stat, crooked * (value.potency[potency] + bonus * value.per_roll_bonus + job_bonus));
                }
            }
        }
    }
    if input.light_shot && input.whm_enabled && contains_ascii_case_insensitive(&input.whm_selections[0], b"dia") {
        let light_shot = tables.table(BuffTableKind::CorDebuffs).get("Light Shot").ok_or_else(||
            CalcError::legacy("aggregate_buffs", "Light Shot", "KeyError", "'Light Shot'"))?;
        add_constants(&mut cor_debuffs, light_shot);
    }

    if input.geo_enabled {
        for (index, selection) in input.bubbles.iter().enumerate() {
            let name = selection.rsplit('-').next().unwrap();
            // Parsing happens even for None/unknown Indi- and Geo- selections.
            let bonus = if index < 2 {
                Some(DecimalInteger::parse(input.bubble_bonus.rsplit('+').next().unwrap(), "bubble_bonus")?)
            } else { None };
            let bolster = if index < 2 { 1.0 + f64::from(input.bolster) } else { 1.0 };
            let bog = if index == 1 { 1.0 + 0.5 * f64::from(input.blaze_of_glory) } else { 1.0 };
            if let Some(bubble) = tables.table(BuffTableKind::Geo).get(name) {
                let bonus = match &bonus { Some(bonus) => bonus.as_float()?, None => 0.0 };
                for entry in &bubble.stats {
                    let BuffValue::Scaling(scaling) = entry.value else { unreachable!("loaded GEO values are Scaling") };
                    geo.add(entry.stat, bolster * bog * (scaling.base + bonus * scaling.per_point));
                }
            }
            if let Some(bubble) = tables.table(BuffTableKind::GeoDebuffs).get(name) {
                let text = if input.bubble_potency.is_empty() { "0" } else { &input.bubble_potency };
                let potency = DecimalInteger::parse(text, "bubble_potency")?.divided_by_100()?.max(0.0);
                let bonus = match &bonus { Some(bonus) => bonus.as_float()?, None => 0.0 };
                for entry in &bubble.stats {
                    let BuffValue::Scaling(scaling) = entry.value else { unreachable!("loaded GEO debuff values are Scaling") };
                    geo_debuffs.add(entry.stat, bolster * bog * (scaling.base + bonus * scaling.per_point) * potency);
                }
            }
        }
    }

    if input.whm_enabled {
        for selection in &input.whm_selections {
            if let Some(spell) = tables.table(BuffTableKind::Whm).get(selection) { add_constants(&mut whm, spell); }
            if let Some(spell) = tables.table(BuffTableKind::WhmDebuffs).get(selection) { add_constants(&mut whm_debuffs, spell); }
        }
    }
    if input.shell_v {
        let shell = tables.table(BuffTableKind::Whm).get("Shell V").ok_or_else(||
            CalcError::legacy("aggregate_buffs", "Shell V", "KeyError", "'Shell V'"))?;
        add_constants(&mut whm, shell);
    }
    if let Some(id) = catalog.food_by_name(&input.food_name) {
        for entry in &catalog.piece(id).stats { food.add(entry.stat, entry.value); }
    }
    if let Some(attack) = food.remove(Stat::Attack) { food.set(Stat::FoodAttack, attack); }
    if let Some(attack) = food.remove(Stat::RangedAttack) { food.set(Stat::FoodRangedAttack, attack); }

    // Qt sorts the merged special-toggle table before traversing numeric rows.
    for (name, ability) in [
        ("Angon", Ability::Angon), ("Armor Break", Ability::ArmorBreak),
        ("Box Step", Ability::BoxStep), ("Box Step (sub)", Ability::BoxStepSub),
        ("Corrosive Ooze", Ability::CorrosiveOoze), ("Distract III", Ability::DistractIii),
        ("Swooping Frenzy", Ability::SwoopingFrenzy),
    ] {
        if input.miscellaneous.enabled(ability)
            && let Some(debuff) = tables.table(BuffTableKind::MiscDebuffs).get(name)
        { add_constants(&mut other_debuffs, debuff); }
    }
    let mut combined = Stats::default();
    for source in [&cor_debuffs, &geo_debuffs, &whm_debuffs, &other_debuffs] {
        for (stat, value) in source.ordered() { combined.add(stat, value); }
    }
    Ok(ActiveEffects {
        buckets: vec![
            EffectBucket { name: Arc::from("brd"), stats: brd }, EffectBucket { name: Arc::from("cor"), stats: cor },
            EffectBucket { name: Arc::from("geo"), stats: geo }, EffectBucket { name: Arc::from("whm"), stats: whm },
            EffectBucket { name: Arc::from("food"), stats: food },
        ],
        enemy_debuffs: combined,
    })
}

fn add_constants(stats: &mut Stats, definition: &BuffDefinition) {
    for entry in &definition.stats {
        let BuffValue::Constant(value) = entry.value else { unreachable!("loaded constant buff table has typed values") };
        stats.add(entry.stat, value);
    }
}

fn contains_ascii_case_insensitive(value: &str, needle: &[u8]) -> bool {
    value.as_bytes().windows(needle.len()).any(|part| part.eq_ignore_ascii_case(needle))
}

pub fn build_enemy(input: &EnemyInput, effects: &ActiveEffects) -> Result<Enemy, CalcError> {
    let mut stats = input.stats.clone();
    for (stat, debuff) in effects.enemy_debuffs.ordered() {
        let value = stats.require(stat, "build_enemy")?;
        stats.set(stat, if stat == Stat::Defense { value * (1.0 - debuff) } else { value - debuff });
    }
    stats.set(Stat::BaseDefense, input.stats.require(Stat::Defense, "build_enemy")?);
    stats.set(Stat::Defense, stats.get_or(Stat::Defense, 1.0).max(1.0));
    stats.set(Stat::MagicDefense, stats.get(Stat::MagicDefense).max(-50.0));
    let magic_dt = stats.require(Stat::MagicDtPct, "build_enemy")?;
    stats.remove(Stat::MagicDtPct);
    stats.set(Stat::MagicDamageTaken, magic_dt);
    Ok(Enemy { name: Arc::clone(&input.name), location: Arc::clone(&input.location), stats })
}
