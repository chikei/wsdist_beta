//! Analytical and sampled auto-attack paths from actions.py:303–820.
//!
//! These remain separate calculations: the analytical eight-landed-hit
//! approximation is not the sampled eight-attempt cap.

use crate::math::{
    divide, get_avg_pdif_melee, get_avg_pdif_ranged, get_avg_phys_damage,
    get_delay_timing, get_dex_crit, get_enspell_damage, get_fstr, get_fstr2,
    get_hit_rate, get_ma_rate3, get_pdif_melee, get_pdif_ranged, get_phys_damage,
    get_tp, python_int, python_max,
};
use crate::model::{
    Ability, ActionResult, AttackKind, AttackProc, CalcError, Enemy,
    EvaluationContext, HitEvent, Job, Metric, PieceKind, Player, RandomSource,
    RandomStream, RoundSample, SkillType, Slot,
};
use crate::model::CalculationSection as Section;
use crate::stats::Stat;

use super::common::{
    EMPYREAN_AM, EMPYREAN_WEAPONS, ONE_HANDED_SKILLS, PRIME_WEAPONS_2,
    PRIME_WEAPONS_3, RELIC_WEAPONS_20, RELIC_WEAPONS_25, RELIC_WEAPONS_30,
    TWO_HANDED_SKILLS, oa_list, oa_tiers, record_tp_gain,
    weapon_damage_bonuses,
};

const OPERATION: &str = "average_attack_round";

const MELEE_FSTR: (&str, &str) = ("fSTR = clamp(F(STR − VIT) / 4, −D / 9, 8 + D / 9)",
    "clamp(F({strength} − {vitality}) / 4, −{damage} / 9, 8 + {damage} / 9) = {fstr}");
const MELEE_W: (&str, &str) = ("w = Atk / Def + min(Crit rate, 1)",
    "{attack} / {defense} + min({crit_rate}, 1)");
const MELEE_PDIF: (&str, &str) = (
    "pDIF = (clamp((U(w) + L(w)) / 2, 0, (Skill cap + PDL trait) × (1 + PDL gear)) + min(Crit rate, 1)) × 1.025",
    "(clamp((U(w) + L(w)) / 2, 0, (Skill cap + {pdl_trait}) × (1 + {pdl_gear})) + min({crit_rate}, 1)) × 1.025 = {pdif}");
const AVG_HIT: (&str, &str) = ("Avg hit = trunc(D + fSTR) × pDIF × (1 + min(Crit rate, 1) × min(Crit dmg, 1))",
    "trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {base}");
const MAIN_HIT_RATE: (&str, &str) = ("Hit rate = clamp((75 + 0.5 × (Acc − max(1, Eva))) / 100, 0.2, Hit cap)",
    "clamp((75 + 0.5 × ({accuracy1} − max(1, {evasion}))) / 100, 0.2, {main_cap}) = {main_first}");

#[inline]
fn flag(value: bool) -> f64 { if value { 1.0 } else { 0.0 } }

// QA/TA/DA, Daken, and kicks use this comparison order in the source. In
// particular NaN becomes 1, unlike a min/clamp, and negative rates stay negative.
#[inline]
fn upper_rate(percent: f64) -> f64 {
    let rate = percent / 100.0;
    if rate < 1.0 { rate } else { 1.0 }
}

struct RoundSetup {
    dual_wield: bool,
    main_skill: SkillType,
    sub_skill: SkillType,
    ammo_skill: SkillType,
    qa: f64,
    ta: f64,
    da: f64,
    oa: [f64; 9],
    fua: [f64; 9],
    ta_damage: f64,
    da_damage: f64,
    pdl_gear: f64,
    pdl_trait: f64,
    crit_rate: f64,
    crit_damage: f64,
    stp: f64,
    mdelay: f64,
    main_damage: f64,
    sub_damage: f64,
    fstr_main: f64,
    fstr_sub: f64,
    attack1: f64,
    attack2: f64,
    hit_rates: [[f64; 2]; 2],
    enemy_defense: f64,
    daken: f64,
    ammo_damage: f64,
    fstr_ammo: f64,
    ammo_delay: f64,
    ranged_attack: f64,
    hit_rate_ranged: f64,
    kick_damage: f64,
    fstr_kick: f64,
    kickattacks: f64,
    zanshin: f64,
    zanhasso: f64,
    zanshin_hit_rate: f64,
    zanshin_oa2: f64,
    enspell_active: bool,
    main_enspell: f64,
    sub_enspell: f64,
    enhancing_skill: f64,
    enspell_percent_main: f64,
    enspell_percent_sub: f64,
    enspell_damage_main: f64,
    enspell_damage_sub: f64,
    dayweather: f64,
    elemental_bonus: f64,
    magic_crit_rate2: f64,
    aftermath: f64,
    empyrean_bonus: f64,
    relic_bonus: f64,
    prime_bonus: f64,
    dragon_fangs_bonus: f64,
    empyrean: bool,
    relic30: bool,
    relic25: bool,
    relic20: bool,
    prime3: bool,
    prime2: bool,
    verethragna: bool,
}

impl RoundSetup {
    #[inline]
    fn hit_delay(&self) -> f64 {
        if self.main_skill == SkillType::HandToHand { self.mdelay / 2.0 } else { self.mdelay }
    }
}

