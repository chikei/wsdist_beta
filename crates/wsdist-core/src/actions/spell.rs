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
    let waist = ctx.catalog.piece(player.gearset[Slot::Waist.index()]);
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
        let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
        let main_skill_type = ctx.state.skill(main);
        let base_delay = (stats.require(Stat::Delay1, OP)? + stats.require(Stat::Delay2, OP)?) / 2.0;
        let mdelay = (base_delay - stats.get(Stat::MartialArts)) * (1.0 - stats.get(Stat::DualWield) / 100.0);
        let stp = stats.get(Stat::StoreTp) / 100.0;
        let tp_delay = if main_skill_type == SkillType::HandToHand { mdelay / 2.0 } else { mdelay };
        let tp_return = get_tp(1.0, tp_delay, stp, false)?;
        let enspell_damage_percent_main = stats.get(Stat::EnSpellDamagePctMain) + stats.get(Stat::EnSpellDamagePct);
        let enspell_damage_main = stats.get(Stat::EnSpellDamageMain) + stats.get(Stat::EnSpellDamage);
        let magic_crit_rate2 = stats.get(Stat::MagicCritRateIi) / 100.0;
        let mut highest_element_bonus = stats.get(Stat::EarthElementalBonus);
        for stat in [Stat::WaterElementalBonus, Stat::WindElementalBonus, Stat::FireElementalBonus, Stat::IceElementalBonus, Stat::ThunderElementalBonus] {
            highest_element_bonus = math::python_max(highest_element_bonus, stats.get(stat));
        }
        let elemental_magic_attack_bonus = highest_element_bonus / 100.0 + stats.get(Stat::ElementalBonus) / 100.0;
        let enspell_base = get_enspell_damage(player.abilities.enhancing_skill, enspell_damage_percent_main, enspell_damage_main)?;
        let damage = enspell_base * dayweather_bonus * (1.0 + elemental_magic_attack_bonus) * (1.0 + 0.25 * magic_crit_rate2);
        if ctx.calculation_trace.is_some() {
            ctx.record_calculation("EnSpell damage bonuses",
                "Flat EnSpell Damage = {main_flat} + {all_flat} = {flat}; EnSpell Damage percent = {main_percent} + {all_percent} = {percent}",
                &[("main_flat", stats.get(Stat::EnSpellDamageMain)), ("all_flat", stats.get(Stat::EnSpellDamage)),
                    ("flat", enspell_damage_main), ("main_percent", stats.get(Stat::EnSpellDamagePctMain)),
                    ("all_percent", stats.get(Stat::EnSpellDamagePct)), ("percent", enspell_damage_percent_main)]);
            ctx.record_calculation("EnSpell base damage",
                if player.abilities.enhancing_skill < 600.0 {
                    "D_base = (int(({skill} - 223) / 7.70) + 29 + {flat}) * (1 + {percent} / 100) = {base}; int truncates toward zero"
                } else {
                    "D_base = (int(({skill} - 202.5) / 8.05) + 29 + {flat}) * (1 + {percent} / 100) = {base}; int truncates toward zero"
                },
                &[("skill", player.abilities.enhancing_skill), ("flat", enspell_damage_main),
                    ("percent", enspell_damage_percent_main), ("base", enspell_base)]);
            ctx.record_calculation("EnSpell elemental bonus",
                "Elemental bonus = highest of the six elemental bonuses ({highest}) / 100 + {all_elements} / 100 = {bonus}",
                &[("highest", highest_element_bonus), ("all_elements", stats.get(Stat::ElementalBonus)),
                    ("bonus", elemental_magic_attack_bonus)]);
            ctx.record_calculation("EnSpell weather bonus",
                "Weather multiplier = 1 + {obi} * {storm} * {potency} = {weather}; the EnSpell branch requires an obi and an active storm, but no element match",
                &[("obi", bool_number(obi)), ("storm", bool_number(active_storm(player))),
                    ("potency", weather_potency(player)), ("weather", dayweather_bonus)]);
            ctx.record_calculation("Expected EnSpell damage",
                "D = {base} * {weather} * (1 + {elemental}) * (1 + 0.25 * {crit_rate}) = {damage}; Magic Crit II is averaged, not a [random magic crit roll: U(0,1)]; EnSpell does not apply hit rate",
                &[("base", enspell_base), ("weather", dayweather_bonus), ("elemental", elemental_magic_attack_bonus),
                    ("crit_rate", magic_crit_rate2), ("damage", damage)]);
            ctx.record_calculation("EnSpell TP delay",
                if main_skill_type == SkillType::HandToHand {
                    "Base delay = ({delay1} + {delay2}) / 2 = {base}; modified delay = ({base} - {martial_arts}) * (1 - {dual_wield} / 100) = {modified}; Hand-to-Hand TP delay = {modified} / 2 = {tp_delay}"
                } else {
                    "Base delay = ({delay1} + {delay2}) / 2 = {base}; TP delay = ({base} - {martial_arts}) * (1 - {dual_wield} / 100) = {tp_delay}"
                },
                &[("delay1", stats.get(Stat::Delay1)), ("delay2", stats.get(Stat::Delay2)), ("base", base_delay),
                    ("martial_arts", stats.get(Stat::MartialArts)), ("dual_wield", stats.get(Stat::DualWield)),
                    ("modified", mdelay), ("tp_delay", tp_delay)]);
            record_tp(ctx, "EnSpell TP return", 1.0, tp_delay, stp, tp_return);
            ctx.record_calculation("EnSpell metric",
                "Metric = damage = {damage}; the legacy EnSpell branch ignores the selected metric",
                &[("damage", damage)]);
        }
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
    if ctx.calculation_trace.is_some()
        && !matches!(request.spell_type, SpellType::RangedAttack | SpellType::DivineMagic)
    {
        ctx.record_calculation("Casting inputs",
            "dINT = max(0, {player_int} - {enemy_int}) = {dint}; Magic Damage = {magic_damage}; Magic Attack = {magic_attack}; starting Magic Accuracy = {magic_accuracy}",
            &[("player_int", stats.get(Stat::Int)), ("enemy_int", enemy_stats.get(Stat::Int)),
                ("dint", dint), ("magic_damage", magic_damage_stat), ("magic_attack", magic_attack),
                ("magic_accuracy", magic_accuracy)]);
        ctx.record_calculation("Starting magic accuracy",
            if magic_burst {
                "Magic Accuracy = {gear} + main-hand Magic Accuracy Skill {skill}; then add 100 + {burst_bonus} for Magic Burst = {accuracy}"
            } else {
                "Magic Accuracy = {gear} + main-hand Magic Accuracy Skill {skill} = {accuracy}"
            },
            &[("gear", stats.get(Stat::MagicAccuracy)), ("skill", stats.get(Stat::MainMagicAccuracySkill)),
                ("burst_bonus", stats.get(Stat::MagicBurstAccuracy)), ("accuracy", magic_accuracy)]);
        if matches!(spec.kernel, SpellKernel::Kaustra) && player.main_job == Job::Geo {
            ctx.record_calculation("Legacy Kaustra GEO adjustments",
                "Magic Damage = {base_damage} + ra Magic Damage {ra_damage} = {damage}; Magic Attack = {base_attack} + ra Magic Attack {ra_attack} = {attack}; the legacy first-word ra suffix rule also matches Kaustra",
                &[("base_damage", stats.get(Stat::MagicDamage)), ("ra_damage", stats.get(Stat::RaMagicDamage)),
                    ("damage", magic_damage_stat), ("base_attack", stats.get(Stat::MagicAttack)),
                    ("ra_attack", stats.get(Stat::RaMagicAttack)), ("attack", magic_attack)]);
        }
        if magic_burst {
            ctx.record_calculation("Magic burst bonuses",
                "Burst multiplier = 1 + 0.35 + {rank_bonus} = {burst}; burst damage multiplier = 1 + min(40, {mb1}) / 100 + {mb2} / 100 + {trait} / 100 = {bonus}; starting Magic Accuracy already includes 100 + {accuracy_bonus}",
                &[("rank_bonus", BURST_RANK_BONUS[resist_rank_index]), ("burst", magic_burst_multiplier),
                    ("mb1", stats.get(Stat::MagicBurstDamage)), ("mb2", stats.get(Stat::MagicBurstDamageIi)),
                    ("trait", stats.get(Stat::MagicBurstDamageTrait)), ("bonus", burst_bonus_multiplier),
                    ("accuracy_bonus", stats.get(Stat::MagicBurstAccuracy))]);
        }
        ctx.record_calculation("Adjusted resistance rank",
            "Adjusted rank index = {rank}; rank resistance multiplier = {resist}; indices 7 through 13 force 1/2, all others (including the legacy 5% rank) use 1",
            &[("rank", resist_rank_index as f64), ("resist", rank_resist_state)]);
    }

    match request.spell_type {
        SpellType::QuickDraw => {
            // Quick Draw obtains its element from the first word, not from the
            // elemental spell's actual element. Unknown stat prefixes default 0.
            let element = spec.first_word_element;
            let dayweather_bonus = matching_weather(player, obi, element, ctx);
            magic_accuracy += stats.get(Stat::QuickDrawMagicAccuracy);
            magic_accuracy += stats.require(Stat::Agi, OP)? / 2.0;
            let ranged_dmg = stats.get(Stat::RangedDmg);
            let ammo_dmg = stats.get(Stat::AmmoDmg);
            let ranged_delay = stats.get(Stat::RangedDelay);
            let ammo_delay = stats.get(Stat::AmmoDelay);
            let magic_hit_rate = magic_hit_rate(magic_accuracy, enemy_magic_evasion)?;
            let resist_state = get_resist_state_average(magic_hit_rate);
            let magic_attack_ratio = math::divide(100.0 + magic_attack, 100.0 + enemy_magic_defense, OP, "Magic Defense")?;
            let affinity = affinity(stats, element, false, ctx);
            let element_magic_attack_bonus = elemental_bonus(stats, element, ctx);
            // Quick Draw uses the opposite MDT sign from elemental/Ninjutsu.
            let magic_multiplier = resist_state * magic_attack_ratio * element_magic_attack_bonus
                * dayweather_bonus * (1.0 - enemy_mdt / 100.0) * affinity * magic_crit_rate2;
            let base_damage = python_int(((ranged_dmg + ammo_dmg) * 2.0 + stats.get(Stat::QuickDrawDamage))
                * (1.0 + stats.get(Stat::QuickDrawDamagePct) / 100.0) + magic_damage_stat, OP, "base_damage")?;
            let damage = base_damage * magic_multiplier;
            // Deliberately raw Store TP, unlike every other casting branch.
            let tp_return = get_tp(1.0, ranged_delay + ammo_delay, stats.get(Stat::StoreTp), false)?;
            let metric = spell_metric(damage, tp_return, &request.metric);
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Quick Draw magic accuracy",
                    "Final Magic Accuracy = starting Magic Accuracy + {quick_draw} + {agi} / 2 = {accuracy}",
                    &[("quick_draw", stats.get(Stat::QuickDrawMagicAccuracy)), ("agi", stats.get(Stat::Agi)),
                        ("accuracy", magic_accuracy)]);
                record_magic_resistance(ctx, magic_accuracy, enemy_magic_evasion, magic_hit_rate, resist_state);
                record_magic_attack(ctx, magic_attack, enemy_magic_defense, magic_attack_ratio);
                ctx.record_calculation("Quick Draw base damage",
                    "D_base = int((({ranged} + {ammo}) * 2 + {flat}) * (1 + {percent} / 100) + {magic_damage}) = {base}; int truncates toward zero",
                    &[("ranged", ranged_dmg), ("ammo", ammo_dmg), ("flat", stats.get(Stat::QuickDrawDamage)),
                        ("percent", stats.get(Stat::QuickDrawDamagePct)), ("magic_damage", magic_damage_stat),
                        ("base", base_damage)]);
                ctx.record_calculation("Expected Magic Crit II",
                    "Critical multiplier = 1 + 0.25 * ({rate} / 100) = {multiplier}; this is averaged instead of taking a [random magic crit roll: U(0,1)]",
                    &[("rate", stats.get(Stat::MagicCritRateIi)), ("multiplier", magic_crit_rate2)]);
                ctx.record_calculation("Quick Draw magic multiplier",
                    "Multiplier = {resist} * {attack} * {elemental} * {weather} * (1 - {mdt} / 100) * {affinity} * {crit} = {multiplier}; Quick Draw uses the legacy minus-MDT sign and no rank/burst damage stages",
                    &[("resist", resist_state), ("attack", magic_attack_ratio), ("elemental", element_magic_attack_bonus),
                        ("weather", dayweather_bonus), ("mdt", enemy_mdt), ("affinity", affinity),
                        ("crit", magic_crit_rate2), ("multiplier", magic_multiplier)]);
                ctx.record_calculation("Quick Draw damage",
                    "D = {base} * {multiplier} = {damage}; no further integer truncation",
                    &[("base", base_damage), ("multiplier", magic_multiplier), ("damage", damage)]);
                record_tp(ctx, "Quick Draw TP (legacy raw Store TP)", 1.0, ranged_delay + ammo_delay,
                    stats.get(Stat::StoreTp), tp_return);
                record_metric(ctx, &request.metric, damage, tp_return, metric, false);
            }
            Ok(result(damage, tp_return, metric, 0.0, damage))
        }
        SpellType::Ninjutsu => {
            let (element, tier) = match spec.kernel {
                SpellKernel::Ninjutsu(element, tier) => (element, tier),
                _ => return Err(CalcError::legacy(OP, "spell_name", "KeyError",
                    format!("'{}'", spec.name.split(':').next().unwrap_or(spec.name)))),
            };
            let dayweather_bonus = matching_weather(player, obi, Some(element), ctx);
            magic_attack += stats.get(Stat::NinjutsuMagicAttack);
            magic_damage_stat += stats.get(Stat::NinjutsuMagicDamage);
            magic_accuracy += stats.get(Stat::NinjutsuMagicAccuracy) + stats.get(Stat::NinjutsuSkill);
            let hit_rate = magic_hit_rate(magic_accuracy, enemy_magic_evasion)?;
            let magic_resist_state = get_resist_state_average(hit_rate);
            let magic_attack_ratio = math::divide(100.0 + magic_attack, 100.0 + enemy_magic_defense, OP, "Magic Defense")?;
            let element_magic_attack_bonus = elemental_bonus(stats, Some(element), ctx);
            let ninjutsu_damage_multiplier = 1.0 + stats.get(Stat::NinjutsuDamagePct) / 100.0;
            let mut futae_multiplier = 1.0;
            if player.abilities.enabled(Ability::Futae) { futae_multiplier += 0.5 + stats.get(Stat::FutaeBonus) / 100.0; }
            let mut innin_multiplier = 1.0;
            if player.abilities.enabled(Ability::Innin) { innin_multiplier += (0.7 * (30.0 - 10.0) + 10.0) / 100.0; }
            let ninjutsu_skill_potency = ninjutsu_potency(tier, stats.get(Stat::NinjutsuSkill), ctx);
            let (m, v) = get_mv_ninjutsu(tier, dint);
            let mut damage = python_int(v + magic_damage_stat + dint * m, OP, "base_damage")?;
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Ninjutsu casting inputs",
                    "Magic Damage = previous Magic Damage + {nin_damage} = {magic_damage}; Magic Attack = previous Magic Attack + {nin_attack} = {magic_attack}; Magic Accuracy = starting Magic Accuracy + {nin_accuracy} + {skill} = {accuracy}",
                    &[("nin_damage", stats.get(Stat::NinjutsuMagicDamage)), ("magic_damage", magic_damage_stat),
                        ("nin_attack", stats.get(Stat::NinjutsuMagicAttack)), ("magic_attack", magic_attack),
                        ("nin_accuracy", stats.get(Stat::NinjutsuMagicAccuracy)), ("skill", stats.get(Stat::NinjutsuSkill)),
                        ("accuracy", magic_accuracy)]);
                ctx.record_calculation("Ninjutsu base damage",
                    "D_base = int(V + Magic Damage + dINT * M) = int({v} + {magic_damage} + {dint} * {m}) = {base}; M and V use the selected tier's legacy dINT window",
                    &[("v", v), ("magic_damage", magic_damage_stat), ("dint", dint), ("m", m), ("base", damage)]);
                record_magic_resistance(ctx, magic_accuracy, enemy_magic_evasion, hit_rate, magic_resist_state);
                record_magic_attack(ctx, magic_attack, enemy_magic_defense, magic_attack_ratio);
                ctx.record_calculation("Ninjutsu ability bonuses",
                    "Futae (enabled={futae_on}) = 1 + 0.5 + {futae_bonus} / 100 when enabled, otherwise 1 = {futae}; Innin = 1 + {innin_on} * (0.7 * (30 - 10) + 10) / 100 = {innin}; Ninjutsu Damage multiplier = 1 + {nin_percent} / 100 = {nin_multiplier}; MDT multiplier = 1 + {mdt} / 100",
                    &[("futae_on", bool_number(player.abilities.enabled(Ability::Futae))),
                        ("futae_bonus", stats.get(Stat::FutaeBonus)), ("futae", futae_multiplier),
                        ("innin_on", bool_number(player.abilities.enabled(Ability::Innin))), ("innin", innin_multiplier),
                        ("nin_percent", stats.get(Stat::NinjutsuDamagePct)), ("nin_multiplier", ninjutsu_damage_multiplier),
                        ("mdt", enemy_mdt)]);
            }
            damage = stage(damage, ninjutsu_skill_potency, "Ninjutsu Skill", ctx)?;
            damage = stage(damage, futae_multiplier, "Futae", ctx)?;
            damage = stage(damage, element_magic_attack_bonus, "Elemental Bonus", ctx)?;
            damage = stage(damage, magic_resist_state, "magic resist", ctx)?;
            damage = stage(damage, rank_resist_state, "resist rank", ctx)?;
            damage = stage(damage, magic_burst_multiplier, "Magic Burst", ctx)?;
            damage = stage(damage, burst_bonus_multiplier, "Magic Burst Damage", ctx)?;
            damage = stage(damage, dayweather_bonus, "dayweather", ctx)?;
            damage = stage(damage, ninjutsu_damage_multiplier, "Ninjutsu Damage%", ctx)?;
            damage = stage(damage, magic_attack_ratio, "Magic Attack", ctx)?;
            damage = stage(damage, innin_multiplier, "Innin", ctx)?;
            damage = stage(damage, 1.0 + enemy_mdt / 100.0, "Magic Damage Taken", ctx)?;
            // Ninjutsu always uses damage as its metric, even for TP return.
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Ninjutsu result and metric",
                    "Damage = {damage}; TP return = 0; metric = damage = {damage}, regardless of the selected metric",
                    &[("damage", damage)]);
            }
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
            let dayweather_bonus = matching_weather(player, obi || helix, element, ctx);
            let ebullience = player.abilities.enabled(Ability::Ebullience);
            let mut ebullience_multiplier = 1.0;
            if ebullience { ebullience_multiplier += 0.2 + stats.get(Stat::EbullienceBonus) / 100.0; }
            let klimaform_multiplier = 1.0 + bool_number(player.abilities.enabled(Ability::Klimaform)) * stats.get(Stat::KlimaformDamagePct) / 100.0;
            let base_damage = match spec.kernel {
                SpellKernel::Kaustra => {
                    dint = if dint < 0.0 { 0.0 } else if dint > 300.0 { 300.0 } else { dint };
                    let level_factor = numpy_round1(0.067 * 99.0);
                    let dint_damage = python_int(0.67 * dint, OP, "Kaustra dINT")?;
                    let base = level_factor * (37.0 + 40.0 * bool_number(ebullience) + dint_damage);
                    if ctx.calculation_trace.is_some() {
                        ctx.record_calculation("Kaustra base damage",
                            "dINT is clamped to [0, 300] = {dint}; dINT damage = int(0.67 * {dint}) = {dint_damage}; level factor = round1(0.067 * 99) = {level}; D_base = {level} * (37 + 40 * {ebullience} + {dint_damage}) = {base}; the legacy base does not add the Magic Damage stat",
                            &[("dint", dint), ("dint_damage", dint_damage), ("level", level_factor),
                                ("ebullience", bool_number(ebullience)), ("base", base)]);
                    }
                    base
                }
                SpellKernel::Impact => {
                    if ctx.calculation_trace.is_some() {
                        ctx.record_calculation("Legacy Impact base damage",
                            "D_base = 1; the frozen Impact branch uses a literal base of 1 instead of an M/V/dINT or Magic Damage formula",
                            &[]);
                    }
                    1.0
                }
                _ => {
                    // Unsupported names leave tier="" and element="None".
                    // get_mv_blm resolves tier first, producing the literal ''.
                    let tier = match tier {
                        Some(tier) => tier,
                        None => return Err(BlmTier::from_name("").unwrap_err()),
                    };
                    let element = element.expect("a supported elemental tier has an element");
                    let (m, v, window) = get_mv_blm(element, tier, dint)?;
                    let base = python_int(v + magic_damage_stat + (dint - window) * m, OP, "base_damage")?;
                    if ctx.calculation_trace.is_some() {
                        ctx.record_calculation(if helix { "Helix base damage" } else { "Elemental base damage" },
                            "D_base = int(V + Magic Damage + (dINT - window) * M) = int({v} + {magic_damage} + ({dint} - {window}) * {m}) = {base}; M/V/window come from the selected element and tier",
                            &[("v", v), ("magic_damage", magic_damage_stat), ("dint", dint),
                                ("window", window), ("m", m), ("base", base)]);
                    }
                    base
                }
            };
            let elemental_skill = stats.get(Stat::ElementalMagicSkill);
            let dstat_macc = get_dstat_macc(stats.require(Stat::Int, OP)?, enemy_stats.require(Stat::Int, OP)?);
            magic_accuracy += elemental_skill + dstat_macc;
            let hit_rate = magic_hit_rate(magic_accuracy, enemy_magic_evasion)?;
            let resist_state = get_resist_state_average(hit_rate);
            let magic_attack_ratio = math::divide(100.0 + magic_attack, 100.0 + enemy_magic_defense, OP, "Magic Defense")?;
            // The elemental branch really accesses lowercase affinity keys.
            let affinity = affinity(stats, element, true, ctx);
            let element_magic_attack_bonus = elemental_bonus(stats, element, ctx);
            if ctx.calculation_trace.is_some() {
                if helix {
                    ctx.record_calculation("Helix casting bonuses",
                        "Magic Attack = previous Magic Attack + {attack_bonus} = {attack}; Magic Accuracy before elemental skill/dSTAT includes Helix Magic Accuracy {accuracy_bonus}; helixes allow matching weather without an obi",
                        &[("attack_bonus", stats.get(Stat::HelixMagicAttack)), ("attack", magic_attack),
                            ("accuracy_bonus", stats.get(Stat::HelixMagicAccuracy))]);
                }
                ctx.record_calculation("Elemental magic accuracy",
                    "dSTAT accuracy bonus = get_dstat_macc(INT={player_int}, enemy INT={enemy_int}) = {dstat}; final Magic Accuracy = starting Magic Accuracy (plus any Helix accuracy) + Elemental Magic Skill {skill} + {dstat} = {accuracy}",
                    &[("player_int", stats.get(Stat::Int)), ("enemy_int", enemy_stats.get(Stat::Int)),
                        ("dstat", dstat_macc), ("skill", elemental_skill), ("accuracy", magic_accuracy)]);
                record_magic_resistance(ctx, magic_accuracy, enemy_magic_evasion, hit_rate, resist_state);
                record_magic_attack(ctx, magic_attack, enemy_magic_defense, magic_attack_ratio);
                ctx.record_calculation("Elemental ability bonuses",
                    "Ebullience (enabled={ebullience_on}) = 1 + 0.2 + {ebullience_bonus} / 100 when enabled, otherwise 1 = {ebullience}; Klimaform = 1 + {klimaform_on} * {klimaform_bonus} / 100 = {klimaform}; MDT multiplier = 1 + {mdt} / 100",
                    &[("ebullience_on", bool_number(ebullience)), ("ebullience_bonus", stats.get(Stat::EbullienceBonus)),
                        ("ebullience", ebullience_multiplier), ("klimaform_on", bool_number(player.abilities.enabled(Ability::Klimaform))),
                        ("klimaform_bonus", stats.get(Stat::KlimaformDamagePct)), ("klimaform", klimaform_multiplier),
                        ("mdt", enemy_mdt)]);
                ctx.record_calculation("Expected Magic Crit II",
                    "Critical multiplier = 1 + 0.25 * ({rate} / 100) = {multiplier}; this is averaged instead of taking a [random magic crit roll: U(0,1)]",
                    &[("rate", stats.get(Stat::MagicCritRateIi)), ("multiplier", magic_crit_rate2)]);
            }
            let mut damage = base_damage;
            damage = stage(damage, element_magic_attack_bonus, "Elemental Bonus", ctx)?;
            damage = stage(damage, affinity, "affinity", ctx)?;
            damage = stage(damage, resist_state, "magic resist", ctx)?;
            damage = stage(damage, rank_resist_state, "resist rank", ctx)?;
            damage = stage(damage, magic_burst_multiplier, "Magic Burst", ctx)?;
            damage = stage(damage, burst_bonus_multiplier, "Magic Burst Damage", ctx)?;
            damage = stage(damage, dayweather_bonus, "dayweather", ctx)?;
            damage = stage(damage, magic_attack_ratio, "Magic Attack", ctx)?;
            damage = stage(damage, klimaform_multiplier, "Klimaform", ctx)?;
            damage = stage(damage, ebullience_multiplier, "Ebullience", ctx)?;
            damage = stage(damage, magic_crit_rate2, "Magic Crit Rate II", ctx)?;
            damage = stage(damage, 1.0 + enemy_mdt / 100.0, "Magic Damage Taken", ctx)?;
            let stp = stats.get(Stat::StoreTp) / 100.0;
            let occult_acumen = stats.get(Stat::OccultAcumen) / 100.0;
            let tp_return = python_int(python_int(mp_cost, OP, "mp_cost")? * occult_acumen, OP, "Occult Acumen")? * (1.0 + stp);
            let metric = spell_metric(damage, tp_return, &request.metric);
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Occult Acumen TP",
                    "TP = int(int({mp_cost}) * {occult_acumen}) * (1 + {store_tp}) = {tp}; MP cost and Occult Acumen truncate before Store TP scaling",
                    &[("mp_cost", mp_cost), ("occult_acumen", occult_acumen), ("store_tp", stp), ("tp", tp_return)]);
                if helix || matches!(spec.kernel, SpellKernel::Kaustra) {
                    ctx.record_calculation("Legacy damage scope",
                        "Returned damage = {damage}; this branch does not accumulate periodic damage ticks",
                        &[("damage", damage)]);
                }
                record_metric(ctx, &request.metric, damage, tp_return, metric, false);
            }
            Ok(result(damage, tp_return, metric, 0.0, damage))
        }
        SpellType::RangedAttack => {
            let aftermath = python_int(player.abilities.aftermath(), OP, "Aftermath")?;
            let ranged = ctx.catalog.piece(player.gearset[Slot::Ranged.index()]);
            let ammo = ctx.catalog.piece(player.gearset[Slot::Ammo.index()]);
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
            let main_tp = get_tp(main_hits, ranged_delay + ammo_delay, stp, false)?;
            let mut tp_return = main_tp;
            let avg_pdif_rng = get_avg_pdif_ranged(player_rangedattack, ranged_skill_type, pdl_trait, pdl_gear, enemy_defense, crit_rate)?;
            let ranged_hit_damage = get_avg_phys_damage(ranged_dmg + ammo_dmg, fstr_rng, 0.0, avg_pdif_rng,
                1.0, crit_rate, crit_dmg, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)?;
            let main_phys = ranged_hit_damage * main_hits * relic_hidden_damage_bonus;
            let mut phys = main_phys;
            let double_shot_hits = 1.0 * hit_rate_ranged * double_shot;
            let double_shot_tp = get_tp(double_shot_hits, ranged_delay + ammo_delay, stp, false)?;
            tp_return += double_shot_tp;
            let double_shot_phys = ranged_hit_damage * double_shot_hits * (1.0 + double_shot_damage);
            phys += double_shot_phys;
            let triple_shot_hits = 2.0 * hit_rate_ranged * triple_shot;
            let triple_shot_tp = get_tp(triple_shot_hits, ranged_delay + ammo_delay, stp, false)?;
            tp_return += triple_shot_tp;
            let triple_shot_phys = ranged_hit_damage * triple_shot_hits * (1.0 + triple_shot_damage);
            phys += triple_shot_phys;
            let quad_shot_hits = 3.0 * hit_rate_ranged * quad_shot;
            let quad_shot_tp = get_tp(quad_shot_hits, ranged_delay + ammo_delay, stp, false)?;
            tp_return += quad_shot_tp;
            let quad_shot_phys = ranged_hit_damage * quad_shot_hits;
            phys += quad_shot_phys;
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Ranged accuracy and Barrage inputs",
                    "Ranged Attack (including eligible Barrage bonus) = {attack}; Ranged Accuracy = {accuracy}; extra Barrage attempts = {barrage}; p_hit = min({cap}, max(0.2, (75 + 0.5 * ({accuracy} - {evasion})) / 100)) = {hit}; enemy evasion is floored at 1",
                    &[("attack", player_rangedattack), ("accuracy", player_rangedaccuracy), ("barrage", barrage_hits),
                        ("cap", hit_rate_cap), ("evasion", enemy_evasion), ("hit", hit_rate_ranged)]);
                ctx.record_calculation("Expected main/Barrage hits",
                    "E[hits] = sum(i=1..ceil(1 + {barrage}), {hit}^i) = {hits}; nonpositive attempt counts give 0; each next shot requires all earlier hits, comparing [random barrage hit roll: U(0,1)] with p_hit; this analytical path draws no rolls",
                    &[("barrage", barrage_hits), ("hit", hit_rate_ranged), ("hits", main_hits)]);
                ctx.record_calculation("Ranged fSTR2",
                    "fSTR2(ranged DMG={ranged}, STR={str}, enemy VIT={vit}) = {fstr}; the legacy helper retains fractional weapon ranks and applies ranged dSTR/fSTR bounds",
                    &[("ranged", ranged_dmg), ("str", stats.get(Stat::Str)), ("vit", enemy_stats.get(Stat::Vit)),
                        ("fstr", fstr_rng)]);
                ctx.record_calculation("Ranged critical inputs",
                    "dAGI critical bonus = (({agi} - {enemy_agi}) / 10) / 100 when AGI > enemy AGI, otherwise 0 = {dagi}; critical rate = {crit_stat} / 100 + {ranged_crit_stat} / 100 + {dagi} = {crit}; critical damage = {crit_damage_stat} / 100 + {ranged_crit_damage_stat} / 100 = {crit_damage}",
                    &[("agi", player_agi), ("enemy_agi", enemy_agi), ("dagi", crit_dagi),
                        ("crit_stat", stats.get(Stat::CritRate)), ("ranged_crit_stat", stats.get(Stat::RangedCritRate)),
                        ("crit", crit_rate), ("crit_damage_stat", stats.get(Stat::CritDamage)),
                        ("ranged_crit_damage_stat", stats.get(Stat::RangedCritDamage)), ("crit_damage", crit_dmg)]);
                ctx.record_calculation("Legacy ranged pDIF bounds",
                    "r = {attack} / {defense}; lower(r) = r for 0 <= r < 0.9, 1 for 0.9 <= r < 1.1, (20/19)*r - 3/19 otherwise; upper(r) = (10/9)*r for 0 <= r < 0.9, 1 for 0.9 <= r < 1.1, r otherwise; enemy defense is floored at 1",
                    &[("attack", player_rangedattack), ("defense", enemy_defense)]);
                ctx.record_calculation("Legacy ranged pDIF clipping",
                    "legacy_clip(q, cap) = 0 when q <= 0, cap when q >= cap, otherwise q; the zero test precedes the cap test",
                    &[]);
                ctx.record_calculation("Legacy average ranged pDIF",
                    if matches!(ranged_skill_type, PdifSkill::Known(SkillType::Marksmanship)) {
                        "pDIF = legacy_clip(0.5 * (lower(r) + upper(r)), (3.5 + {trait}) * (1 + {gear})) * (1 + 0.25 * min({crit}, 1)) = {pdif}; Marksmanship uses base cap 3.5; this is the legacy midpoint approximation, not an integrated sampled mean"
                    } else {
                        "pDIF = legacy_clip(0.5 * (lower(r) + upper(r)), (3.25 + {trait}) * (1 + {gear})) * (1 + 0.25 * min({crit}, 1)) = {pdif}; the loaded ranged weapon skill (not ammo fallback) uses base cap 3.25; this is the legacy midpoint approximation, not an integrated sampled mean"
                    },
                    &[("trait", pdl_trait), ("gear", pdl_gear), ("crit", crit_rate), ("pdif", avg_pdif_rng)]);
                ctx.record_calculation("Ranged stochastic terms (not sampled)",
                    "A sampled attack would use qRatio = [random ranged pDIF: U(lower(r),upper(r))] and critical = ([random ranged crit roll: U(0,1)] < {crit}); this branch uses the legacy average pDIF and capped critical damage expectation instead",
                    &[("crit", crit_rate)]);
                ctx.record_calculation("Expected ranged hit damage",
                    "D_hit = int({ranged} + {ammo} + {fstr}) * {pdif} * (1 + min({crit}, 1) * min({crit_damage}, 1)) = {damage}; only the base truncates toward zero",
                    &[("ranged", ranged_dmg), ("ammo", ammo_dmg), ("fstr", fstr_rng), ("pdif", avg_pdif_rng),
                        ("crit", crit_rate), ("crit_damage", crit_dmg), ("damage", ranged_hit_damage)]);
                ctx.record_calculation("Ranged additional shot probabilities",
                    "Double Shot probability = min({double_stat} / 100, 1) = {double}; Triple Shot probability = min({triple_stat} / 100, 1) = {triple}; Quad Shot probability = {quad_stat} / 100 = {quad}; the legacy path does not normalize these probabilities together",
                    &[("double_stat", stats.get(Stat::DoubleShot)), ("double", double_shot),
                        ("triple_stat", stats.get(Stat::TripleShot)), ("triple", triple_shot),
                        ("quad_stat", stats.get(Stat::QuadShot)), ("quad", quad_shot)]);
                ctx.record_calculation("Expected additional shot hits",
                    "Double Shot: E[extra hits] = 1 * {hit} * {double} = {double_hits}; Triple Shot: 2 * {hit} * {triple} = {triple_hits}; Quad Shot: 3 * {hit} * {quad} = {quad_hits}; these legacy contributions add independently, averaging [random shot proc roll: U(0,1)] and [random ranged hit roll: U(0,1)]",
                    &[("hit", hit_rate_ranged), ("double", double_shot), ("double_hits", double_shot_hits),
                        ("triple", triple_shot), ("triple_hits", triple_shot_hits),
                        ("quad", quad_shot), ("quad_hits", quad_shot_hits)]);
                ctx.record_calculation("Expected main/Barrage damage",
                    "D_main = {hit_damage} * {hits} * {relic} = {damage}; eligible relic weapons use 1 + 2 * 0.13 instead of taking a [random relic damage proc roll: U(0,1)]",
                    &[("hit_damage", ranged_hit_damage), ("hits", main_hits), ("relic", relic_hidden_damage_bonus),
                        ("damage", main_phys)]);
                ctx.record_calculation("Expected additional shot damage",
                    "D_double = {hit_damage} * {double_hits} * (1 + {double_bonus}) = {double_damage}; D_triple = {hit_damage} * {triple_hits} * (1 + {triple_bonus}) = {triple_damage}; D_quad = {hit_damage} * {quad_hits} = {quad_damage}; summed physical damage before final bonuses = {physical}",
                    &[("hit_damage", ranged_hit_damage), ("double_hits", double_shot_hits), ("double_bonus", double_shot_damage),
                        ("double_damage", double_shot_phys), ("triple_hits", triple_shot_hits), ("triple_bonus", triple_shot_damage),
                        ("triple_damage", triple_shot_phys), ("quad_hits", quad_shot_hits), ("quad_damage", quad_shot_phys),
                        ("physical", phys)]);
                record_tp(ctx, "Main/Barrage TP", main_hits, ranged_delay + ammo_delay, stp, main_tp);
                record_tp(ctx, "Double Shot TP", double_shot_hits, ranged_delay + ammo_delay, stp, double_shot_tp);
                record_tp(ctx, "Triple Shot TP", triple_shot_hits, ranged_delay + ammo_delay, stp, triple_shot_tp);
                record_tp(ctx, "Quad Shot TP", quad_shot_hits, ranged_delay + ammo_delay, stp, quad_shot_tp);
            }
            let head = ctx.catalog.piece(player.gearset[Slot::Head.index()]);
            if head.name.contains("Arcadian Beret") {
                let recycle = stats.get(Stat::Recycle);
                let recycle = if recycle > 90.0 { 90.0 } else { recycle };
                let recycle_main_tp = 50.0 * recycle / 100.0;
                tp_return += recycle_main_tp;
                let recycle_double_tp = 1.0 * 50.0 * recycle / 100.0 * double_shot;
                tp_return += recycle_double_tp;
                let recycle_triple_tp = 2.0 * 50.0 * recycle / 100.0 * triple_shot;
                tp_return += recycle_triple_tp;
                let recycle_quad_tp = 3.0 * 50.0 * recycle / 100.0 * quad_shot;
                tp_return += recycle_quad_tp;
                if ctx.calculation_trace.is_some() {
                    ctx.record_calculation("Expected Arcadian Beret Recycle TP",
                        "Recycle = min({stat}, 90) = {recycle}%; added TP in order: 50 * {recycle} / 100 = {main_tp}; 50 * {recycle} / 100 * {double} = {double_tp}; 2 * 50 * {recycle} / 100 * {triple} = {triple_tp}; 3 * 50 * {recycle} / 100 * {quad} = {quad_tp}; TP after additions = {tp}; expectation replaces [random recycle roll: U(0,1)] and does not multiply by ranged hit rate",
                        &[("stat", stats.get(Stat::Recycle)), ("recycle", recycle), ("main_tp", recycle_main_tp),
                            ("double", double_shot), ("double_tp", recycle_double_tp), ("triple", triple_shot),
                            ("triple_tp", recycle_triple_tp), ("quad", quad_shot), ("quad_tp", recycle_quad_tp),
                            ("tp", tp_return)]);
                }
            }
            let damage = phys * (1.0 + true_shot) * (1.0 + hover_shot) * empyrean_am_damage_bonus * mythic_am_damage_bonus;
            let metric = ranged_metric(damage, tp_return, &request.metric);
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Expected ranged weapon damage bonuses",
                    "Aftermath = int({raw_am}) = {am}; applied Empyrean multiplier = {empyrean} (eligible weapons: 1 + 2 * p, with p = 0.3 at AM1, 0.4 at AM2, 0.5 at AM3); applied Mythic multiplier = {mythic} (eligible AM3: 1 + 1 * 0.4 + 2 * 0.2); inactive effects use 1; these average [random aftermath damage proc roll: U(0,1)]",
                    &[("raw_am", player.abilities.aftermath()), ("am", aftermath),
                        ("empyrean", empyrean_am_damage_bonus), ("mythic", mythic_am_damage_bonus)]);
                ctx.record_calculation("Ranged result",
                    "D = {physical} * (1 + {true_shot}) * (1 + {hover_shot}) * {empyrean} * {mythic} = {damage}; total TP return = {tp}; all damage is physical",
                    &[("physical", phys), ("true_shot", true_shot), ("hover_shot", hover_shot),
                        ("empyrean", empyrean_am_damage_bonus), ("mythic", mythic_am_damage_bonus),
                        ("damage", damage), ("tp", tp_return)]);
                record_metric(ctx, &request.metric, damage, tp_return, metric, true);
            }
            Ok(result(damage, tp_return, metric, damage, 0.0))
        }
        SpellType::DivineMagic => {
            // There is no divine-damage branch in the frozen implementation.
            // GUI Banish/Holy reach its initialized physical/TP zero return;
            // optimizer dispatches those same names to the failing BLM lookup.
            let metric = ranged_metric(0.0, 0.0, &request.metric);
            if ctx.calculation_trace.is_some() {
                ctx.record_calculation("Legacy divine zero-output behavior",
                    "Damage = physical damage = magical damage = TP return = 0; the frozen Divine Magic dispatch has no damage kernel and returns its initialized zeros. This is not a calculated Banish/Holy damage formula; optimizer dispatch of those names to the elemental lookup is unchanged",
                    &[]);
                record_metric(ctx, &request.metric, 0.0, 0.0, metric, true);
            }
            Ok(result(0.0, 0.0, metric, 0.0, 0.0))
        }
    }
}

