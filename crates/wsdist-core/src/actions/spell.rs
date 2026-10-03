//! Literal analytical casting and ranged-attack paths from actions.py:823–1340.
//! Casting does not consume either random stream, including the EnSpell path.

mod registry;

pub use registry::{SPELL_REGISTRY, SpellSpec, gui_spell_type, optimizer_spell_type, resolve_spell_name};

use crate::math::{
    self, BlmElement, BlmTier, NinjutsuTier, PdifSkill, get_avg_pdif_ranged,
    get_avg_phys_damage, get_dstat_macc, get_enspell_damage, get_fstr2,
    get_magic_hit_rate, get_mv_blm, get_mv_ninjutsu, get_resist_state_average,
    get_tp, numpy_round1, python_int,
};
use crate::model::{
    Ability, ActionResult, CalcError, Enemy, EvaluationContext, Job, Metric,
    PieceKind, Player, RandomSource, SkillType, Slot, SpellRequest, SpellType, Storm,
};
use crate::stats::{Stat, Stats};

use super::common::EMPYREAN_AM;
use registry::SpellKernel;

const OP: &str = "cast_spell";
const BURST_RANK_BONUS: [f64; 15] = [1.5, 1.15, 0.85, 0.60, 0.50, 0.40, 0.15, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

pub fn evaluate_spell<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    request: &SpellRequest,
    ctx: &mut EvaluationContext<R>,
) -> Result<ActionResult, CalcError> {
    let spec = SPELL_REGISTRY.get(request.spell.index()).ok_or_else(|| CalcError::Input {
        operation: OP, field: "spell_name", message: format!("unsupported spell ID {}", request.spell.index()),
    })?;
    let stats = &player.stats;
    let enemy_stats = &enemy.stats;
    let catalog = &ctx.catalog;
    let waist = catalog.piece(player.gearset[Slot::Waist.index()]);
    let obi = waist.name.as_ref() == "Hachirin-no-Obi";

    // This lookup precedes even EnSpell and ranged attacks in the source.
    let mut resist_rank_index = player.abilities.resist_rank_index.ok_or_else(|| {
        CalcError::legacy(OP, "Enemy Resist Rank", "ValueError",
            format!("'{}' is not in list", player.abilities.resist_rank_name))
    })? as usize;
    let magic_burst = player.abilities.enabled(Ability::MagicBurst);
    if magic_burst && resist_rank_index > 0 { resist_rank_index -= 1; }
    // The literal 5% rank is intentionally absent from this forced-half list.
    let rank_resist_state = if (7..=13).contains(&resist_rank_index) { 0.5 } else { 1.0 };

    if matches!(spec.kernel, SpellKernel::EnSpell) {
        let dayweather_bonus = if obi && active_storm(player) { 1.0 + weather_potency(player) } else { 1.0 };
        let main = catalog.piece(player.gearset[Slot::Main.index()]);
        let main_skill_type = ctx.state.skill(main);
        let base_delay = (stats.require(Stat::Delay1, OP)? + stats.require(Stat::Delay2, OP)?) / 2.0;
        let mdelay = (base_delay - stats.get(Stat::MartialArts)) * (1.0 - stats.get(Stat::DualWield) / 100.0);
        let stp = stats.get(Stat::StoreTp) / 100.0;
        let tp_return = get_tp(1.0, if main_skill_type == SkillType::HandToHand { mdelay / 2.0 } else { mdelay }, stp, false)?;
        let enspell_damage_percent_main = stats.get(Stat::EnSpellDamagePctMain) + stats.get(Stat::EnSpellDamagePct);
        let enspell_damage_main = stats.get(Stat::EnSpellDamageMain) + stats.get(Stat::EnSpellDamage);
        let magic_crit_rate2 = stats.get(Stat::MagicCritRateIi) / 100.0;
        let mut highest_element_bonus = stats.get(Stat::EarthElementalBonus);
        for stat in [Stat::WaterElementalBonus, Stat::WindElementalBonus, Stat::FireElementalBonus, Stat::IceElementalBonus, Stat::ThunderElementalBonus] {
            highest_element_bonus = math::python_max(highest_element_bonus, stats.get(stat));
        }
        let elemental_magic_attack_bonus = highest_element_bonus / 100.0 + stats.get(Stat::ElementalBonus) / 100.0;
        let damage = get_enspell_damage(player.abilities.enhancing_skill, enspell_damage_percent_main, enspell_damage_main)?
            * dayweather_bonus * (1.0 + elemental_magic_attack_bonus) * (1.0 + 0.25 * magic_crit_rate2);
        // EnSpell ignores the chosen metric and does not apply hit rate.
        return Ok(result(damage, tp_return, damage, 0.0, damage));
    }

    let mut dint = stats.require(Stat::Int, OP)? - enemy_stats.require(Stat::Int, OP)?;
    if dint < 0.0 { dint = 0.0; }
    let mut magic_damage_stat = stats.get(Stat::MagicDamage);
    let mut magic_attack = stats.get(Stat::MagicAttack);
    let mut magic_accuracy = stats.get(Stat::MagicAccuracy) + stats.get(Stat::MainMagicAccuracySkill);
    let magic_crit_rate2 = 1.0 + 0.25 * (stats.get(Stat::MagicCritRateIi) / 100.0);
    // Kaustra also satisfies the source's first-word "ra" suffix comparison.
    if matches!(spec.kernel, SpellKernel::Kaustra) && player.main_job == Job::Geo {
        magic_damage_stat += stats.get(Stat::RaMagicDamage);
        magic_attack += stats.get(Stat::RaMagicAttack);
    }
    let enemy_magic_defense = enemy_stats.get(Stat::MagicDefense);
    let enemy_magic_evasion = enemy_stats.get(Stat::MagicEvasion);
    let enemy_mdt = enemy_stats.get(Stat::MagicDamageTaken);
    let mut magic_burst_multiplier = 1.0;
    let mut burst_bonus_multiplier = 1.0;
    if magic_burst {
        magic_accuracy += 100.0 + stats.get(Stat::MagicBurstAccuracy);
        magic_burst_multiplier += 0.35;
        magic_burst_multiplier += BURST_RANK_BONUS[resist_rank_index];
        let burst_bonus1 = if stats.get(Stat::MagicBurstDamage) > 40.0 { 40.0 } else { stats.get(Stat::MagicBurstDamage) };
        burst_bonus_multiplier += burst_bonus1 / 100.0 + stats.get(Stat::MagicBurstDamageIi) / 100.0 + stats.get(Stat::MagicBurstDamageTrait) / 100.0;
    }

    match request.spell_type {
        SpellType::QuickDraw => {
            // Quick Draw obtains its element from the first word, not from the
            // elemental spell's actual element. Unknown stat prefixes default 0.
            let element = spec.first_word_element;
            let dayweather_bonus = matching_weather(player, obi, element);
            magic_accuracy += stats.get(Stat::QuickDrawMagicAccuracy);
            magic_accuracy += stats.require(Stat::Agi, OP)? / 2.0;
            let ranged_dmg = stats.get(Stat::RangedDmg);
            let ammo_dmg = stats.get(Stat::AmmoDmg);
            let ranged_delay = stats.get(Stat::RangedDelay);
            let ammo_delay = stats.get(Stat::AmmoDelay);
            let magic_hit_rate = magic_hit_rate(magic_accuracy, enemy_magic_evasion)?;
            let resist_state = get_resist_state_average(magic_hit_rate);
            let magic_attack_ratio = math::divide(100.0 + magic_attack, 100.0 + enemy_magic_defense, OP, "Magic Defense")?;
            let affinity = affinity(stats, element, false);
            let element_magic_attack_bonus = elemental_bonus(stats, element);
            // Quick Draw uses the opposite MDT sign from elemental/Ninjutsu.
            let magic_multiplier = resist_state * magic_attack_ratio * element_magic_attack_bonus
                * dayweather_bonus * (1.0 - enemy_mdt / 100.0) * affinity * magic_crit_rate2;
            let base_damage = python_int(((ranged_dmg + ammo_dmg) * 2.0 + stats.get(Stat::QuickDrawDamage))
                * (1.0 + stats.get(Stat::QuickDrawDamagePct) / 100.0) + magic_damage_stat, OP, "base_damage")?;
            let damage = base_damage * magic_multiplier;
            // Deliberately raw Store TP, unlike every other casting branch.
            let tp_return = get_tp(1.0, ranged_delay + ammo_delay, stats.get(Stat::StoreTp), false)?;
            Ok(result(damage, tp_return, spell_metric(damage, tp_return, &request.metric), 0.0, damage))
        }
        SpellType::Ninjutsu => {
            let (element, tier) = match spec.kernel {
                SpellKernel::Ninjutsu(element, tier) => (element, tier),
                _ => return Err(CalcError::legacy(OP, "spell_name", "KeyError",
                    format!("'{}'", spec.name.split(':').next().unwrap_or(spec.name)))),
            };
            let dayweather_bonus = matching_weather(player, obi, Some(element));
            magic_attack += stats.get(Stat::NinjutsuMagicAttack);
            magic_damage_stat += stats.get(Stat::NinjutsuMagicDamage);
            magic_accuracy += stats.get(Stat::NinjutsuMagicAccuracy) + stats.get(Stat::NinjutsuSkill);
            let magic_resist_state = get_resist_state_average(magic_hit_rate(magic_accuracy, enemy_magic_evasion)?);
            let magic_attack_ratio = math::divide(100.0 + magic_attack, 100.0 + enemy_magic_defense, OP, "Magic Defense")?;
            let element_magic_attack_bonus = elemental_bonus(stats, Some(element));
            let ninjutsu_damage_multiplier = 1.0 + stats.get(Stat::NinjutsuDamagePct) / 100.0;
            let mut futae_multiplier = 1.0;
            if player.abilities.enabled(Ability::Futae) { futae_multiplier += 0.5 + stats.get(Stat::FutaeBonus) / 100.0; }
            let mut innin_multiplier = 1.0;
            if player.abilities.enabled(Ability::Innin) { innin_multiplier += (0.7 * (30.0 - 10.0) + 10.0) / 100.0; }
            let ninjutsu_skill_potency = ninjutsu_potency(tier, stats.get(Stat::NinjutsuSkill));
            let (m, v) = get_mv_ninjutsu(tier, dint);
            let mut damage = python_int(v + magic_damage_stat + dint * m, OP, "base_damage")?;
            damage = stage(damage, ninjutsu_skill_potency, "Ninjutsu Skill")?;
            damage = stage(damage, futae_multiplier, "Futae")?;
            damage = stage(damage, element_magic_attack_bonus, "Elemental Bonus")?;
            damage = stage(damage, magic_resist_state, "magic resist")?;
            damage = stage(damage, rank_resist_state, "resist rank")?;
            damage = stage(damage, magic_burst_multiplier, "Magic Burst")?;
            damage = stage(damage, burst_bonus_multiplier, "Magic Burst Damage")?;
            damage = stage(damage, dayweather_bonus, "dayweather")?;
            damage = stage(damage, ninjutsu_damage_multiplier, "Ninjutsu Damage%")?;
            damage = stage(damage, magic_attack_ratio, "Magic Attack")?;
            damage = stage(damage, innin_multiplier, "Innin")?;
            damage = stage(damage, 1.0 + enemy_mdt / 100.0, "Magic Damage Taken")?;
            // Ninjutsu always uses damage as its metric, even for TP return.
            Ok(result(damage, 0.0, damage, 0.0, damage))
        }
        SpellType::ElementalMagic => {
            let (element, tier, mp_cost) = match spec.kernel {
                SpellKernel::Elemental(element, tier, mp_cost) => (Some(element), Some(tier), mp_cost),
                SpellKernel::Kaustra => (Some(BlmElement::Dark), None, 0.0),
                SpellKernel::Impact => (Some(BlmElement::Dark), None, 666.0),
                _ => (None, None, 0.0),
            };
            let helix = tier == Some(BlmTier::Helix);
            if helix {
                magic_attack += stats.get(Stat::HelixMagicAttack);
                magic_accuracy += stats.get(Stat::HelixMagicAccuracy);
            }
            let dayweather_bonus = matching_weather(player, obi || helix, element);
            let ebullience = player.abilities.enabled(Ability::Ebullience);
            let mut ebullience_multiplier = 1.0;
            if ebullience { ebullience_multiplier += 0.2 + stats.get(Stat::EbullienceBonus) / 100.0; }
            let klimaform_multiplier = 1.0 + bool_number(player.abilities.enabled(Ability::Klimaform)) * stats.get(Stat::KlimaformDamagePct) / 100.0;
            let base_damage = match spec.kernel {
                SpellKernel::Kaustra => {
                    dint = if dint < 0.0 { 0.0 } else if dint > 300.0 { 300.0 } else { dint };
                    numpy_round1(0.067 * 99.0) * (37.0 + 40.0 * bool_number(ebullience) + python_int(0.67 * dint, OP, "Kaustra dINT")?)
                }
                SpellKernel::Impact => 1.0,
                _ => {
                    // Unsupported names leave tier="" and element="None".
                    // get_mv_blm resolves tier first, producing the literal ''.
                    let tier = match tier {
                        Some(tier) => tier,
                        None => return Err(BlmTier::from_name("").unwrap_err()),
                    };
                    let element = element.expect("a supported elemental tier has an element");
                    let (m, v, window) = get_mv_blm(element, tier, dint)?;
                    python_int(v + magic_damage_stat + (dint - window) * m, OP, "base_damage")?
                }
            };
            magic_accuracy += stats.get(Stat::ElementalMagicSkill)
                + get_dstat_macc(stats.require(Stat::Int, OP)?, enemy_stats.require(Stat::Int, OP)?);
            let resist_state = get_resist_state_average(magic_hit_rate(magic_accuracy, enemy_magic_evasion)?);
            let magic_attack_ratio = math::divide(100.0 + magic_attack, 100.0 + enemy_magic_defense, OP, "Magic Defense")?;
            // The elemental branch really accesses lowercase affinity keys.
            let affinity = affinity(stats, element, true);
            let element_magic_attack_bonus = elemental_bonus(stats, element);
            let mut damage = base_damage;
            damage = stage(damage, element_magic_attack_bonus, "Elemental Bonus")?;
            damage = stage(damage, affinity, "affinity")?;
            damage = stage(damage, resist_state, "magic resist")?;
            damage = stage(damage, rank_resist_state, "resist rank")?;
            damage = stage(damage, magic_burst_multiplier, "Magic Burst")?;
            damage = stage(damage, burst_bonus_multiplier, "Magic Burst Damage")?;
            damage = stage(damage, dayweather_bonus, "dayweather")?;
            damage = stage(damage, magic_attack_ratio, "Magic Attack")?;
            damage = stage(damage, klimaform_multiplier, "Klimaform")?;
            damage = stage(damage, ebullience_multiplier, "Ebullience")?;
            damage = stage(damage, magic_crit_rate2, "Magic Crit Rate II")?;
            damage = stage(damage, 1.0 + enemy_mdt / 100.0, "Magic Damage Taken")?;
            let stp = stats.get(Stat::StoreTp) / 100.0;
            let occult_acumen = stats.get(Stat::OccultAcumen) / 100.0;
            let tp_return = python_int(python_int(mp_cost, OP, "mp_cost")? * occult_acumen, OP, "Occult Acumen")? * (1.0 + stp);
            Ok(result(damage, tp_return, spell_metric(damage, tp_return, &request.metric), 0.0, damage))
        }
        SpellType::RangedAttack => {
            let aftermath = python_int(player.abilities.aftermath(), OP, "Aftermath")?;
            let ranged = catalog.piece(player.gearset[Slot::Ranged.index()]);
            let ammo = catalog.piece(player.gearset[Slot::Ammo.index()]);
            let mut empyrean_am_damage_bonus = 1.0;
            if matches!(ranged.name.as_ref(), "Gandiva" | "Armageddon") && aftermath > 0.0 {
                if aftermath > EMPYREAN_AM.len() as f64 {
                    let message = if aftermath - 1.0 >= 9223372036854775808.0 {
                        "cannot fit 'int' into an index-sized integer"
                    } else { "list index out of range" };
                    return Err(CalcError::legacy(OP, "Aftermath", "IndexError", message));
                }
                empyrean_am_damage_bonus += 2.0 * EMPYREAN_AM[(aftermath - 1.0) as usize];
            }
            let mut relic_hidden_damage_bonus = 1.0;
            if matches!(ranged.name.as_ref(), "Annihilator" | "Yoichinoyumi") { relic_hidden_damage_bonus += 2.0 * 0.13; }
            let mut mythic_am_damage_bonus = 1.0;
            if matches!(ranged.name.as_ref(), "Gastraphetes" | "Death Penalty") && aftermath == 3.0 {
                mythic_am_damage_bonus += 1.0 * 0.4 + 2.0 * 0.2;
            }
            let player_agi = stats.require(Stat::Agi, OP)?;
            let stp = stats.get(Stat::StoreTp) / 100.0;
            let enemy_agi = enemy_stats.get(Stat::Agi);
            let enemy_evasion = math::python_max(1.0, enemy_stats.get_or(Stat::Evasion, 1.0));
            let enemy_defense = math::python_max(1.0, enemy_stats.get_or(Stat::Defense, 1.0));
            let sharpshot = player.abilities.enabled(Ability::Sharpshot);
            let hover_shot = bool_number(player.abilities.enabled(Ability::HoverShot));
            let true_shot = stats.get(Stat::TrueShot) / 100.0;
            let double_shot = stats.get(Stat::DoubleShot) / 100.0;
            let double_shot = if double_shot > 1.0 { 1.0 } else { double_shot };
            let double_shot_damage = stats.get(Stat::DoubleShotDamagePct) / 100.0;
            let triple_shot = stats.get(Stat::TripleShot) / 100.0;
            let triple_shot = if triple_shot > 1.0 { 1.0 } else { triple_shot };
            let triple_shot_damage = stats.get(Stat::TripleShotDamagePct) / 100.0;
            let quad_shot = stats.get(Stat::QuadShot) / 100.0;
            let crit_dagi = if player_agi > enemy_agi { ((player_agi - enemy_agi) / 10.0) / 100.0 } else { 0.0 };
            let crit_rate = stats.get(Stat::CritRate) / 100.0 + stats.get(Stat::RangedCritRate) / 100.0 + crit_dagi;
            let crit_dmg = stats.get(Stat::CritDamage) / 100.0 + stats.get(Stat::RangedCritDamage) / 100.0;
            let pdl_gear = stats.get(Stat::Pdl) / 100.0;
            let pdl_trait = stats.get(Stat::PdlTrait) / 100.0;
            let ranged_delay = ranged.delay;
            let ammo_delay = ammo.delay;
            let ranged_dmg = stats.get(Stat::RangedDmg);
            let ammo_dmg = stats.get(Stat::AmmoDmg);
            let fstr_rng = get_fstr2(ranged_dmg, stats.require(Stat::Str, OP)?, enemy_stats.require(Stat::Vit, OP)?);
            let mut player_rangedattack = stats.get(Stat::RangedAttack);
            let mut player_rangedaccuracy = stats.get(Stat::RangedAccuracy);
            let mut barrage_hits = 0.0;
            if player.abilities.enabled(Ability::Barrage) && matches!(ammo.kind, PieceKind::Bolt | PieceKind::Bullet | PieceKind::Arrow) {
                player_rangedattack += stats.get(Stat::BarrageRangedAttack);
                player_rangedaccuracy += stats.get(Stat::BarrageRangedAccuracy);
                barrage_hits += stats.get(Stat::Barrage);
            }
            // All loaded skill strings are nonempty, including literal "None";
            // it is truthy in Python and must not be replaced by ammo's skill.
            let ranged_skill_type = PdifSkill::Known(ctx.state.skill(ranged));
            let hit_rate_cap = if sharpshot { 0.99 } else { 0.95 };
            let hit_rate_ranged = math::get_hit_rate(player_rangedaccuracy, enemy_evasion, hit_rate_cap);
            let main_hits = expected_barrage_hits(1.0 + barrage_hits, hit_rate_ranged)?;
            let mut tp_return = get_tp(main_hits, ranged_delay + ammo_delay, stp, false)?;
            let avg_pdif_rng = get_avg_pdif_ranged(player_rangedattack, ranged_skill_type, pdl_trait, pdl_gear, enemy_defense, crit_rate)?;
            let ranged_hit_damage = get_avg_phys_damage(ranged_dmg + ammo_dmg, fstr_rng, 0.0, avg_pdif_rng,
                1.0, crit_rate, crit_dmg, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)?;
            let mut phys = ranged_hit_damage * main_hits * relic_hidden_damage_bonus;
            let double_shot_hits = 1.0 * hit_rate_ranged * double_shot;
            tp_return += get_tp(double_shot_hits, ranged_delay + ammo_delay, stp, false)?;
            phys += ranged_hit_damage * double_shot_hits * (1.0 + double_shot_damage);
            let triple_shot_hits = 2.0 * hit_rate_ranged * triple_shot;
            tp_return += get_tp(triple_shot_hits, ranged_delay + ammo_delay, stp, false)?;
            phys += ranged_hit_damage * triple_shot_hits * (1.0 + triple_shot_damage);
            let quad_shot_hits = 3.0 * hit_rate_ranged * quad_shot;
            tp_return += get_tp(quad_shot_hits, ranged_delay + ammo_delay, stp, false)?;
            phys += ranged_hit_damage * quad_shot_hits;
            let head = catalog.piece(player.gearset[Slot::Head.index()]);
            if head.name.contains("Arcadian Beret") {
                let recycle = stats.get(Stat::Recycle);
                let recycle = if recycle > 90.0 { 90.0 } else { recycle };
                tp_return += 50.0 * recycle / 100.0;
                tp_return += 1.0 * 50.0 * recycle / 100.0 * double_shot;
                tp_return += 2.0 * 50.0 * recycle / 100.0 * triple_shot;
                tp_return += 3.0 * 50.0 * recycle / 100.0 * quad_shot;
            }
            let damage = phys * (1.0 + true_shot) * (1.0 + hover_shot) * empyrean_am_damage_bonus * mythic_am_damage_bonus;
            Ok(result(damage, tp_return, ranged_metric(damage, tp_return, &request.metric), damage, 0.0))
        }
        SpellType::DivineMagic => {
            // There is no divine-damage branch in the frozen implementation.
            // GUI Banish/Holy reach its initialized physical/TP zero return;
            // optimizer dispatches those same names to the failing BLM lookup.
            Ok(result(0.0, 0.0, ranged_metric(0.0, 0.0, &request.metric), 0.0, 0.0))
        }
    }
}