fn prepare_round<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    starting_tp: f64,
    ctx: &mut EvaluationContext<R>,
) -> Result<RoundSetup, CalcError> {
    let stats = &player.stats;
    let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
    let sub = ctx.catalog.piece(player.gearset[Slot::Sub.index()]);
    let main_skill = ctx.state.skill(main);
    let dual_wield = sub.kind == PieceKind::Weapon || main_skill == SkillType::HandToHand;
    let enemy_defense = python_max(1.0, enemy.stats.require(Stat::Defense, OPERATION)?);
    let enemy_evasion = python_max(1.0, enemy.stats.require(Stat::Evasion, OPERATION)?);
    let qa = upper_rate(stats.get(Stat::Qa));
    let ta = upper_rate(stats.get(Stat::Ta));
    let da = upper_rate(stats.get(Stat::Da));
    let oa = oa_list(stats);
    // The source reads FUA but neither round branch applies it. The verified
    // analytical helper accepts a fixed nine-element slice and ignores it.
    let fua = [stats.get(Stat::FuaMain) / 100.0, stats.get(Stat::FuaSub) / 100.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let ta_damage = stats.get(Stat::TaDamagePct) / 100.0;
    let da_damage = stats.get(Stat::DaDamagePct) / 100.0;
    let pdl_gear = stats.get(Stat::Pdl) / 100.0;
    let pdl_trait = stats.get(Stat::PdlTrait) / 100.0;
    let mut crit_rate = stats.require(Stat::CritRate, OPERATION)? / 100.0
        + get_dex_crit(stats.require(Stat::Dex, OPERATION)?, enemy.stats.require(Stat::Agi, OPERATION)?);
    let crit_damage = stats.get(Stat::CritDamage) / 100.0;
    crit_rate += 0.5 * (1.0 - starting_tp / 3000.0) * flag(main.name.as_ref() == "Tauret");
    crit_rate = if crit_rate > 1.0 { 1.0 } else { crit_rate };
    // Karambit's analytical critical-rate-weighted STP also feeds the sampler.
    let stp = stats.get(Stat::StoreTp) / 100.0
        + (50.0 / 100.0) * crit_rate * flag(main.name.as_ref() == "Karambit");
    let base_delay = (stats.require(Stat::Delay1, OPERATION)? + stats.require(Stat::Delay2, OPERATION)?) / 2.0;
    let mdelay = (base_delay - stats.get(Stat::MartialArts)) * (1.0 - stats.get(Stat::DualWield) / 100.0);
    let main_damage = stats.require(Stat::Dmg1, OPERATION)?;
    let sub_damage = stats.require(Stat::Dmg2, OPERATION)?;
    let strength = stats.require(Stat::Str, OPERATION)?;
    let vitality = enemy.stats.require(Stat::Vit, OPERATION)?;
    let fstr_main = get_fstr(main_damage, strength, vitality);
    let fstr_sub = get_fstr(sub_damage, strength, vitality);
    let attack1 = stats.require(Stat::Attack1, OPERATION)?;
    let attack2 = stats.require(Stat::Attack2, OPERATION)?;
    let accuracy1 = stats.require(Stat::Accuracy1, OPERATION)?;
    let accuracy2 = stats.require(Stat::Accuracy2, OPERATION)?;
    let sub_skill = if main_skill == SkillType::HandToHand { SkillType::HandToHand } else { ctx.state.skill(sub) };
    let attack2 = if main_skill == SkillType::HandToHand { attack1 } else { attack2 };
    let main_cap = if ONE_HANDED_SKILLS.contains(&main_skill) || main_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
    let sub_cap = if sub_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
    let hit_rate11 = get_hit_rate(accuracy1, enemy_evasion, main_cap);
    let hit_rate12 = get_hit_rate(accuracy1, enemy_evasion, main_cap);
    let hit_rate21 = if dual_wield { get_hit_rate(accuracy2, enemy_evasion, sub_cap) } else { 0.0 };
    let hit_rate22 = if dual_wield { get_hit_rate(accuracy2, enemy_evasion, sub_cap) } else { 0.0 };
    let hit_rates = [[hit_rate11, hit_rate21], [hit_rate12, hit_rate22]];
    let daken = upper_rate(stats.get(Stat::Daken));
    let ammo_damage = stats.get(Stat::AmmoDmg);
    let fstr_ammo = get_fstr2(ammo_damage, strength, vitality);
    let ammo_delay = stats.get(Stat::AmmoDelay);
    let ranged_accuracy = stats.require(Stat::RangedAccuracy, OPERATION)? + 100.0 * flag(stats.get(Stat::Daken) > 0.0);
    let ranged_attack = stats.get(Stat::RangedAttack);
    let ammo_skill = ctx.state.skill(ctx.catalog.piece(player.gearset[Slot::Ammo.index()]));
    let ranged_cap = if player.abilities.enabled(Ability::Sharpshot) { 0.99 } else { 0.95 };
    let hit_rate_ranged = get_hit_rate(ranged_accuracy, enemy_evasion, ranged_cap);
    let kick_damage = stats.require(Stat::KickDmg, OPERATION)?;
    let fstr_kick = get_fstr(kick_damage, strength, enemy.stats.get(Stat::Vit));
    let kickattacks = upper_rate(stats.get(Stat::KickAttacks));
    let zanshin = stats.get(Stat::Zanshin) / 100.0;
    let zanshin = if zanshin > 1.0 { 1.0 } else { zanshin };
    let zanhasso = stats.get(Stat::Zanhasso) / 100.0;
    let zanshin_hit_rate = get_hit_rate(accuracy1 + 34.0, enemy_evasion, 0.95);
    let zanshin_oa2 = stats.get(Stat::ZanshinOa2) / 100.0;
    let enhancing_skill = player.abilities.enhancing_skill;
    let enspell_active = player.abilities.enabled(Ability::EnSpell)
        || player.abilities.enabled(Ability::EndarkIi) || player.abilities.enabled(Ability::EnlightIi);
    let mut main_enspell = 0.0;
    let mut sub_enspell = 0.0;
    let mut enspell_percent_main = 0.0;
    let mut enspell_percent_sub = 0.0;
    let mut enspell_damage_main = 0.0;
    let mut enspell_damage_sub = 0.0;
    let mut dayweather = 0.0;
    let mut elemental_bonus = 0.0;
    let mut magic_crit_rate2 = 0.0;
    if player.abilities.enabled(Ability::EnSpell) {
        enspell_percent_main = stats.get(Stat::EnSpellDamagePctMain) + stats.get(Stat::EnSpellDamagePct);
        enspell_percent_sub = stats.get(Stat::EnSpellDamagePctSub) + stats.get(Stat::EnSpellDamagePct);
        enspell_damage_main = stats.get(Stat::EnSpellDamageMain) + stats.get(Stat::EnSpellDamage);
        enspell_damage_sub = stats.get(Stat::EnSpellDamageSub) + stats.get(Stat::EnSpellDamage);
        magic_crit_rate2 = stats.get(Stat::MagicCritRateIi) / 100.0;
        let waist = ctx.catalog.piece(player.gearset[Slot::Waist.index()]);
        if player.abilities.storm_name.as_deref() != Some("None") && waist.name.as_ref() == "Hachirin-no-Obi" {
            if player.abilities.storm_name.is_none() {
                return Err(CalcError::legacy(OPERATION, "Storm spell", "TypeError", "argument of type 'bool' is not iterable"));
            }
            dayweather = if player.abilities.storm_contains_ii { 0.25 } else { 0.10 };
        }
        let mut elemental_max = stats.get(Stat::EarthElementalBonus);
        for stat in [Stat::WaterElementalBonus, Stat::WindElementalBonus, Stat::FireElementalBonus,
            Stat::IceElementalBonus, Stat::ThunderElementalBonus] {
            elemental_max = python_max(elemental_max, stats.get(stat));
        }
        elemental_bonus = elemental_max / 100.0 + stats.get(Stat::ElementalBonus) / 100.0;
        main_enspell = get_enspell_damage(enhancing_skill, enspell_percent_main, enspell_damage_main)?
            * (1.0 + dayweather) * (1.0 + elemental_bonus)
            * (1.0 + 0.25 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < magic_crit_rate2));
        // Deliberately main-hand percentage on the sampled offhand damage.
        // Both uniforms are also consumed by the analytical setup.
        sub_enspell = get_enspell_damage(enhancing_skill, enspell_percent_main, enspell_damage_sub)?
            * (1.0 + dayweather) * (1.0 + elemental_bonus)
            * (1.0 + 0.25 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < magic_crit_rate2));
    } else if player.abilities.enabled(Ability::EnlightIi) {
        main_enspell = if enhancing_skill <= 500.0 {
            2.0 * ((enhancing_skill + 85.0) / 13.0) + ((enhancing_skill + 85.0) / 26.0)
        } else {
            2.0 * ((enhancing_skill + 400.0) / 20.0) + ((enhancing_skill + 400.0) / 40.0)
        };
        main_enspell = python_int(main_enspell * 0.8, OPERATION, "Enlight II")? + 20.0;
    } else if player.abilities.enabled(Ability::EndarkIi) {
        main_enspell = python_int(((enhancing_skill + 20.0) / 13.0 + 5.0) * 2.5 * 0.8, OPERATION, "Endark II")? + 20.0;
    }
    let aftermath = python_int(player.abilities.aftermath(), OPERATION, "Aftermath")?;
    let [empyrean_bonus, relic_bonus, prime_bonus, dragon_fangs_bonus] = weapon_damage_bonuses(main, aftermath)?;
    Ok(RoundSetup {
        dual_wield, main_skill, sub_skill, ammo_skill, qa, ta, da, oa, fua, ta_damage, da_damage,
        pdl_gear, pdl_trait, crit_rate, crit_damage, stp, mdelay, main_damage, sub_damage,
        fstr_main, fstr_sub, attack1, attack2, hit_rates, enemy_defense, daken,
        ammo_damage, fstr_ammo, ammo_delay, ranged_attack, hit_rate_ranged, kick_damage,
        fstr_kick, kickattacks, zanshin, zanhasso, zanshin_hit_rate, zanshin_oa2,
        enspell_active, main_enspell, sub_enspell, enhancing_skill, enspell_percent_main,
        enspell_percent_sub, enspell_damage_main, enspell_damage_sub, dayweather,
        elemental_bonus, magic_crit_rate2, aftermath, empyrean_bonus, relic_bonus,
        prime_bonus, dragon_fangs_bonus,
        empyrean: EMPYREAN_WEAPONS.contains(&main.name.as_ref()),
        relic30: RELIC_WEAPONS_30.contains(&main.name.as_ref()),
        relic25: RELIC_WEAPONS_25.contains(&main.name.as_ref()),
        relic20: RELIC_WEAPONS_20.contains(&main.name.as_ref()),
        prime3: PRIME_WEAPONS_3.contains(&main.name2.as_ref()),
        prime2: PRIME_WEAPONS_2.contains(&main.name2.as_ref()),
        verethragna: main.name.as_ref() == "Verethragna",
    })
}