#[inline]
fn stage<R: RandomSource>(damage: f64, multiplier: f64, field: &'static str,
    ctx: &mut EvaluationContext<R>) -> Result<f64, CalcError> {
    let next = python_int(damage * multiplier, OP, field)?;
    if ctx.calculation_trace.is_some() {
        ctx.record_calculation(field,
            "D_next = int({damage} * {multiplier}) = {next}; int truncates toward zero at this stage",
            &[("damage", damage), ("multiplier", multiplier), ("next", next)]);
    }
    Ok(next)
}

fn record_magic_resistance<R: RandomSource>(ctx: &mut EvaluationContext<R>, accuracy: f64,
    evasion: f64, hit_rate: f64, resist: f64) {
    ctx.record_calculation("Magic hit rate",
        if evasion > 0.0 {
            if accuracy - evasion < 0.0 {
                "p_hit = max(0, min(0.95, 0.50 + int(({accuracy} - {evasion}) / 2) / 100)) = {hit}; negative dMAcc truncates after division by 2"
            } else {
                "p_hit = max(0, min(0.95, 0.50 + int({accuracy} - {evasion}) / 100)) = {hit}; int truncates toward zero"
            }
        } else {
            "Enemy Magic Evasion {evasion} is not positive: the legacy branch bypasses the capped hit-rate formula and uses p_hit = 1 = {hit}; Magic Accuracy = {accuracy}"
        },
        &[("accuracy", accuracy), ("evasion", evasion), ("hit", hit_rate)]);
    ctx.record_calculation("Expected magic resistance",
        "E[resist] = {hit} + 0.5 * {hit} * (1 - {hit}) + 0.25 * {hit} * (1 - {hit})^2 + 0.125 * (1 - {hit})^3 = {resist}; a sampled check succeeds when {hit} >= [random resist roll: U(0,1)], halving on failure for up to three checks; this branch uses the expectation and draws no rolls",
        &[("hit", hit_rate), ("resist", resist)]);
}