#[inline]
fn stage(damage: f64, multiplier: f64, field: &'static str) -> Result<f64, CalcError> {
    python_int(damage * multiplier, OP, field)
}

#[inline]
fn bool_number(value: bool) -> f64 { if value { 1.0 } else { 0.0 } }

fn result(damage: f64, tp_return: f64, metric: f64, physical_damage: f64, magical_damage: f64) -> ActionResult {
    ActionResult { damage, tp_return, seconds_per_round: None, time_to_ws: None,
        metric, invert: 1, physical_damage, magical_damage }
}

fn spell_metric(damage: f64, tp_return: f64, metric: &Metric) -> f64 {
    if metric.name.as_ref() == "TP return" { tp_return } else { damage }
}

fn ranged_metric(damage: f64, tp_return: f64, metric: &Metric) -> f64 {
    match metric.name.as_ref() {
        "TP return" => tp_return,
        "TP > Damage" => damage * tp_return * tp_return / 1e4,
        "Damage > TP" => damage * damage * tp_return / 1e6,
        _ => damage,
    }
}

fn active_storm(player: &Player) -> bool {
    player.abilities.storm_name.as_deref().is_some_and(|name| !name.is_empty())
}

fn weather_potency(player: &Player) -> f64 { if player.abilities.storm_contains_ii { 0.25 } else { 0.1 } }