#[inline]
fn average_hit(damage: f64, fstr: f64, pdif: f64, crit_rate: f64, crit_damage: f64) -> Result<f64, CalcError> {
    get_avg_phys_damage(damage, fstr, 0.0, pdif, 1.0, crit_rate, crit_damage,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
}

pub fn evaluate_round<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    starting_tp: f64,
    threshold: f64,
    metric: &Metric,
    ctx: &mut EvaluationContext<R>,
) -> Result<ActionResult, CalcError> {
    let setup = prepare_round(player, enemy, starting_tp, ctx)?;
    let stats = &player.stats;
    let tracing = ctx.calculation_trace.is_some();
    if tracing {
        ctx.record_step(Section::Damage, "Main-hand fSTR", &[MELEE_FSTR],
            &[("damage", setup.main_damage), ("strength", stats.get(Stat::Str)),
                ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_main)]);
        if setup.dual_wield {
            ctx.record_step(Section::Damage,
                if setup.main_skill == SkillType::HandToHand { "Second-fist fSTR" } else { "Offhand fSTR" },
                &[MELEE_FSTR],
                &[("damage", setup.sub_damage), ("strength", stats.get(Stat::Str)),
                    ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_sub)]);
        }
        let (tauret, karambit) = {
            let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
            (flag(main.name.as_ref() == "Tauret"), flag(main.name.as_ref() == "Karambit"))
        };
        ctx.record_step(Section::Damage, "Critical hit rate", &[
            ("Crit rate = min(1, Crit% / 100 + DEXcrit(DEX − AGI) + 0.5 × (1 − TP / 3000) × I(Tauret))",
                "min(1, {base_crit} / 100 + DEXcrit({dex} − {enemy_agi}) + 0.5 × (1 − {starting_tp} / 3000) × {tauret}) = {crit_rate}"),
            ("Crit dmg", "{crit_damage}"),
        ], &[("base_crit", stats.get(Stat::CritRate)), ("dex", stats.get(Stat::Dex)),
            ("enemy_agi", enemy.stats.get(Stat::Agi)), ("starting_tp", starting_tp), ("tauret", tauret),
            ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage)]);
        ctx.record_step(Section::Tp, "Store TP", &[
            ("Store TP = STP / 100 + 0.5 × Crit rate × I(Karambit)", "{store_tp} / 100 + 0.5 × {crit_rate} × {karambit} = {stp}"),
        ], &[("store_tp", stats.get(Stat::StoreTp)), ("crit_rate", setup.crit_rate),
            ("karambit", karambit), ("stp", setup.stp)]);
    }
    let striking = player.abilities.enabled(Ability::StrikingFlourish);
    let ternary = player.abilities.enabled(Ability::TernaryFlourish);
    let climactic = player.abilities.enabled(Ability::ClimacticFlourish);
    let sneak = player.abilities.enabled(Ability::SneakAttack);
    let trick = player.abilities.enabled(Ability::TrickAttack);
    // The source constructs the matrix BEFORE forcing these local first-hit
    // rates. SA/TA consequently do not change get_ma_rate3's expected hits.
    let hit_rate11 = if sneak || trick { 1.0 } else { setup.hit_rates[0][0] };
    let hits = get_ma_rate3(player.main_job, 1.0, setup.qa, setup.ta, setup.da,
        &setup.oa, &setup.fua, setup.dual_wield, &setup.hit_rates,
        setup.hit_rate_ranged, setup.daken, setup.kickattacks, setup.zanshin,
        setup.zanhasso, setup.zanshin_hit_rate, setup.zanshin_oa2,
        striking, ternary, true);
    if tracing {
        let rate_lines: &'static [(&'static str, &'static str)] = if setup.dual_wield {
            &[MAIN_HIT_RATE, ("Sub hit rate = clamp((75 + 0.5 × (Acc2 − max(1, Eva))) / 100, 0.2, Sub hit cap)",
                "clamp((75 + 0.5 × ({accuracy2} − max(1, {evasion}))) / 100, 0.2, {sub_cap}) = {sub_first}")]
        } else {
            &[MAIN_HIT_RATE, ("Sub hit rate", "{sub_first}")]
        };
        ctx.record_step(Section::Hits, "Melee hit rates", rate_lines,
            &[("evasion", enemy.stats.get(Stat::Evasion)), ("accuracy1", stats.get(Stat::Accuracy1)),
                ("main_cap", if ONE_HANDED_SKILLS.contains(&setup.main_skill) || setup.main_skill == SkillType::HandToHand { 0.99 } else { 0.95 }),
                ("main_first", setup.hit_rates[0][0]), ("accuracy2", stats.get(Stat::Accuracy2)),
                ("sub_cap", if setup.sub_skill == SkillType::HandToHand { 0.99 } else { 0.95 }),
                ("sub_first", setup.hit_rates[0][1])]);
        let main_hit_lines: &'static [(&'static str, &'static str)] = if striking {
            &[("E[Main hits] = Hit rate + MA(8 − (Hit rate + Sub hit rate); 0, 0, 1, OA3, OA2) × Hit rate",
                "{main_first} + MA(8 − ({main_first} + {sub_first}); 0, 0, 1, {oa3}, {oa2}) × {main_later} = {hits}")]
        } else if ternary {
            &[("E[Main hits] = Hit rate + MA(8 − (Hit rate + Sub hit rate); 0, 1, 0, OA3, OA2) × Hit rate",
                "{main_first} + MA(8 − ({main_first} + {sub_first}); 0, 1, 0, {oa3}, {oa2}) × {main_later} = {hits}")]
        } else {
            &[("E[Main hits] = Hit rate + MA(8 − (Hit rate + Sub hit rate); QA, TA, DA, OA3, OA2) × Hit rate",
                "{main_first} + MA(8 − ({main_first} + {sub_first}); {qa}, {ta}, {da}, {oa3}, {oa2}) × {main_later} = {hits}")]
        };
        ctx.record_step(Section::Hits,
            if setup.main_skill == SkillType::HandToHand { "Expected first-fist hits" } else { "Expected main-hand hits" },
            main_hit_lines,
            &[("main_first", setup.hit_rates[0][0]), ("sub_first", setup.hit_rates[0][1]),
                ("main_later", setup.hit_rates[1][0]), ("qa", setup.qa), ("ta", setup.ta), ("da", setup.da),
                ("oa3", setup.oa[0]), ("oa2", setup.oa[1]), ("hits", hits.main)]);
        if setup.dual_wield {
            ctx.record_step(Section::Hits,
                if setup.main_skill == SkillType::HandToHand { "Expected second-fist hits" } else { "Expected offhand hits" },
                &[("E[Sub hits] = Sub hit rate + MA(8 − (E[Main hits] + Sub hit rate); QA, TA, DA, OA8, …, OA2) × Sub hit rate",
                    "{first} + MA(8 − ({main} + {first}); {qa}, {ta}, {da}, {oa8}, {oa7}, {oa6}, {oa5}, {oa4}, {oa3}, {oa2}) × {later} = {hits}")],
                &[("first", setup.hit_rates[0][1]), ("main", hits.main), ("later", setup.hit_rates[1][1]),
                    ("hits", hits.sub), ("qa", setup.qa), ("ta", setup.ta), ("da", setup.da),
                    ("oa8", setup.oa[2]), ("oa7", setup.oa[3]), ("oa6", setup.oa[4]), ("oa5", setup.oa[5]),
                    ("oa4", setup.oa[6]), ("oa3", setup.oa[7]), ("oa2", setup.oa[8])]);
        }
        if setup.kickattacks != 0.0 || hits.kick != 0.0 {
            ctx.record_step(Section::Hits, "Expected kicks",
                &[("E[Kicks] = min(max(0, 8 − (E[Main hits] + E[Sub hits])), 1) × Kick rate × Hit rate",
                    "min(max(0, 8 − ({main} + {sub})), 1) × {rate} × {hit_rate} = {hits}")],
                &[("main", hits.main), ("sub", hits.sub), ("rate", setup.kickattacks),
                    ("hit_rate", setup.hit_rates[1][0]), ("hits", hits.kick)]);
        }
        if setup.daken != 0.0 || hits.daken != 0.0 {
            ctx.record_step(Section::Hits, "Expected Daken hits", &[
                ("Ranged hit rate = clamp((75 + 0.5 × (RAcc + 100 × I(Daken > 0) − max(1, Eva))) / 100, 0.2, Hit cap)",
                    "clamp((75 + 0.5 × ({accuracy} + 100 × I({daken} > 0) − max(1, {evasion}))) / 100, 0.2, {cap}) = {hit_rate}"),
                ("E[Daken hits] = min(max(0, 8 − (E[Main hits] + E[Sub hits]) − Kick rate), 1) × Daken × Ranged hit rate",
                    "min(max(0, 8 − ({main} + {sub}) − {kick_rate}), 1) × {daken} × {hit_rate} = {hits}"),
            ], &[("accuracy", stats.get(Stat::RangedAccuracy)), ("evasion", enemy.stats.get(Stat::Evasion)),
                ("cap", if player.abilities.enabled(Ability::Sharpshot) { 0.99 } else { 0.95 }),
                ("hit_rate", setup.hit_rate_ranged), ("main", hits.main), ("sub", hits.sub),
                ("kick_rate", setup.kickattacks), ("daken", setup.daken), ("hits", hits.daken)]);
        }
        if setup.zanshin != 0.0 || setup.zanhasso != 0.0 || hits.zanshin != 0.0 {
            const ZANSHIN_HIT_RATE: (&str, &str) = (
                "Zanshin hit rate = clamp((75 + 0.5 × (Acc + 34 − max(1, Eva))) / 100, 0.2, 0.95)",
                "clamp((75 + 0.5 × ({accuracy} + 34 − max(1, {evasion}))) / 100, 0.2, 0.95) = {zanshin_hit_rate}");
            let lines: &'static [(&'static str, &'static str)] = if striking {
                &[ZANSHIN_HIT_RATE, ("E[Zanshin hits] = (1 − 0)(1 − 0)(1 − 1)(1 − OA3)(1 − OA2) × ((1 − Hit rate) × Zanshin + I(SAM) × Hit rate × Zanhasso) × (1 + Zanshin OA2) × Zanshin hit rate",
                    "(1 − 0)(1 − 0)(1 − 1)(1 − {oa3})(1 − {oa2}) × ((1 − {main_hit_rate}) × {zanshin} + {sam} × {main_hit_rate} × {zanhasso}) × (1 + {oa2_rate}) × {zanshin_hit_rate} = {hits}")]
            } else if ternary {
                &[ZANSHIN_HIT_RATE, ("E[Zanshin hits] = (1 − 0)(1 − 1)(1 − 0)(1 − OA3)(1 − OA2) × ((1 − Hit rate) × Zanshin + I(SAM) × Hit rate × Zanhasso) × (1 + Zanshin OA2) × Zanshin hit rate",
                    "(1 − 0)(1 − 1)(1 − 0)(1 − {oa3})(1 − {oa2}) × ((1 − {main_hit_rate}) × {zanshin} + {sam} × {main_hit_rate} × {zanhasso}) × (1 + {oa2_rate}) × {zanshin_hit_rate} = {hits}")]
            } else {
                &[ZANSHIN_HIT_RATE, ("E[Zanshin hits] = (1 − QA)(1 − TA)(1 − DA)(1 − OA3)(1 − OA2) × ((1 − Hit rate) × Zanshin + I(SAM) × Hit rate × Zanhasso) × (1 + Zanshin OA2) × Zanshin hit rate",
                    "(1 − {qa})(1 − {ta})(1 − {da})(1 − {oa3})(1 − {oa2}) × ((1 − {main_hit_rate}) × {zanshin} + {sam} × {main_hit_rate} × {zanhasso}) × (1 + {oa2_rate}) × {zanshin_hit_rate} = {hits}")]
            };
            ctx.record_step(Section::Hits, "Expected Zanshin hits", lines,
                &[("qa", setup.qa), ("ta", setup.ta), ("da", setup.da),
                    ("oa3", setup.oa[0]), ("oa2", setup.oa[1]), ("accuracy", stats.get(Stat::Accuracy1)),
                    ("evasion", enemy.stats.get(Stat::Evasion)), ("zanshin_hit_rate", setup.zanshin_hit_rate),
                    ("main_hit_rate", setup.hit_rates[1][0]), ("zanshin", setup.zanshin),
                    ("sam", flag(player.main_job == Job::Sam)), ("zanhasso", setup.zanhasso),
                    ("oa2_rate", setup.zanshin_oa2), ("hits", hits.zanshin)]);
        }
        ctx.record_step(Section::Hits, "Expected hits",
            &[("Main, Sub, Kick, Daken, Zanshin hits", "{main}, {sub}, {kick}, {daken}, {zanshin}")],
            &[("main", hits.main), ("sub", hits.sub), ("kick", hits.kick), ("daken", hits.daken), ("zanshin", hits.zanshin)]);
    }
    let mut physical_damage = 0.0;
    let mut magical_damage = 0.0;
    // Unlike the sampler, analytical Enlight/Endark contribute no magic damage.
    if player.abilities.enabled(Ability::EnSpell) {
        let main_enspell = get_enspell_damage(setup.enhancing_skill, setup.enspell_percent_main, setup.enspell_damage_main)?;
        magical_damage += hits.main * main_enspell;
        let sub_enspell = get_enspell_damage(setup.enhancing_skill, setup.enspell_percent_sub, setup.enspell_damage_sub)?;
        magical_damage += hits.sub * sub_enspell;
        let kick_enspell = get_enspell_damage(setup.enhancing_skill, setup.enspell_percent_main, setup.enspell_damage_main)?;
        magical_damage += hits.kick * kick_enspell;
        let zanshin_enspell = get_enspell_damage(setup.enhancing_skill, setup.enspell_percent_main, setup.enspell_damage_main)?;
        magical_damage += hits.zanshin * zanshin_enspell;
        if tracing {
            let lines: &'static [(&'static str, &'static str)] = if setup.enhancing_skill < 600.0 {
                &[("Main EnSpell = (trunc((Skill − 223) / 7.70) + 29 + Main flat) × (1 + Main % / 100)",
                    "(trunc(({skill} − 223) / 7.70) + 29 + {main_flat}) × (1 + {main_percent} / 100) = {main}"),
                    ("Sub EnSpell = (trunc((Skill − 223) / 7.70) + 29 + Sub flat) × (1 + Sub % / 100)",
                    "(trunc(({skill} − 223) / 7.70) + 29 + {sub_flat}) × (1 + {sub_percent} / 100) = {sub}")]
            } else {
                &[("Main EnSpell = (trunc((Skill − 202.5) / 8.05) + 29 + Main flat) × (1 + Main % / 100)",
                    "(trunc(({skill} − 202.5) / 8.05) + 29 + {main_flat}) × (1 + {main_percent} / 100) = {main}"),
                    ("Sub EnSpell = (trunc((Skill − 202.5) / 8.05) + 29 + Sub flat) × (1 + Sub % / 100)",
                    "(trunc(({skill} − 202.5) / 8.05) + 29 + {sub_flat}) × (1 + {sub_percent} / 100) = {sub}")]
            };
            ctx.record_step(Section::Damage, "EnSpell damage per hit", lines,
                &[("skill", setup.enhancing_skill), ("main_percent", setup.enspell_percent_main),
                    ("main_flat", setup.enspell_damage_main), ("main", main_enspell),
                    ("sub_percent", setup.enspell_percent_sub), ("sub_flat", setup.enspell_damage_sub),
                    ("sub", sub_enspell)]);
            ctx.record_step(Section::Damage, "EnSpell subtotal",
                &[("Subtotal = E[Main hits] × Main EnSpell + E[Sub hits] × Sub EnSpell + E[Kicks] × Main EnSpell + E[Zanshin hits] × Main EnSpell",
                    "{main_hits} × {main} + {sub_hits} × {sub} + {kick_hits} × {kick} + {zanshin_hits} × {zanshin} = {subtotal}")],
                &[("main_hits", hits.main), ("main", main_enspell), ("sub_hits", hits.sub),
                    ("sub", sub_enspell), ("kick_hits", hits.kick), ("kick", kick_enspell),
                    ("zanshin_hits", hits.zanshin), ("zanshin", zanshin_enspell), ("subtotal", magical_damage)]);
        }
        let enspell_subtotal = magical_damage;
        magical_damage *= 1.0 + setup.dayweather;
        magical_damage *= 1.0 + setup.elemental_bonus;
        magical_damage *= 1.0 + 0.25 * setup.magic_crit_rate2;
        if tracing {
            ctx.record_step(Section::Damage, "EnSpell damage",
                &[("EnSpell damage = Subtotal × (1 + Day/weather) × (1 + Elemental bonus) × (1 + 0.25 × Magic crit rate II)",
                    "{subtotal} × (1 + {dayweather}) × (1 + {elemental_bonus}) × (1 + 0.25 × {magic_crit_rate}) = {magic}")],
                &[("subtotal", enspell_subtotal), ("dayweather", setup.dayweather), ("elemental_bonus", setup.elemental_bonus),
                    ("magic_crit_rate", setup.magic_crit_rate2), ("magic", magical_damage)]);
        }
    }
    let mut tp_return = 0.0;
    let melee_tp = get_tp(hits.main + hits.sub + hits.kick, setup.hit_delay(), setup.stp, false)?;
    tp_return += melee_tp;
    let zanshin_tp = get_tp(hits.zanshin, setup.hit_delay(), setup.stp, player.main_job == Job::Sam)?;
    tp_return += zanshin_tp;
    let daken_tp = get_tp(hits.daken, setup.ammo_delay, setup.stp, false)?;
    tp_return += daken_tp;
    tp_return = if tp_return < 0.0 { 0.0 } else { tp_return };
    if tracing {
        let delay_lines: &'static [(&'static str, &'static str)] = if setup.main_skill == SkillType::HandToHand {
            &[("TP delay = ((Delay1 + Delay2) / 2 − Martial Arts) × (1 − DW / 100) / 2",
                "(({delay1} + {delay2}) / 2 − {martial_arts}) × (1 − {dual_wield} / 100) / 2 = {hit_delay}")]
        } else {
            &[("TP delay = ((Delay1 + Delay2) / 2 − Martial Arts) × (1 − DW / 100)",
                "(({delay1} + {delay2}) / 2 − {martial_arts}) × (1 − {dual_wield} / 100) = {hit_delay}")]
        };
        ctx.record_step(Section::Tp, "TP delay", delay_lines,
            &[("delay1", stats.get(Stat::Delay1)), ("delay2", stats.get(Stat::Delay2)),
                ("martial_arts", stats.get(Stat::MartialArts)), ("dual_wield", stats.get(Stat::DualWield)),
                ("hit_delay", setup.hit_delay())]);
        record_tp_gain(ctx, "Melee and kick TP", hits.main + hits.sub + hits.kick,
            setup.hit_delay(), setup.stp, false, melee_tp);
        record_tp_gain(ctx, if player.main_job == Job::Sam { "SAM Zanshin TP" } else { "Non-SAM Zanshin TP" },
            hits.zanshin, setup.hit_delay(), setup.stp, player.main_job == Job::Sam, zanshin_tp);
        ctx.record_step(Section::Tp, "Daken TP delay", &[("Daken delay = Ammo delay", "{ammo_delay}")],
            &[("ammo_delay", setup.ammo_delay)]);
        record_tp_gain(ctx, "Daken TP", hits.daken, setup.ammo_delay, setup.stp, false, daken_tp);
        ctx.record_step(Section::Tp, "Attack TP",
            &[("Attack TP = max(0, Melee TP + Zanshin TP + Daken TP)",
                "max(0, {melee_tp} + {zanshin_tp} + {daken_tp}) = {attack_tp}")],
            &[("melee_tp", melee_tp), ("zanshin_tp", zanshin_tp), ("daken_tp", daken_tp), ("attack_tp", tp_return)]);
    }
    let seconds_per_round = python_max(0.0, get_delay_timing(
        stats.require(Stat::Delay1, OPERATION)?,
        if setup.dual_wield && setup.main_skill != SkillType::HandToHand { stats.require(Stat::Delay2, OPERATION)? } else { 0.0 },
        stats.get(Stat::DualWield) / 100.0, stats.get(Stat::MartialArts),
        stats.get(Stat::MagicHaste), stats.get(Stat::JaHaste), stats.get(Stat::GearHaste)));
    let (gokotai, vajra) = {
        let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
        (main.name.as_ref() == "Gokotai", main.name.as_ref() == "Vajra")
    };
    let regain = stats.get(Stat::DualWield) * flag(gokotai) + stats.get(Stat::Regain);
    let regain_tp = (seconds_per_round / 3.0) * regain;
    let attack_tp = tp_return;
    tp_return += regain_tp;
    let time_to_ws = if tp_return == 0.0 { 9999.0 } else { seconds_per_round * ((threshold - starting_tp) / tp_return) };
    if tracing {
        let timing_delay2 = if setup.dual_wield && setup.main_skill != SkillType::HandToHand { stats.get(Stat::Delay2) } else { 0.0 };
        ctx.record_step(Section::Time, "Haste and seconds per round", &[
            ("Haste = min(Gear haste, 256/1024) + min(Magic haste, 448/1024) + min(JA haste, 256/1024)",
                "min({gear_haste}, 256/1024) + min({magic_haste}, 448/1024) + min({ja_haste}, 256/1024)"),
            ("Seconds = max(0, max((Delay1 + Delay2 − Martial Arts) × (1 − DW / 100) × (1 − Haste), 0.2 × (Delay1 + Delay2)) / 60)",
                "max(0, max(({delay1} + {delay2} − {martial_arts}) × (1 − {dual_wield} / 100) × (1 − Haste), 0.2 × ({delay1} + {delay2})) / 60) = {seconds}"),
        ], &[("delay1", stats.get(Stat::Delay1)), ("delay2", timing_delay2),
            ("gear_haste", stats.get(Stat::GearHaste)), ("magic_haste", stats.get(Stat::MagicHaste)),
            ("ja_haste", stats.get(Stat::JaHaste)), ("martial_arts", stats.get(Stat::MartialArts)),
            ("dual_wield", stats.get(Stat::DualWield)), ("seconds", seconds_per_round)]);
        ctx.record_step(Section::Tp, "Regain and total TP", &[
            ("Regain rate = Regain + DW × I(Gokotai)", "{regain_stat} + {dual_wield} × {gokotai} = {regain}"),
            ("Regain TP = Seconds / 3 × Regain rate", "{seconds} / 3 × {regain} = {regain_tp}"),
            ("TP = Attack TP + Regain TP", "{attack_tp} + {regain_tp} = {tp}"),
        ], &[("regain_stat", stats.get(Stat::Regain)), ("dual_wield", stats.get(Stat::DualWield)),
            ("gokotai", flag(gokotai)), ("regain", regain), ("seconds", seconds_per_round),
            ("regain_tp", regain_tp), ("attack_tp", attack_tp), ("tp", tp_return)]);
        if tp_return == 0.0 {
            ctx.record_step(Section::Time, "Rounds and time to WS",
                &[("Time to WS = 9999 if TP = 0", "{time}")], &[("time", time_to_ws)]);
        } else {
            let rounds_to_ws = (threshold - starting_tp) / tp_return;
            ctx.record_step(Section::Time, "Rounds and time to WS", &[
                ("Rounds = (Threshold − Starting TP) / TP", "({threshold} − {starting_tp}) / {tp} = {rounds}"),
                ("Time to WS = Seconds × Rounds", "{seconds} × {rounds} = {time}"),
            ], &[("threshold", threshold), ("starting_tp", starting_tp), ("tp", tp_return),
                ("rounds", rounds_to_ws), ("seconds", seconds_per_round), ("time", time_to_ws)]);
        }
    }

    let main_pdif = get_avg_pdif_melee(setup.attack1, setup.main_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let main_base_damage = average_hit(setup.main_damage, setup.fstr_main, main_pdif, setup.crit_rate, setup.crit_damage)?;
    let main_empyrean_multiplier = if setup.verethragna { 1.0 } else { setup.empyrean_bonus };
    let main_hit_damage = main_base_damage * main_empyrean_multiplier;
    let main_expected_damage = hits.main * main_hit_damage;
    physical_damage += main_expected_damage;
    if tracing {
        ctx.record_step(Section::Damage, "Weapon proc multipliers", &[
            ("Empyrean = 1 + 2 × AM(Aftermath) × I(Empyrean weapon)", "1 + 2 × AM({aftermath}) × {empyrean_weapon} = {empyrean}"),
            ("Relic", "{relic}"),
            ("Prime", "{prime}"),
            ("Dragon Fangs", "{dragon_fangs}"),
        ], &[("empyrean_weapon", flag(setup.empyrean)), ("aftermath", setup.aftermath),
            ("empyrean", setup.empyrean_bonus), ("relic", setup.relic_bonus),
            ("prime", setup.prime_bonus), ("dragon_fangs", setup.dragon_fangs_bonus)]);
        ctx.record_step(Section::Damage, "Main-hand pDIF", &[MELEE_W, MELEE_PDIF],
            &[("attack", setup.attack1), ("defense", setup.enemy_defense), ("pdl_trait", setup.pdl_trait),
                ("pdl_gear", setup.pdl_gear), ("crit_rate", setup.crit_rate), ("pdif", main_pdif)]);
        ctx.record_step(Section::Damage, "Main-hand damage", &[
            AVG_HIT,
            ("Main hit = Avg hit × Empyrean", "{base} × {empyrean} = {hit}"),
            ("Main damage = E[Main hits] × Main hit", "{hits} × {hit} = {total}"),
        ], &[("damage", setup.main_damage), ("fstr", setup.fstr_main), ("pdif", main_pdif),
            ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage), ("base", main_base_damage),
            ("empyrean", main_empyrean_multiplier), ("hit", main_hit_damage),
            ("hits", hits.main), ("total", main_expected_damage)]);
    }
    let sneak_bonus = (stats.require(Stat::Dex, OPERATION)? * (1.0 + stats.get(Stat::SneakAttackBonus) / 100.0))
        * flag(sneak) * flag(player.main_job == Job::Thf);
    let trick_bonus = (stats.require(Stat::Agi, OPERATION)? * (1.0 + stats.get(Stat::TrickAttackBonus) / 100.0))
        * flag(trick) * flag(player.main_job == Job::Thf);
    let climactic_bonus = (0.5 * stats.require(Stat::Chr, OPERATION)? * (1.0 + stats.get(Stat::FlourishChrPct) / 100.0)) * flag(climactic);
    let striking_bonus = (1.0 * stats.require(Stat::Chr, OPERATION)? * (1.0 + stats.get(Stat::FlourishChrPct) / 100.0)) * flag(striking);
    let ternary_bonus = (1.0 * stats.require(Stat::Chr, OPERATION)? * (1.0 + stats.get(Stat::FlourishChrPct) / 100.0)) * flag(ternary);
    let climactic_crit_damage = stats.get(Stat::ClimacticCritDamage) / 100.0 * flag(climactic);
    let striking_crit_rate = stats.get(Stat::StrikingCritRate) / 100.0 * flag(striking);
    let vajra_crit_damage = 0.3 * flag(vajra && (sneak || trick));
    let first_crit_rate = if sneak || trick || climactic { 1.0 } else { setup.crit_rate + striking_crit_rate * flag(setup.crit_rate > 0.0) };
    let adjusted_crit_damage = (setup.crit_damage + vajra_crit_damage) * (1.0 + climactic_crit_damage);
    let first_pdif = get_avg_pdif_melee(setup.attack1, setup.main_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, first_crit_rate)?;
    let mut first_damage = get_avg_phys_damage(setup.main_damage, setup.fstr_main, 0.0, first_pdif,
        1.0, first_crit_rate, adjusted_crit_damage, 0.0, 0.0, 0.0,
        sneak_bonus, trick_bonus, climactic_bonus, striking_bonus, ternary_bonus)?;
    let first_base_damage = first_damage;
    first_damage *= setup.relic_bonus;
    first_damage *= setup.empyrean_bonus;
    first_damage *= setup.prime_bonus;
    let first_hit_correction = first_damage * hit_rate11 - main_hit_damage * hit_rate11;
    physical_damage += first_hit_correction;
    if tracing {
        ctx.record_step(Section::Damage, "Sneak/Trick Attack bonuses", &[
            ("Sneak = DEX × (1 + Sneak bonus / 100) × I(Sneak Attack) × I(THF)",
                "{dex} × (1 + {sneak_percent} / 100) × {sneak_active} × {thf} = {sneak}"),
            ("Trick = AGI × (1 + Trick bonus / 100) × I(Trick Attack) × I(THF)",
                "{agi} × (1 + {trick_percent} / 100) × {trick_active} × {thf} = {trick}"),
        ], &[("dex", stats.get(Stat::Dex)), ("sneak_percent", stats.get(Stat::SneakAttackBonus)),
            ("sneak_active", flag(sneak)), ("thf", flag(player.main_job == Job::Thf)), ("sneak", sneak_bonus),
            ("agi", stats.get(Stat::Agi)), ("trick_percent", stats.get(Stat::TrickAttackBonus)),
            ("trick_active", flag(trick)), ("trick", trick_bonus)]);
        ctx.record_step(Section::Damage, "Flourish bonuses", &[
            ("Climactic = 0.5 × CHR × (1 + Flourish CHR / 100) × I(Climactic)",
                "0.5 × {chr} × (1 + {chr_percent} / 100) × {climactic_active} = {climactic}"),
            ("Striking = CHR × (1 + Flourish CHR / 100) × I(Striking)",
                "{chr} × (1 + {chr_percent} / 100) × {striking_active} = {striking}"),
            ("Ternary = CHR × (1 + Flourish CHR / 100) × I(Ternary)",
                "{chr} × (1 + {chr_percent} / 100) × {ternary_active} = {ternary}"),
        ], &[("chr_percent", stats.get(Stat::FlourishChrPct)), ("chr", stats.get(Stat::Chr)),
            ("climactic_active", flag(climactic)), ("climactic", climactic_bonus),
            ("striking_active", flag(striking)), ("striking", striking_bonus),
            ("ternary_active", flag(ternary)), ("ternary", ternary_bonus)]);
        const FIRST_CRIT_DMG: (&str, &str) = ("First crit dmg = (Crit dmg + Vajra) × (1 + Climactic crit dmg)",
            "({crit_damage} + {vajra}) × (1 + {climactic_crit}) = {adjusted_crit}");
        const FIRST_W: (&str, &str) = ("w = Atk / Def + min(First crit rate, 1)",
            "{attack} / {defense} + min({first_crit}, 1)");
        const FIRST_PDIF: (&str, &str) = (
            "First pDIF = (clamp((U(w) + L(w)) / 2, 0, (Skill cap + PDL trait) × (1 + PDL gear)) + min(First crit rate, 1)) × 1.025",
            "(clamp((U(w) + L(w)) / 2, 0, (Skill cap + {pdl_trait}) × (1 + {pdl_gear})) + min({first_crit}, 1)) × 1.025 = {pdif}");
        let first_crit_lines: &'static [(&'static str, &'static str)] = if sneak || trick || climactic {
            &[("First crit rate (Sneak/Trick/Climactic)", "{first_crit}"), FIRST_CRIT_DMG, FIRST_W, FIRST_PDIF]
        } else {
            &[("First crit rate = Crit rate + Striking crit × I(Crit rate > 0)",
                "{crit_rate} + {striking_crit} × I({crit_rate} > 0) = {first_crit}"), FIRST_CRIT_DMG, FIRST_W, FIRST_PDIF]
        };
        ctx.record_step(Section::Damage, "First-hit crit and pDIF", first_crit_lines,
            &[("crit_rate", setup.crit_rate), ("striking_crit", striking_crit_rate),
                ("first_crit", first_crit_rate), ("crit_damage", setup.crit_damage),
                ("vajra", vajra_crit_damage), ("climactic_crit", climactic_crit_damage),
                ("adjusted_crit", adjusted_crit_damage), ("attack", setup.attack1), ("defense", setup.enemy_defense),
                ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear), ("pdif", first_pdif)]);
        ctx.record_step(Section::Damage, "First-hit damage", &[
            ("First hit = trunc(D + fSTR + Sneak + Trick + Climactic + Striking + Ternary) × First pDIF × (1 + min(First crit rate, 1) × min(First crit dmg, 1))",
                "trunc({damage} + {fstr} + {sneak} + {trick} + {climactic} + {striking} + {ternary}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}"),
        ], &[("damage", setup.main_damage), ("fstr", setup.fstr_main), ("sneak", sneak_bonus),
            ("trick", trick_bonus), ("climactic", climactic_bonus), ("striking", striking_bonus),
            ("ternary", ternary_bonus), ("pdif", first_pdif), ("crit_rate", first_crit_rate),
            ("crit_damage", adjusted_crit_damage), ("hit", first_base_damage)]);
        ctx.record_step(Section::Damage, "First-hit correction", &[
            ("Modified first hit = First hit × Relic × Empyrean × Prime", "{base} × {relic} × {empyrean} × {prime} = {first}"),
            ("Correction = Modified first hit × First hit rate − Main hit × First hit rate",
                "{first} × {first_hit_rate} − {ordinary} × {first_hit_rate} = {correction}"),
            ("Physical subtotal", "{subtotal}"),
        ], &[("base", first_base_damage), ("relic", setup.relic_bonus), ("empyrean", setup.empyrean_bonus),
            ("prime", setup.prime_bonus), ("first", first_damage), ("first_hit_rate", hit_rate11),
            ("ordinary", main_hit_damage), ("correction", first_hit_correction), ("subtotal", physical_damage)]);
    }
    if striking {
        let striking_rate = setup.crit_rate + striking_crit_rate * flag(setup.crit_rate > 0.0);
        let striking_pdif = get_avg_pdif_melee(setup.attack1, setup.main_skill.into(), setup.pdl_trait,
            setup.pdl_gear, setup.enemy_defense, striking_rate)?;
        let striking_damage = get_avg_phys_damage(setup.main_damage, setup.fstr_main, 0.0, striking_pdif,
            1.0, striking_rate, setup.crit_damage, 0.0, 0.0, 0.0,
            sneak_bonus, trick_bonus, climactic_bonus, striking_bonus, ternary_bonus)?;
        let striking_correction = (striking_damage - main_hit_damage) * hit_rate11;
        physical_damage += striking_correction;
        if tracing {
            ctx.record_step(Section::Damage, "Striking Flourish correction", &[
                ("Striking hit = trunc(D + fSTR + Sneak + Trick + Climactic + Striking + Ternary) × Striking pDIF × (1 + min(Striking crit rate, 1) × min(Crit dmg, 1))",
                    "trunc({damage} + {fstr} + {sneak} + {trick} + {climactic} + {striking} + {ternary}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}"),
                ("Correction = (Striking hit − Main hit) × First hit rate", "({hit} − {ordinary}) × {first_hit_rate} = {correction}"),
                ("Physical subtotal", "{subtotal}"),
            ], &[("damage", setup.main_damage), ("fstr", setup.fstr_main), ("sneak", sneak_bonus),
                ("trick", trick_bonus), ("climactic", climactic_bonus), ("striking", striking_bonus),
                ("ternary", ternary_bonus), ("crit_rate", striking_rate), ("pdif", striking_pdif),
                ("crit_damage", setup.crit_damage), ("hit", striking_damage), ("ordinary", main_hit_damage),
                ("first_hit_rate", hit_rate11), ("correction", striking_correction), ("subtotal", physical_damage)]);
        }
    }
    let offhand_pdif = get_avg_pdif_melee(setup.attack2, setup.sub_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let offhand_damage = average_hit(setup.sub_damage, setup.fstr_sub, offhand_pdif, setup.crit_rate, setup.crit_damage)?;
    let offhand_expected_damage = offhand_damage * hits.sub;
    physical_damage += offhand_expected_damage;
    if tracing && setup.dual_wield {
        let h2h = setup.main_skill == SkillType::HandToHand;
        ctx.record_step(Section::Damage, if h2h { "Second-fist pDIF" } else { "Offhand pDIF" }, &[MELEE_W, MELEE_PDIF],
            &[("attack", setup.attack2), ("defense", setup.enemy_defense), ("pdl_trait", setup.pdl_trait),
                ("pdl_gear", setup.pdl_gear), ("crit_rate", setup.crit_rate), ("pdif", offhand_pdif)]);
        ctx.record_step(Section::Damage, if h2h { "Second-fist damage" } else { "Offhand damage" }, &[
            ("Sub hit = trunc(D + fSTR) × pDIF × (1 + min(Crit rate, 1) × min(Crit dmg, 1))",
                "trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}"),
            ("Sub damage = Sub hit × E[Sub hits]", "{hit} × {hits} = {total}"),
            ("Physical subtotal", "{subtotal}"),
        ], &[("damage", setup.sub_damage), ("fstr", setup.fstr_sub), ("pdif", offhand_pdif),
            ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage),
            ("hit", offhand_damage), ("hits", hits.sub), ("total", offhand_expected_damage),
            ("subtotal", physical_damage)]);
    }
    let melee_before_multiattack = physical_damage;
    physical_damage *= (1.0 + setup.da_damage * setup.da) * (1.0 + setup.ta_damage * setup.ta);
    let melee_damage = physical_damage;
    if tracing {
        ctx.record_step(Section::Damage, "DA/TA damage modifiers",
            &[("Melee damage = Physical subtotal × ((1 + DA dmg × DA) × (1 + TA dmg × TA))",
                "{before} × ((1 + {da_damage} × {da}) × (1 + {ta_damage} × {ta})) = {after}")],
            &[("before", melee_before_multiattack), ("da_damage", setup.da_damage), ("da", setup.da),
                ("ta_damage", setup.ta_damage), ("ta", setup.ta), ("after", melee_damage)]);
    }
    let kick_pdif = get_avg_pdif_melee(setup.attack1 + stats.get(Stat::KickAttacksAttack),
        setup.main_skill.into(), setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let mut kick_damage = average_hit(setup.kick_damage, setup.fstr_kick, kick_pdif, setup.crit_rate, setup.crit_damage)?;
    let kick_base_damage = kick_damage;
    kick_damage *= setup.dragon_fangs_bonus;
    let kick_expected_damage = kick_damage * hits.kick;
    physical_damage += kick_expected_damage;
    if tracing && (setup.kickattacks != 0.0 || hits.kick != 0.0) {
        ctx.record_step(Section::Damage, "Kick fSTR and pDIF", &[
            MELEE_FSTR,
            ("w = (Atk + Kick Atk) / Def + min(Crit rate, 1)", "({attack} + {kick_attack}) / {defense} + min({crit_rate}, 1)"),
            MELEE_PDIF,
        ], &[("damage", setup.kick_damage), ("strength", stats.get(Stat::Str)),
            ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_kick),
            ("attack", setup.attack1), ("kick_attack", stats.get(Stat::KickAttacksAttack)),
            ("defense", setup.enemy_defense), ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear),
            ("crit_rate", setup.crit_rate), ("pdif", kick_pdif)]);
        ctx.record_step(Section::Damage, "Kick damage", &[
            AVG_HIT,
            ("Kick hit = Avg hit × Dragon Fangs", "{base} × {dragon_fangs} = {hit}"),
            ("Kick damage = Kick hit × E[Kicks]", "{hit} × {hits} = {total}"),
        ], &[("damage", setup.kick_damage), ("fstr", setup.fstr_kick), ("pdif", kick_pdif),
            ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage),
            ("base", kick_base_damage), ("dragon_fangs", setup.dragon_fangs_bonus),
            ("hit", kick_damage), ("hits", hits.kick), ("total", kick_expected_damage)]);
    }
    let daken_pdif = get_avg_pdif_ranged(setup.ranged_attack, setup.ammo_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let daken_damage = average_hit(setup.ammo_damage, setup.fstr_ammo, daken_pdif, setup.crit_rate, setup.crit_damage)?;
    let daken_expected_damage = divide(hits.daken * daken_damage, setup.empyrean_bonus, OPERATION, "empyrean_am_damage_bonus")?;
    physical_damage += daken_expected_damage;
    if tracing && (setup.daken != 0.0 || hits.daken != 0.0) {
        ctx.record_step(Section::Damage, "Daken fSTR2 and pDIF", &[
            ("dSTR = clamp(STR − VIT, −2 × (7 + 2 × D / 9), 2 × (14 + 2 × D / 9))",
                "clamp({strength} − {vitality}, −2 × (7 + 2 × {damage} / 9), 2 × (14 + 2 × {damage} / 9))"),
            ("fSTR2 = clamp(F(dSTR) / 2, −2 × D / 9, 2 × (D / 9 + 8))",
                "clamp(F(dSTR) / 2, −2 × {damage} / 9, 2 × ({damage} / 9 + 8)) = {fstr}"),
            ("r = RAtk / Def", "{attack} / {defense}"),
            ("pDIF = clamp((Lr(r) + Ur(r)) / 2, 0, (Skill cap + PDL trait) × (1 + PDL gear)) × (1 + 0.25 × min(Crit rate, 1))",
                "clamp((Lr(r) + Ur(r)) / 2, 0, (Skill cap + {pdl_trait}) × (1 + {pdl_gear})) × (1 + 0.25 × min({crit_rate}, 1)) = {pdif}"),
        ], &[("damage", setup.ammo_damage), ("strength", stats.get(Stat::Str)),
            ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_ammo),
            ("attack", setup.ranged_attack), ("defense", setup.enemy_defense),
            ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear),
            ("crit_rate", setup.crit_rate), ("pdif", daken_pdif)]);
        ctx.record_step(Section::Damage, "Daken damage", &[
            ("Daken hit = trunc(D + fSTR2) × pDIF × (1 + min(Crit rate, 1) × min(Crit dmg, 1))",
                "trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}"),
            ("Daken damage = E[Daken hits] × Daken hit / Empyrean", "{hits} × {hit} / {empyrean} = {total}"),
        ], &[("damage", setup.ammo_damage), ("fstr", setup.fstr_ammo), ("pdif", daken_pdif),
            ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage), ("hit", daken_damage),
            ("hits", hits.daken), ("empyrean", setup.empyrean_bonus), ("total", daken_expected_damage)]);
    }
    let zanshin_pdif = get_avg_pdif_melee(setup.attack1 + stats.get(Stat::ZanshinAttack),
        setup.main_skill.into(), setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let mut zanshin_damage = average_hit(setup.main_damage, setup.fstr_main, zanshin_pdif, setup.crit_rate, setup.crit_damage)?;
    let zanshin_base_damage = zanshin_damage;
    zanshin_damage *= setup.empyrean_bonus;
    let zanshin_expected_damage = hits.zanshin * zanshin_damage;
    physical_damage += zanshin_expected_damage;
    if tracing && (setup.zanshin != 0.0 || setup.zanhasso != 0.0 || hits.zanshin != 0.0) {
        ctx.record_step(Section::Damage, "Zanshin pDIF", &[
            ("w = (Atk + Zanshin Atk) / Def + min(Crit rate, 1)", "({attack} + {zanshin_attack}) / {defense} + min({crit_rate}, 1)"),
            MELEE_PDIF,
        ], &[("attack", setup.attack1), ("zanshin_attack", stats.get(Stat::ZanshinAttack)),
            ("defense", setup.enemy_defense), ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear),
            ("crit_rate", setup.crit_rate), ("pdif", zanshin_pdif)]);
        ctx.record_step(Section::Damage, "Zanshin damage", &[
            AVG_HIT,
            ("Zanshin hit = Avg hit × Empyrean", "{base} × {empyrean} = {hit}"),
            ("Zanshin damage = E[Zanshin hits] × Zanshin hit", "{hits} × {hit} = {total}"),
        ], &[("damage", setup.main_damage), ("fstr", setup.fstr_main), ("pdif", zanshin_pdif),
            ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage), ("base", zanshin_base_damage),
            ("empyrean", setup.empyrean_bonus), ("hit", zanshin_damage), ("hits", hits.zanshin),
            ("total", zanshin_expected_damage)]);
    }
    let damage = physical_damage + magical_damage;
    if tracing {
        ctx.record_step(Section::Damage, "Damage per round", &[
            ("Physical = Melee + Kick + Daken + Zanshin", "{melee} + {kick} + {daken} + {zanshin} = {physical}"),
            ("Damage = Physical + Magical", "{physical} + {magical} = {damage}"),
        ], &[("melee", melee_damage), ("kick", kick_expected_damage), ("daken", daken_expected_damage),
            ("zanshin", zanshin_expected_damage), ("physical", physical_damage),
            ("magical", magical_damage), ("damage", damage)]);
    }
    let (value, invert) = match metric.name.as_ref() {
        "Damage dealt" => (damage, 1),
        "TP return" => (tp_return, 1),
        "DPS" => (divide(damage, seconds_per_round, OPERATION, "time_per_attack_round")?, 1),
        "TP > Damage" => (damage * tp_return * tp_return / 1e4, 1),
        "Damage > TP" => (damage * damage * tp_return / 1e6, 1),
        // The round fallback differs from WS/spell fallbacks. Capitalization is
        // observable: "Damage Dealt" and "Magic Accuracy" both select time.
        _ => (time_to_ws, -1),
    };
    if tracing {
        if seconds_per_round == 0.0 {
            ctx.record_step(Section::Result, "DPS", &[("DPS = Damage / Seconds", "{damage} / {seconds}")],
                &[("damage", damage), ("seconds", seconds_per_round)]);
        } else {
            ctx.record_step(Section::Result, "DPS", &[("DPS = Damage / Seconds", "{damage} / {seconds} = {dps}")],
                &[("damage", damage), ("seconds", seconds_per_round), ("dps", damage / seconds_per_round)]);
        }
        let lines: &'static [(&'static str, &'static str)] = match metric.name.as_ref() {
            "Damage dealt" => &[("Metric = Damage", "{value}"), ("Invert", "{invert}")],
            "TP return" => &[("Metric = TP", "{value}"), ("Invert", "{invert}")],
            "DPS" => &[("Metric = Damage / Seconds", "{damage} / {seconds} = {value}"), ("Invert", "{invert}")],
            "TP > Damage" => &[("Metric = Damage × TP × TP / 10000", "{damage} × {tp} × {tp} / 10000 = {value}"),
                ("Invert", "{invert}")],
            "Damage > TP" => &[("Metric = Damage × Damage × TP / 1000000", "{damage} × {damage} × {tp} / 1000000 = {value}"),
                ("Invert", "{invert}")],
            _ => &[("Metric = Time to WS", "{value}"), ("Invert", "{invert}")],
        };
        ctx.record_step(Section::Result, "Selected metric", lines,
            &[("damage", damage), ("tp", tp_return), ("seconds", seconds_per_round),
                ("time", time_to_ws), ("value", value), ("invert", invert as f64)]);
    }
    Ok(ActionResult { damage, tp_return, seconds_per_round: Some(seconds_per_round),
        time_to_ws: Some(time_to_ws), metric: value, invert, physical_damage, magical_damage })
}