fn record_magic_attack<R: RandomSource>(ctx: &mut EvaluationContext<R>, attack: f64,
    defense: f64, ratio: f64) {
    ctx.record_calculation("Magic Attack ratio",
        "Magic Attack multiplier = (100 + {attack}) / (100 + {defense}) = {ratio}",
        &[("attack", attack), ("defense", defense), ("ratio", ratio)]);
}

fn record_tp<R: RandomSource>(ctx: &mut EvaluationContext<R>, label: &'static str,
    hits: f64, delay: f64, store_tp: f64, tp: f64) {
    let formula = if delay <= 180.0 {
        "TP = {hits} * int(int(61 + ({delay} - 180) * 63 / 360) * (1 + {store_tp})) = {tp}; both int stages truncate before multiplying expected hits"
    } else if delay <= 540.0 {
        "TP = {hits} * int(int(61 + ({delay} - 180) * 88 / 360) * (1 + {store_tp})) = {tp}; both int stages truncate before multiplying expected hits"
    } else if delay <= 630.0 {
        "TP = {hits} * int(int(149 + ({delay} - 540) * 20 / 360) * (1 + {store_tp})) = {tp}; both int stages truncate before multiplying expected hits"
    } else if delay <= 720.0 {
        "TP = {hits} * int(int(154 + ({delay} - 630) * 28 / 360) * (1 + {store_tp})) = {tp}; both int stages truncate before multiplying expected hits"
    } else if delay <= 900.0 {
        "TP = {hits} * int(int(161 + ({delay} - 720) * 24 / 360) * (1 + {store_tp})) = {tp}; both int stages truncate before multiplying expected hits"
    } else {
        "TP = {hits} * int(int(173 + ({delay} - 900) * 28 / 360) * (1 + {store_tp})) = {tp}; both int stages truncate before multiplying expected hits"
    };
    ctx.record_calculation(label, formula,
        &[("hits", hits), ("delay", delay), ("store_tp", store_tp), ("tp", tp)]);
}