fn matching_weather(player: &Player, weather_applies: bool, element: Option<BlmElement>) -> f64 {
    let storm_element = match player.abilities.storm {
        Some(Storm::Sand | Storm::SandII) => Some(BlmElement::Earth),
        Some(Storm::Rain | Storm::RainII) => Some(BlmElement::Water),
        Some(Storm::Wind | Storm::WindII) => Some(BlmElement::Wind),
        Some(Storm::Fire | Storm::FireII) => Some(BlmElement::Fire),
        Some(Storm::Hail | Storm::HailII) => Some(BlmElement::Ice),
        Some(Storm::Thunder | Storm::ThunderII) => Some(BlmElement::Thunder),
        Some(Storm::Aurora | Storm::AuroraII) => Some(BlmElement::Light),
        Some(Storm::Void | Storm::VoidII) => Some(BlmElement::Dark),
        _ => None,
    };
    if weather_applies && element.is_some() && element == storm_element {
        1.0 + weather_potency(player)
    } else { 1.0 }
}

fn elemental_bonus(stats: &Stats, element: Option<BlmElement>) -> f64 {
    let stat = element.map(|element| match element {
        BlmElement::Earth => Stat::EarthElementalBonus, BlmElement::Water => Stat::WaterElementalBonus,
        BlmElement::Wind => Stat::WindElementalBonus, BlmElement::Fire => Stat::FireElementalBonus,
        BlmElement::Ice => Stat::IceElementalBonus, BlmElement::Thunder => Stat::ThunderElementalBonus,
        BlmElement::Light => Stat::LightElementalBonus, BlmElement::Dark => Stat::DarkElementalBonus,
    });
    1.0 + (stat.map_or(0.0, |stat| stats.get(stat)) / 100.0 + stats.get(Stat::ElementalBonus) / 100.0)
}