#[derive(Clone, Copy)]
struct MultiattackRolls { qa: bool, ta: bool, da: bool }

impl MultiattackRolls {
    fn draw<R: RandomSource>(setup: &RoundSetup, ctx: &mut EvaluationContext<R>) -> Self {
        let qa = ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.qa;
        // Source uses multiplication rather than `and`: all three draws occur.
        let ta = (ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.ta) && !qa;
        let da = (ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.da) && !qa && !ta;
        Self { qa, ta, da }
    }

    fn proc(self) -> AttackProc {
        if self.qa { AttackProc::QuadAttack }
        else if self.ta { AttackProc::TripleAttack }
        else if self.da { AttackProc::DoubleAttack }
        else { AttackProc::Normal }
    }

    fn bonus(self, setup: &RoundSetup) -> Option<(AttackProc, u8, f64)> {
        if self.qa { Some((AttackProc::QuadAttack, 3, 1.0)) }
        else if self.ta { Some((AttackProc::TripleAttack, 2, 1.0 + setup.ta_damage)) }
        else if self.da { Some((AttackProc::DoubleAttack, 1, 1.0 + setup.da_damage)) }
        else { None }
    }
}

#[derive(Default)]
struct SampleTotals { attempts: u8, tp: f64, magic: f64 }

#[inline]
fn record<R: RandomSource>(ctx: &mut EvaluationContext<R>, totals: &SampleTotals,
    kind: AttackKind, proc: AttackProc, landed: bool, critical: bool, physical: f64,
    magic: f64, tp: f64) {
    ctx.record_hit(HitEvent { kind, proc, attempt: totals.attempts, landed, critical,
        physical_damage: physical, magical_damage: magic, tp_return: tp });
}