fn record_metric<R: RandomSource>(ctx: &mut EvaluationContext<R>, metric: &Metric,
    damage: f64, tp: f64, value: f64, ranged: bool) {
    let formula = match metric.name.as_ref() {
        "TP return" => "Metric = TP return = {tp} = {value}",
        "TP > Damage" if ranged => "Metric = damage * TP * TP / 10000 = {damage} * {tp} * {tp} / 10000 = {value}",
        "Damage > TP" if ranged => "Metric = damage * damage * TP / 1000000 = {damage} * {damage} * {tp} / 1000000 = {value}",
        _ => "Metric = damage = {damage} = {value}",
    };
    ctx.record_calculation("Returned metric", formula,
        &[("damage", damage), ("tp", tp), ("value", value)]);
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

fn matching_weather<R: RandomSource>(player: &Player, weather_applies: bool, element: Option<BlmElement>,
    ctx: &mut EvaluationContext<R>) -> f64 {
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
    let multiplier = if weather_applies && element.is_some() && element == storm_element {
        1.0 + weather_potency(player)
    } else { 1.0 };
    if ctx.calculation_trace.is_some() {
        ctx.record_calculation("Matching weather bonus",
            "Weather multiplier = 1 + {eligible} * {matching} * {potency} = {multiplier}; eligibility is obi or the helix rule, and only a matching storm contributes",
            &[("eligible", bool_number(weather_applies)),
                ("matching", bool_number(element.is_some() && element == storm_element)),
                ("potency", weather_potency(player)), ("multiplier", multiplier)]);
    }
    multiplier
}

fn elemental_bonus<R: RandomSource>(stats: &Stats, element: Option<BlmElement>,
    ctx: &mut EvaluationContext<R>) -> f64 {
    let stat = element.map(|element| match element {
        BlmElement::Earth => Stat::EarthElementalBonus, BlmElement::Water => Stat::WaterElementalBonus,
        BlmElement::Wind => Stat::WindElementalBonus, BlmElement::Fire => Stat::FireElementalBonus,
        BlmElement::Ice => Stat::IceElementalBonus, BlmElement::Thunder => Stat::ThunderElementalBonus,
        BlmElement::Light => Stat::LightElementalBonus, BlmElement::Dark => Stat::DarkElementalBonus,
    });
    let potency = stat.map_or(0.0, |stat| stats.get(stat));
    let all_elements = stats.get(Stat::ElementalBonus);
    let multiplier = 1.0 + (potency / 100.0 + all_elements / 100.0);
    if ctx.calculation_trace.is_some() {
        ctx.record_calculation("Elemental bonus multiplier",
            "Elemental multiplier = 1 + ({element} / 100 + {all_elements} / 100) = {multiplier}; unknown element stat prefixes contribute 0",
            &[("element", potency), ("all_elements", all_elements), ("multiplier", multiplier)]);
    }
    multiplier
}

fn affinity<R: RandomSource>(stats: &Stats, element: Option<BlmElement>, lowercase: bool,
    ctx: &mut EvaluationContext<R>) -> f64 {
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
    let multiplier = 1.0 + 0.05 * potency + 0.05 * bool_number(potency > 0.0);
    if ctx.calculation_trace.is_some() {
        ctx.record_calculation(if lowercase { "Legacy lowercase affinity" } else { "Affinity" },
            "Affinity multiplier = 1 + 0.05 * {potency} + 0.05 * ({potency} > 0) = {multiplier}; the stat key's original case is preserved",
            &[("potency", potency), ("multiplier", multiplier)]);
    }
    multiplier
}

fn magic_hit_rate(magic_accuracy: f64, enemy_magic_evasion: f64) -> Result<f64, CalcError> {
    if enemy_magic_evasion > 0.0 { get_magic_hit_rate(magic_accuracy, enemy_magic_evasion) } else { Ok(1.0) }
}

fn ninjutsu_potency<R: RandomSource>(tier: NinjutsuTier, skill: f64,
    ctx: &mut EvaluationContext<R>) -> f64 {
    let potency = match tier {
        NinjutsuTier::Ichi => if skill > 50.0 {
            if skill <= 250.0 { (100.0 + (skill - 50.0) / 2.0) / 100.0 } else { 2.0 }
        } else { 1.0 },
        NinjutsuTier::Ni => if skill > 126.0 {
            if skill <= 350.0 { (100.0 + (skill - 126.0) / 2.0) / 100.0 } else { 2.12 }
        } else { 1.0 },
        NinjutsuTier::San => if skill > 276.0 {
            if skill <= 500.0 { (100.0 + (skill - 276.0) / 2.0) / 100.0 } else { 2.12 }
        } else { 1.0 },
    };
    if ctx.calculation_trace.is_some() {
        let formula = match tier {
            NinjutsuTier::Ichi =>
                "Skill potency = if {skill} > 50 then (if {skill} <= 250 then (100 + ({skill} - 50) / 2) / 100 else 2) else 1 = {potency}",
            NinjutsuTier::Ni =>
                "Skill potency = if {skill} > 126 then (if {skill} <= 350 then (100 + ({skill} - 126) / 2) / 100 else 2.12) else 1 = {potency}",
            NinjutsuTier::San =>
                "Skill potency = if {skill} > 276 then (if {skill} <= 500 then (100 + ({skill} - 276) / 2) / 100 else 2.12) else 1 = {potency}",
        };
        ctx.record_calculation("Ninjutsu skill potency", formula,
            &[("skill", skill), ("potency", potency)]);
    }
    potency
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