fn affinity(stats: &Stats, element: Option<BlmElement>, lowercase: bool) -> f64 {
    let stat = element.map(|element| match (element, lowercase) {
        (BlmElement::Earth, false) => Stat::EarthAffinity, (BlmElement::Earth, true) => Stat::EarthAffinityLower,
        (BlmElement::Water, false) => Stat::WaterAffinity, (BlmElement::Water, true) => Stat::WaterAffinityLower,
        (BlmElement::Wind, false) => Stat::WindAffinity, (BlmElement::Wind, true) => Stat::WindAffinityLower,
        (BlmElement::Fire, false) => Stat::FireAffinity, (BlmElement::Fire, true) => Stat::FireAffinityLower,
        (BlmElement::Ice, false) => Stat::IceAffinity, (BlmElement::Ice, true) => Stat::IceAffinityLower,
        (BlmElement::Thunder, false) => Stat::ThunderAffinity, (BlmElement::Thunder, true) => Stat::ThunderAffinityLower,
        (BlmElement::Light, false) => Stat::LightAffinity, (BlmElement::Light, true) => Stat::LightAffinityLower,
        (BlmElement::Dark, false) => Stat::DarkAffinity, (BlmElement::Dark, true) => Stat::DarkAffinityLower,
    });
    let potency = stat.map_or(0.0, |stat| stats.get(stat));
    1.0 + 0.05 * potency + 0.05 * bool_number(potency > 0.0)
}