#[inline]
fn sampled_hit(damage: f64, fstr: f64, pdif: f64, critical: bool, crit_damage: f64) -> Result<f64, CalcError> {
    get_phys_damage(damage, fstr, 0.0, pdif, 1.0, critical, crit_damage,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
}

// The source indexes before checking weapon/positive AM. Preserve negative
// Python indexing and late failure, including the preceding uniform draw.
#[inline]
fn empyrean_proc<R: RandomSource>(setup: &RoundSetup, ctx: &mut EvaluationContext<R>) -> Result<bool, CalcError> {
    let draw = ctx.random.uniform(RandomStream::Primary, 0.0, 1.0);
    let rate = match setup.aftermath {
        -2.0 | 1.0 => EMPYREAN_AM[0],
        -1.0 | 2.0 => EMPYREAN_AM[1],
        0.0 | 3.0 => EMPYREAN_AM[2],
        _ => {
            let message = if setup.aftermath >= 9223372036854775808.0
                || setup.aftermath <= -9223372036854775808.0 {
                "cannot fit 'int' into an index-sized integer"
            } else { "list index out of range" };
            return Err(CalcError::legacy(OPERATION, "Aftermath", "IndexError", message));
        }
    };
    Ok(draw < rate && setup.empyrean && setup.aftermath > 0.0)
}

fn oa_proc<R: RandomSource>(player: &Player, offhand: bool, ctx: &mut EvaluationContext<R>) -> Option<(AttackProc, u8, f64)> {
    for (proc, rate, bonus) in oa_tiers(&player.stats, offhand) {
        if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < rate {
            return Some((proc, bonus, 1.0));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn bonus_swings<R: RandomSource>(
    setup: &RoundSetup,
    count: u8,
    hit_rate: f64,
    attack: f64,
    skill: SkillType,
    damage: f64,
    fstr: f64,
    enspell: f64,
    damage_multiplier: f64,
    roll_empyrean: bool,
    sam_tp: bool,
    kind: AttackKind,
    proc: AttackProc,
    totals: &mut SampleTotals,
    ctx: &mut EvaluationContext<R>,
) -> Result<f64, CalcError> {
    let mut total_damage = 0.0;
    for _ in 0..count {
        if totals.attempts < 8 {
            totals.attempts += 1;
            if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < hit_rate {
                let (pdif, critical) = get_pdif_melee(attack, skill.into(), setup.pdl_trait,
                    setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
                let mut physical = sampled_hit(damage, fstr, pdif, critical, setup.crit_damage)?;
                if roll_empyrean { physical *= 1.0 + 2.0 * flag(empyrean_proc(setup, ctx)?); }
                physical *= damage_multiplier;
                total_damage += physical;
                let tp = get_tp(1.0, setup.hit_delay(), setup.stp, sam_tp)?;
                totals.tp += tp;
                let mut magic = 0.0;
                if setup.enspell_active {
                    magic += enspell;
                    totals.magic += magic;
                }
                record(ctx, totals, kind, proc, true, critical, physical, magic, tp);
            } else {
                record(ctx, totals, kind, proc, false, false, 0.0, 0.0, 0.0);
            }
        }
    }
    Ok(total_damage)
}

pub fn sample_round<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    starting_tp: f64,
    _threshold: f64,
    ctx: &mut EvaluationContext<R>,
) -> Result<RoundSample, CalcError> {
    let setup = prepare_round(player, enemy, starting_tp, ctx)?;
    // Sampled rounds explicitly disable all first-hit abilities and do not use
    // the analytical FUA or flourish values.
    let main_rolls = MultiattackRolls::draw(&setup, ctx);
    let sub_rolls = MultiattackRolls::draw(&setup, ctx);
    let mut totals = SampleTotals::default();
    let mut main_damage = 0.0;
    let mut sub_damage = 0.0;
    let mut physical_damage = 0.0;
    let mut main_connected = false;
    let mut main_ma_proc = false;
    totals.attempts += 1;
    if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.hit_rates[0][0] {
        main_connected = true;
        let (pdif, critical) = get_pdif_melee(setup.attack1, setup.main_skill.into(),
            setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
        let mut physical = sampled_hit(setup.main_damage, setup.fstr_main, pdif, critical, setup.crit_damage)?;
        // Membership is checked AFTER each draw, even on ordinary weapons.
        physical *= 1.0 + 2.0 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < 0.13 && setup.relic30);
        physical *= 1.0 + 1.5 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < 0.16 && setup.relic25);
        physical *= 1.0 + 1.0 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < 0.2 && setup.relic20);
        physical *= 1.0 + 2.0 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < 0.3 && setup.prime3);
        physical *= 1.0 + 1.0 * flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < 0.3 && setup.prime2);
        physical *= 1.0 + 2.0 * flag(empyrean_proc(&setup, ctx)?);
        physical *= 1.0 + setup.da_damage * flag(main_rolls.da);
        physical *= 1.0 + setup.ta_damage * flag(main_rolls.ta);
        main_damage += physical;
        let tp = get_tp(1.0, setup.hit_delay(), setup.stp, false)?;
        totals.tp += tp;
        let mut magic = 0.0;
        if setup.enspell_active { magic += setup.main_enspell; totals.magic += magic; }
        record(ctx, &totals, AttackKind::Main, main_rolls.proc(), true, critical, physical, magic, tp);
    } else {
        record(ctx, &totals, AttackKind::Main, main_rolls.proc(), false, false, 0.0, 0.0, 0.0);
    }
    if setup.dual_wield {
        totals.attempts += 1;
        if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.hit_rates[0][1] {
            let (pdif, critical) = get_pdif_melee(setup.attack2, setup.sub_skill.into(),
                setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
            let mut physical = sampled_hit(setup.sub_damage, setup.fstr_sub, pdif, critical, setup.crit_damage)?;
            physical *= 1.0 + setup.da_damage * flag(sub_rolls.da);
            physical *= 1.0 + setup.ta_damage * flag(sub_rolls.ta);
            sub_damage += physical;
            let tp = get_tp(1.0, setup.hit_delay(), setup.stp, false)?;
            totals.tp += tp;
            let mut magic = 0.0;
            if setup.enspell_active { magic += setup.sub_enspell; totals.magic += magic; }
            record(ctx, &totals, AttackKind::Offhand, sub_rolls.proc(), true, critical, physical, magic, tp);
        } else {
            record(ctx, &totals, AttackKind::Offhand, sub_rolls.proc(), false, false, 0.0, 0.0, 0.0);
        }
    }
    if let Some((proc, count, multiplier)) = main_rolls.bonus(&setup).or_else(|| oa_proc(player, false, ctx)) {
        main_ma_proc = true;
        main_damage += bonus_swings(&setup, count, setup.hit_rates[1][0], setup.attack1,
            setup.main_skill, setup.main_damage, setup.fstr_main, setup.main_enspell,
            multiplier, true, false, AttackKind::Main, proc, &mut totals, ctx)?;
    }
    physical_damage += main_damage;
    if setup.dual_wield {
        if totals.attempts < 8 {
            if let Some((proc, count, multiplier)) = sub_rolls.bonus(&setup).or_else(|| oa_proc(player, true, ctx)) {
                sub_damage += bonus_swings(&setup, count, setup.hit_rates[1][1], setup.attack2,
                    setup.sub_skill, setup.sub_damage, setup.fstr_sub, setup.sub_enspell,
                    multiplier, false, false, AttackKind::Offhand, proc, &mut totals, ctx)?;
            }
        }
        physical_damage += sub_damage;
    }
    let mut zanshin_damage = 0.0;
    if totals.attempts < 8 {
        // Keep this left-to-right short circuit: landed main hits consume a
        // Zanhasso draw even on dual wield/one-handed gear and after an MA proc.
        if (!main_connected || ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.zanhasso)
            && !setup.dual_wield && TWO_HANDED_SKILLS.contains(&setup.main_skill) {
            if !main_ma_proc {
                let attack = setup.attack1 + player.stats.get(Stat::ZanshinAttack);
                if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.zanshin_oa2 {
                    zanshin_damage += bonus_swings(&setup, 2, setup.zanshin_hit_rate, attack,
                        setup.main_skill, setup.main_damage, setup.fstr_main, setup.main_enspell,
                        1.0, true, player.main_job == Job::Sam, AttackKind::Zanshin,
                        AttackProc::Occasionally(2), &mut totals, ctx)?;
                } else if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.zanshin {
                    zanshin_damage += bonus_swings(&setup, 1, setup.zanshin_hit_rate, attack,
                        setup.main_skill, setup.main_damage, setup.fstr_main, setup.main_enspell,
                        1.0, true, player.main_job == Job::Sam, AttackKind::Zanshin,
                        AttackProc::Normal, &mut totals, ctx)?;
                }
            }
        }
    }
    physical_damage += zanshin_damage;
    let mut kick_damage = 0.0;
    if totals.attempts < 8 {
        if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.kickattacks && setup.main_skill == SkillType::HandToHand {
            totals.attempts += 1;
            if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.hit_rates[0][0] {
                let (pdif, critical) = get_pdif_melee(setup.attack1 + player.stats.get(Stat::KickAttacksAttack),
                    setup.main_skill.into(), setup.pdl_trait, setup.pdl_gear, setup.enemy_defense,
                    setup.crit_rate, &mut ctx.random)?;
                let physical = sampled_hit(setup.kick_damage, setup.fstr_kick, pdif, critical, setup.crit_damage)?;
                // The sampled source applies this 20% double damage roll to ALL
                // H2H kicks, not only Dragon Fangs. Trace the boosted damage.
                kick_damage = physical * (1.0 + flag(ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < 0.2));
                let tp = get_tp(1.0, setup.hit_delay(), setup.stp, false)?;
                totals.tp += tp;
                let mut magic = 0.0;
                if setup.enspell_active { magic += setup.main_enspell; totals.magic += magic; }
                record(ctx, &totals, AttackKind::Kick, AttackProc::Normal, true, critical, kick_damage, magic, tp);
            } else {
                record(ctx, &totals, AttackKind::Kick, AttackProc::Normal, false, false, 0.0, 0.0, 0.0);
            }
        }
    }
    physical_damage += kick_damage;
    let mut daken_damage = 0.0;
    if totals.attempts < 8 {
        if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.daken && setup.ammo_skill == SkillType::Throwing {
            totals.attempts += 1;
            if ctx.random.uniform(RandomStream::Primary, 0.0, 1.0) < setup.hit_rate_ranged {
                let (pdif, critical) = get_pdif_ranged(setup.ranged_attack, setup.ammo_skill.into(),
                    setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
                daken_damage = sampled_hit(setup.ammo_damage, setup.fstr_ammo, pdif, critical, setup.crit_damage)?;
                let tp = get_tp(1.0, setup.ammo_delay, setup.stp, false)?;
                totals.tp += tp;
                record(ctx, &totals, AttackKind::Daken, AttackProc::Normal, true, critical, daken_damage, 0.0, tp);
            } else {
                record(ctx, &totals, AttackKind::Daken, AttackProc::Normal, false, false, 0.0, 0.0, 0.0);
            }
        }
    }
    physical_damage += daken_damage;
    // Preserve the physical-only sampled legacy tuple in physical_damage,
    // while exposing an unambiguous total for native consumers.
    Ok(RoundSample { damage: physical_damage + totals.magic, tp_return: totals.tp,
        physical_damage, magical_damage: totals.magic })
}