fn magic_hit_rate(magic_accuracy: f64, enemy_magic_evasion: f64) -> Result<f64, CalcError> {
    if enemy_magic_evasion > 0.0 { get_magic_hit_rate(magic_accuracy, enemy_magic_evasion) } else { Ok(1.0) }
}

fn ninjutsu_potency(tier: NinjutsuTier, skill: f64) -> f64 {
    match tier {
        NinjutsuTier::Ichi => if skill > 50.0 {
            if skill <= 250.0 { (100.0 + (skill - 50.0) / 2.0) / 100.0 } else { 2.0 }
        } else { 1.0 },
        NinjutsuTier::Ni => if skill > 126.0 {
            if skill <= 350.0 { (100.0 + (skill - 126.0) / 2.0) / 100.0 } else { 2.12 }
        } else { 1.0 },
        NinjutsuTier::San => if skill > 276.0 {
            if skill <= 500.0 { (100.0 + (skill - 276.0) / 2.0) / 100.0 } else { 2.12 }
        } else { 1.0 },
    }
}

// Iterate NumPy arange's attempted barrage positions without allocating its
// temporary array. Fractional positive nhits includes the final partial index.
fn expected_barrage_hits(nhits: f64, hit_rate: f64) -> Result<f64, CalcError> {
    if nhits.is_nan() {
        return Err(CalcError::legacy(OP, "Barrage", "ValueError", "arange: cannot compute length"));
    }
    if nhits.is_infinite() || nhits.ceil() >= isize::MAX as f64 {
        return Err(CalcError::legacy(OP, "Barrage", "ValueError", "Maximum allowed size exceeded"));
    }
    if nhits <= 0.0 { return Ok(0.0); }
    let count = nhits.ceil() as usize;
    let mut hits = 0.0;
    for i in 0..count { hits += hit_rate.powf(i as f64 + 1.0); }
    Ok(hits)
}
