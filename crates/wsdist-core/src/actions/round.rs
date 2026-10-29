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
use crate::stats::Stat;

use super::common::{
    EMPYREAN_AM, EMPYREAN_WEAPONS, ONE_HANDED_SKILLS, PRIME_WEAPONS_2,
    PRIME_WEAPONS_3, RELIC_WEAPONS_20, RELIC_WEAPONS_25, RELIC_WEAPONS_30,
    TWO_HANDED_SKILLS, oa_list, oa_tiers, record_tp_gain, record_tp_reference,
    weapon_damage_bonuses,
};

const OPERATION: &str = "average_attack_round";

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
            * (1.0 + 0.25 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < magic_crit_rate2));
        // Deliberately main-hand percentage on the sampled offhand damage.
        // Both uniforms are also consumed by the analytical setup.
        sub_enspell = get_enspell_damage(enhancing_skill, enspell_percent_main, enspell_damage_sub)?
            * (1.0 + dayweather) * (1.0 + elemental_bonus)
            * (1.0 + 0.25 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < magic_crit_rate2));
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
        ctx.record_equation(
            "Analytical damage conventions",
            "BG Wiki reference: Physical Damage = Base Damage × pDIF\nMelee Base Damage = Weapon Base Damage + fSTR\nEvaluator average landed-hit damage = trunc(Base Damage) × legacy average pDIF × expected critical-damage multiplier\nOrdinary expected component = expected landed hits × average landed-hit damage\nAbility, weapon, and component-specific adjustments are shown separately below.",
            "trunc(x) truncates toward zero. For dSTR = STR − Target VIT, adjusted dSTR F(x) = x+13 if x<=-22; x+12 if x<=-16; x+10 if x<=-8; x+9 if x<=-3; x+8 if x<=0; x+7 if x<=5; x+6 if x<12; otherwise x+4. Melee fSTR = clamp(F(dSTR)/4, −Weapon Base Damage/9, 8+Weapon Base Damage/9). This evaluator retains fractional rank and fSTR, truncates the combined base before pDIF, and does not truncate after pDIF or expected-hit weighting.",
            &[],
        );
        ctx.record_equation(
            "Main-hand damage and fSTR",
            "dSTR = STR − Target VIT\nWeapon Rank = Weapon Base Damage / 9\nfSTR = clamp(adjusted dSTR / 4, −Weapon Rank, 8 + Weapon Rank)\nBase Damage = Weapon Base Damage + fSTR",
            "dSTR = {strength} − {vitality}; Weapon Rank = {damage} / 9; fSTR = clamp(F(dSTR) / 4, −{damage} / 9, 8 + {damage} / 9) = {fstr}; Base Damage = {damage} + {fstr}. Rank and fSTR are not floored.",
            &[("damage", setup.main_damage), ("strength", stats.get(Stat::Str)),
                ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_main)],
        );
        if setup.dual_wield {
            ctx.record_equation(
                if setup.main_skill == SkillType::HandToHand { "Second-fist damage and fSTR" } else { "Offhand damage and fSTR" },
                "dSTR = STR − Target VIT\nWeapon Rank = Weapon Base Damage / 9\nfSTR = clamp(adjusted dSTR / 4, −Weapon Rank, 8 + Weapon Rank)\nBase Damage = Weapon Base Damage + fSTR",
                "dSTR = {strength} − {vitality}; Weapon Rank = {damage} / 9; fSTR = clamp(F(dSTR) / 4, −{damage} / 9, 8 + {damage} / 9) = {fstr}; Base Damage = {damage} + {fstr}. Rank and fSTR are not floored.",
                &[("damage", setup.sub_damage), ("strength", stats.get(Stat::Str)),
                    ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_sub)],
            );
        }
        ctx.record_equation(
            "DEX critical contribution",
            "dDEX = DEX − Target AGI\nDEX critical contribution = the evaluator's piecewise dDEX bonus",
            "dDEX = {dex} − {enemy_agi}; DEX critical contribution = 0 for dDEX<=6, 0.01 for <=13, 0.02 for <=19, 0.03 for <=29, 0.04 for <=39, otherwise min((dDEX−35)/100, 0.15)",
            &[("dex", stats.get(Stat::Dex)), ("enemy_agi", enemy.stats.get(Stat::Agi))],
        );
        let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
        ctx.record_equation(
            "Critical chance and Store TP",
            "Critical Hit Rate = min(1, Sheet Critical Hit Rate / 100 + DEX critical contribution + Tauret bonus)\nTauret bonus = 0.5 × (1 − Starting TP / 3000) when wielding Tauret, otherwise 0\nStore TP = Sheet Store TP / 100 + Karambit bonus\nKarambit bonus = 0.5 × Critical Hit Rate when wielding Karambit, otherwise 0\nExpected critical-damage multiplier = 1 + min(Critical Hit Rate, 1) × min(Critical Damage Bonus, 1)",
            "Critical Hit Rate = min(1, {base_crit} / 100 + DEX critical contribution at DEX {dex} and Target AGI {enemy_agi} + 0.5 × (1 − {starting_tp} / 3000) × {tauret}) = {crit_rate}; Store TP = {store_tp} / 100 + 0.5 × {crit_rate} × {karambit} = {stp}; Critical Damage Bonus = {crit_damage}",
            &[("base_crit", stats.get(Stat::CritRate)), ("dex", stats.get(Stat::Dex)),
                ("enemy_agi", enemy.stats.get(Stat::Agi)), ("starting_tp", starting_tp),
                ("tauret", flag(main.name.as_ref() == "Tauret")), ("crit_rate", setup.crit_rate),
                ("store_tp", stats.get(Stat::StoreTp)), ("karambit", flag(main.name.as_ref() == "Karambit")),
                ("stp", setup.stp), ("crit_damage", setup.crit_damage)],
        );
        ctx.record_calculation(
            "Expected-value random terms",
            "hit = I([random hit roll: U(0,1)] < hit probability); critical = I([random critical roll: U(0,1)] < {crit_rate}); sampled melee damage uses [random pDIF roll] and [random damage spread: U(1,1.05)]. This calculation uses expected landed hits and legacy average pDIF, not sampled rolls.",
            &[("crit_rate", setup.crit_rate)],
        );
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
        ctx.record_calculation(
            "Melee hit probabilities",
            "enemy evasion = max(1, {evasion}); hit probability = cap-before-floor((75 + 0.5 * (accuracy - evasion)) / 100, floor 0.2). Main accuracy {accuracy1}, cap {main_cap}: first {main_first}, later {main_later}; offhand/second-fist accuracy {accuracy2}, cap {sub_cap}: first {sub_first}, later {sub_later}; offhand/second fist active = {dual_wield}",
            &[("evasion", enemy.stats.get(Stat::Evasion)), ("accuracy1", stats.get(Stat::Accuracy1)),
                ("main_cap", if ONE_HANDED_SKILLS.contains(&setup.main_skill) || setup.main_skill == SkillType::HandToHand { 0.99 } else { 0.95 }),
                ("main_first", setup.hit_rates[0][0]), ("main_later", setup.hit_rates[1][0]),
                ("accuracy2", stats.get(Stat::Accuracy2)), ("sub_cap", if setup.sub_skill == SkillType::HandToHand { 0.99 } else { 0.95 }),
                ("sub_first", setup.hit_rates[0][1]), ("sub_later", setup.hit_rates[1][1]), ("dual_wield", flag(setup.dual_wield))],
        );
        ctx.record_calculation(
            "Multiattack proc hierarchy",
            "prepared QA q = {qa}, TA t = {ta}, DA d = {da} (sheet percent / 100, upper-cap only); main OA3 = {oa3}, OA2 = {oa2}. Proc probabilities in priority order: QA q; TA (1-q)*t; DA (1-q)*(1-t)*d; OA3 (1-q)*(1-t)*(1-d)*OA3; OA2 (1-q)*(1-t)*(1-d)*(1-OA3)*OA2. A tier comparison is I([random multiattack proc roll: U(0,1)] < tier rate); FUA is not applied by the analytical helper.",
            &[("qa", setup.qa), ("ta", setup.ta), ("da", setup.da), ("oa3", setup.oa[0]), ("oa2", setup.oa[1])],
        );
        ctx.record_calculation(
            if setup.main_skill == SkillType::HandToHand { "Expected first-fist landed hits" } else { "Expected main-hand landed hits" },
            "main q,t,d = (0,0,1) if Striking {striking}=1, otherwise (0,1,0) if Ternary {ternary}=1, otherwise prepared q,t,d. Start M0 = {main_first}; S0 = {sub_first} if offhand/second fist is active and M0<8, otherwise 0; R0 = 8-(M0+S0). E_main = M0 + {main_later} * [max(0,min(3,R0)*q) + max(0,min(2,R0)*(1-q)*t) + max(0,min(1,R0)*(1-q)*(1-t)*d) + max(0,min(2,R0)*(1-q)*(1-t)*(1-d)*{oa3}) + max(0,min(1,R0)*(1-q)*(1-t)*(1-d)*(1-{oa3})*{oa2})] = {hits}",
            &[("striking", flag(striking)), ("ternary", flag(ternary)), ("main_first", setup.hit_rates[0][0]),
                ("sub_first", setup.hit_rates[0][1]), ("main_later", setup.hit_rates[1][0]),
                ("oa3", setup.oa[0]), ("oa2", setup.oa[1]), ("hits", hits.main)],
        );
        if setup.dual_wield {
            ctx.record_calculation(
                if setup.main_skill == SkillType::HandToHand { "Expected second-fist landed hits" } else { "Expected offhand landed hits" },
                "S0 = {first} if initial main hits<8, otherwise 0; R1 = 8-({main}+S0); E_sub = S0 + {later} * sum(max(0,min(extra swings,R1))*proc probability) = {hits}. Unmodified q={qa}, t={ta}, d={da}; priority QA/TA/DA/OA8/OA7/OA6/OA5/OA4/OA3/OA2 adds 3/2/1/7/6/5/4/3/2/1 swings, each gated by failure of earlier tiers. Offhand OA8..OA2 = {oa8}, {oa7}, {oa6}, {oa5}, {oa4}, {oa3}, {oa2}. This is an eight-expected-landed-hit approximation, not an eight-attempt simulation.",
                &[("first", setup.hit_rates[0][1]), ("main", hits.main), ("later", setup.hit_rates[1][1]),
                    ("hits", hits.sub), ("qa", setup.qa), ("ta", setup.ta), ("da", setup.da),
                    ("oa8", setup.oa[2]), ("oa7", setup.oa[3]), ("oa6", setup.oa[4]), ("oa5", setup.oa[5]),
                    ("oa4", setup.oa[6]), ("oa3", setup.oa[7]), ("oa2", setup.oa[8])],
            );
        }
        if setup.kickattacks != 0.0 || hits.kick != 0.0 {
            ctx.record_calculation(
                "Expected landed kicks",
                "remaining expected-hit budget R = 8-({main}+{sub}); E_kick = min(max(0,R),1) * {rate} * {hit_rate} = {hits}; proc = I([random kick proc roll: U(0,1)] < {rate})",
                &[("main", hits.main), ("sub", hits.sub), ("rate", setup.kickattacks),
                    ("hit_rate", setup.hit_rates[1][0]), ("hits", hits.kick)],
            );
        }
        if setup.daken != 0.0 || hits.daken != 0.0 {
            ctx.record_calculation(
                "Daken hit probability and expected landed attacks",
                "ranged accuracy = {accuracy} + 100 * I(Daken sheet rate > 0); enemy evasion = max(1,{evasion}); ranged hit cap = {cap}; ranged hit probability = {hit_rate}. E_Daken = min(max(0,8-({main}+{sub})-{kick_rate}),1) * {daken} * {hit_rate} = {hits}; proc = I([random Daken proc roll: U(0,1)] < {daken}). The budget subtracts kick proc rate, not expected landed kicks.",
                &[("accuracy", stats.get(Stat::RangedAccuracy)), ("evasion", enemy.stats.get(Stat::Evasion)),
                    ("cap", if player.abilities.enabled(Ability::Sharpshot) { 0.99 } else { 0.95 }),
                    ("hit_rate", setup.hit_rate_ranged), ("main", hits.main), ("sub", hits.sub),
                    ("kick_rate", setup.kickattacks), ("daken", setup.daken), ("hits", hits.daken)],
            );
        }
        if setup.zanshin != 0.0 || setup.zanhasso != 0.0 || hits.zanshin != 0.0 {
            ctx.record_calculation(
                "Expected landed Zanshin attacks",
                "N = (1-q_main)*(1-t_main)*(1-d_main)*(1-{oa3})*(1-{oa2}), using the main-hand Flourish-adjusted rates; Zanshin hit probability = get_hit_rate({accuracy}+34, max(1,{evasion}), 0.95) = {zanshin_hit_rate}. E_Zanshin = N * [(1-{main_hit_rate})*{zanshin} + {sam}*{main_hit_rate}*{zanhasso}] * (1+{oa2_rate}) * {zanshin_hit_rate} = {hits}. Proc comparisons use [random Zanshin proc roll: U(0,1)] and [random Zanshin OA2 roll: U(0,1)]; Zanshin is outside the eight-expected-landed-hit budget.",
                &[("oa3", setup.oa[0]), ("oa2", setup.oa[1]), ("accuracy", stats.get(Stat::Accuracy1)),
                    ("evasion", enemy.stats.get(Stat::Evasion)), ("zanshin_hit_rate", setup.zanshin_hit_rate),
                    ("main_hit_rate", setup.hit_rates[1][0]), ("zanshin", setup.zanshin),
                    ("sam", flag(player.main_job == Job::Sam)), ("zanhasso", setup.zanhasso),
                    ("oa2_rate", setup.zanshin_oa2), ("hits", hits.zanshin)],
            );
        }
        ctx.record_calculation(
            "Expected landed-hit totals",
            "main/first fist = {main}; offhand/second fist = {sub}; kick = {kick}; Daken = {daken}; Zanshin = {zanshin}. These are fractional expected landed hits from the legacy eight-landed-hit approximation and already include hit probabilities.",
            &[("main", hits.main), ("sub", hits.sub), ("kick", hits.kick), ("daken", hits.daken), ("zanshin", hits.zanshin)],
        );
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
            ctx.record_equation(
                "EnSpell skill scaling",
                "BG Wiki reference: Damage Dealt = D × MTDR × Staff × Affinity × Resist × Resistance Rank Reduction × MB × MBB × Day & Weather × (MAB / MDB) × TMDA × Potency Multipliers\nEvaluator EnSpell base per landed hit = (trunc(Scaled Enhancing Magic Skill) + 29 + Flat EnSpell Damage) × (1 + EnSpell Damage Bonus / 100)\nThis EnSpell base uses enhancing skill, not the reference's dINT-adjusted spell base. Only the explicitly shown EnSpell, Day & Weather, elemental-bonus, and magic-critical factors are modeled here; other reference factors are unmodeled, not measured values of 1.",
                "Scaled Enhancing Magic Skill = (skill−223)/7.70 for skill<600, otherwise (skill−202.5)/8.05. The BG Wiki reference floors after each multiplier; this evaluator truncates only scaled skill, before adding 29 and the flat bonus, and retains fractional products thereafter.",
                &[],
            );
            ctx.record_equation(
                "EnSpell damage per landed hit",
                "Main EnSpell base = (trunc(Scaled Enhancing Magic Skill) + 29 + Main Flat Bonus) × (1 + Main Percent Bonus / 100)\nOffhand EnSpell base = (trunc(Scaled Enhancing Magic Skill) + 29 + Offhand Flat Bonus) × (1 + Offhand Percent Bonus / 100)\nKick and Zanshin EnSpell bases = Main EnSpell base",
                "Enhancing Magic Skill = {skill}; main EnSpell base = (trunc(scaled skill) + 29 + {main_flat}) × (1 + {main_percent} / 100) = {main}; offhand EnSpell base = (trunc(scaled skill) + 29 + {sub_flat}) × (1 + {sub_percent} / 100) = {sub}; kicks = {kick}; Zanshin = {zanshin}",
                &[("skill", setup.enhancing_skill), ("main_percent", setup.enspell_percent_main),
                    ("main_flat", setup.enspell_damage_main), ("main", main_enspell),
                    ("sub_percent", setup.enspell_percent_sub), ("sub_flat", setup.enspell_damage_sub),
                    ("sub", sub_enspell), ("kick", kick_enspell), ("zanshin", zanshin_enspell)],
            );
            ctx.record_equation(
                "Expected EnSpell subtotal",
                "Expected EnSpell subtotal = Main Landed Hits × Main EnSpell base + Offhand Landed Hits × Offhand EnSpell base + Landed Kicks × Kick EnSpell base + Landed Zanshin Hits × Zanshin EnSpell base",
                "EnSpell subtotal = {main_hits} × {main} + {sub_hits} × {sub} + {kick_hits} × {kick} + {zanshin_hits} × {zanshin} = {subtotal}",
                &[("main_hits", hits.main), ("main", main_enspell), ("sub_hits", hits.sub),
                    ("sub", sub_enspell), ("kick_hits", hits.kick), ("kick", kick_enspell),
                    ("zanshin_hits", hits.zanshin), ("zanshin", zanshin_enspell), ("subtotal", magical_damage)],
            );
        }
        let enspell_subtotal = magical_damage;
        magical_damage *= 1.0 + setup.dayweather;
        magical_damage *= 1.0 + setup.elemental_bonus;
        magical_damage *= 1.0 + 0.25 * setup.magic_crit_rate2;
        if tracing {
            ctx.record_equation(
                "Expected EnSpell multipliers",
                "Expected EnSpell Damage = Expected EnSpell subtotal × Day & Weather × Elemental Bonus Multiplier × Expected Magic Critical Multiplier\nDay & Weather = 1 + Day & Weather Bonus\nElemental Bonus Multiplier = 1 + Elemental Bonus\nExpected Magic Critical Multiplier = 1 + 0.25 × Magic Critical Rate II\nThis is the evaluator's multiplication order, with no flooring between these factors.",
                "EnSpell Damage = {subtotal} × (1 + {dayweather}) × (1 + {elemental_bonus}) × (1 + 0.25 × {magic_crit_rate}) = {magic}; sampled magic critical = I([random magic critical roll: U(0,1)] < {magic_crit_rate}). The two compatibility preparation rolls are consumed unchanged but are not used by this analytical expectation.",
                &[("subtotal", enspell_subtotal), ("dayweather", setup.dayweather), ("elemental_bonus", setup.elemental_bonus),
                    ("magic_crit_rate", setup.magic_crit_rate2), ("magic", magical_damage)],
            );
        }
    } else if tracing {
        ctx.record_calculation(
            "Analytical magical damage",
            "EnSpell is disabled: analytical magical damage = {magic}. Enlight II and Endark II do not add magical damage in this analytical path.",
            &[("magic", magical_damage)],
        );
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
        record_tp_reference(ctx);
        let delay_equation = if setup.main_skill == SkillType::HandToHand {
            "H2H TP Delay = (Base H2H Delay + Weapon Delay − Martial Arts) / 2\nImplemented H2H TP Delay = (((Sheet Main Delay + Sheet Offhand Delay) / 2 − Martial Arts) × (1 − Dual Wield / 100)) / 2\nBuilt H2H sheets already include the 480 base delay in Main Delay, duplicate that total in Offhand Delay, and set Dual Wield to 0. Do not add the base delay again."
        } else if setup.dual_wield {
            "Dual-wield TP Delay per Weapon = (Main Weapon Delay + Offhand Weapon Delay) × (1 − Dual Wield / 100) / 2\nImplemented TP Delay = ((Sheet Main Delay + Sheet Offhand Delay) / 2 − Martial Arts) × (1 − Dual Wield / 100)\nBuilt non-H2H sheets set Martial Arts to 0; the implemented form retains the actual supplied sheet inputs."
        } else {
            "Single-wield TP Delay = Weapon Delay\nImplemented TP Delay = ((Sheet Main Delay + Sheet Offhand Delay) / 2 − Martial Arts) × (1 − Dual Wield / 100)\nBuilt single-wield sheets duplicate Main Delay in Offhand Delay and set Martial Arts and Dual Wield to 0; the implemented form retains the actual supplied sheet inputs."
        };
        ctx.record_equation(
            "TP-generating delay",
            delay_equation,
            "Average sheet delay = ({delay1} + {delay2}) / 2; modified delay = (average sheet delay − {martial_arts}) × (1 − {dual_wield} / 100) = {modified_delay}; TP hit delay = {modified_delay} / {fists} = {hit_delay}. Haste affects seconds per round, not TP per landed hit. This TP setup does not apply a separate Delay Reduction factor.",
            &[("delay1", stats.get(Stat::Delay1)), ("delay2", stats.get(Stat::Delay2)),
                ("martial_arts", stats.get(Stat::MartialArts)), ("dual_wield", stats.get(Stat::DualWield)),
                ("modified_delay", setup.mdelay), ("fists", if setup.main_skill == SkillType::HandToHand { 2.0 } else { 1.0 }),
                ("hit_delay", setup.hit_delay())],
        );
        ctx.record_equation(
            "TP base and truncation",
            "Implemented Base TP = trunc(Delay TP(TP Hit Delay))\nOrdinary Melee or Kick TP per Landed Hit = trunc(Base TP × (1 + Store TP Input))\nZanshin TP per Landed Hit = trunc((Base TP + Zanshin TP Bonus) × (1 + Store TP Input))\nExpected Component TP = Expected Landed Hits × TP per Landed Hit",
            "Store TP Input = {stp}, including any Karambit expectation shown above. Zanshin TP Bonus = 150 for SAM, otherwise 0, before Store TP. Every ordinary landed multiattack hit grants full TP, unlike additional weapon-skill hits' 10 base TP. Both truncations occur before fractional expected-landed-hit weighting; hit probabilities are already included in those expectations, and no truncation follows their multiplication.",
            &[("stp", setup.stp)],
        );
        record_tp_gain(ctx, "Melee and kick TP", hits.main + hits.sub + hits.kick,
            setup.hit_delay(), setup.stp, false, melee_tp);
        record_tp_gain(ctx, if player.main_job == Job::Sam { "SAM Zanshin TP" } else { "Non-SAM Zanshin TP" },
            hits.zanshin, setup.hit_delay(), setup.stp, player.main_job == Job::Sam, zanshin_tp);
        ctx.record_equation(
            "Daken TP-generating delay",
            "Daken TP Delay = Ammo Delay\nDaken uses the shuriken's delay alone, not Ranged Weapon Delay + Ammo Delay or the melee TP delay.",
            "Daken TP delay = {ammo_delay}",
            &[("ammo_delay", setup.ammo_delay)],
        );
        record_tp_gain(ctx, "Daken TP", hits.daken, setup.ammo_delay, setup.stp, false, daken_tp);
        ctx.record_equation(
            "Expected TP from landed attacks",
            "Expected Melee and Kick TP = (Expected Main Hits + Expected Offhand Hits + Expected Kicks) × Ordinary TP per Landed Hit\nExpected Zanshin TP = Expected Zanshin Hits × Zanshin TP per Landed Hit\nExpected Daken TP = Expected Daken Hits × Ammo-delay TP per Landed Hit\nExpected Attack TP = max(0, Expected Melee and Kick TP + Expected Zanshin TP + Expected Daken TP)",
            "Melee/kick expected landed hits = {main_hits} + {sub_hits} + {kick_hits}, TP hit delay = {delay}, Store TP Input = {stp}: melee/kick TP = {melee_tp}; expected Zanshin hits = {zanshin_hits}, SAM bonus active = {sam}: Zanshin TP = {zanshin_tp}; expected Daken hits = {daken_hits}, ammo-only delay = {ammo_delay}: Daken TP = {daken_tp}; attack TP = max(0, {melee_tp} + {zanshin_tp} + {daken_tp}) = {attack_tp}",
            &[("main_hits", hits.main), ("sub_hits", hits.sub), ("kick_hits", hits.kick),
                ("delay", setup.hit_delay()), ("stp", setup.stp), ("melee_tp", melee_tp),
                ("zanshin_hits", hits.zanshin), ("sam", flag(player.main_job == Job::Sam)), ("zanshin_tp", zanshin_tp),
                ("daken_hits", hits.daken), ("ammo_delay", setup.ammo_delay), ("daken_tp", daken_tp), ("attack_tp", tp_return)],
        );
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
        ctx.record_equation(
            "Haste and seconds per attack round",
            "Timing Base Delay = Main Delay + Offhand Delay (offhand omitted for single-wield and H2H)\nCapped Gear Haste = min(Gear Haste, 256 / 1024)\nCapped Magic Haste = min(Magic Haste, 448 / 1024)\nCapped JA Haste = min(JA Haste, 256 / 1024)\nReduced Timing Delay = (Timing Base Delay − Martial Arts) × (1 − Dual Wield / 100) × (1 − Capped Gear Haste − Capped Magic Haste − Capped JA Haste)\nSeconds per Round = max(0, max(Reduced Timing Delay, 0.2 × Timing Base Delay) / 60)\nHaste affects Seconds per Round, not the TP-generating delay or TP per landed hit.",
            "timing base delay B = {delay1}+{delay2} (offhand delay is omitted for single-wield and H2H); g=min({gear_haste},256/1024), m=min({magic_haste},448/1024), j=min({ja_haste},256/1024), with upper caps only; reduced delay = (B-{martial_arts})*(1-{dual_wield}/100)*(1-g-m-j); seconds per round = max(0,max(reduced delay,0.2*B)/60) = {seconds}. Haste changes elapsed time, not TP hit delay.",
            &[("delay1", stats.get(Stat::Delay1)), ("delay2", timing_delay2),
                ("gear_haste", stats.get(Stat::GearHaste)), ("magic_haste", stats.get(Stat::MagicHaste)),
                ("ja_haste", stats.get(Stat::JaHaste)), ("martial_arts", stats.get(Stat::MartialArts)),
                ("dual_wield", stats.get(Stat::DualWield)), ("seconds", seconds_per_round)],
        );
        ctx.record_equation(
            "Regain and total expected TP",
            "Regain per 3 Seconds = Sheet Regain + Sheet Dual Wield × I(Main Weapon is Gokotai)\nContinuous Regain TP per Round = (Seconds per Round / 3) × Regain per 3 Seconds\nTotal Expected TP per Round = Expected Attack TP + Continuous Regain TP per Round",
            "Regain per 3 seconds = {regain_stat} + {dual_wield} × {gokotai} = {regain}; Regain TP = ({seconds} / 3) × {regain} = {regain_tp}; TP per round = {attack_tp} + {regain_tp} = {tp}. Only attack TP was clamped to at least zero, before adding Regain. This is continuous proportional Regain, not the article's discrete 3-second ticks; neither tick count nor Regain TP is rounded.",
            &[("regain_stat", stats.get(Stat::Regain)), ("dual_wield", stats.get(Stat::DualWield)),
                ("gokotai", flag(gokotai)), ("regain", regain), ("seconds", seconds_per_round),
                ("regain_tp", regain_tp), ("attack_tp", attack_tp), ("tp", tp_return)],
        );
        if tp_return == 0.0 {
            ctx.record_equation(
                "Rounds and time to WS threshold",
                "TP Gap = WS Threshold − Starting TP\nRounds to WS = undefined when Total TP per Round = 0\nTime to WS = 9999 seconds when Total TP per Round = 0 (legacy sentinel, not an elapsed-time estimate)",
                "TP Gap = {threshold} − {starting_tp}, with no negative-gap clamp; TP per round = 0, so rounds to threshold are undefined and the legacy time-to-WS sentinel is {time} seconds. The sentinel is not a whole-round or discrete-tick calculation.",
                &[("starting_tp", starting_tp), ("threshold", threshold), ("time", time_to_ws)],
            );
        } else {
            let rounds_to_ws = (threshold - starting_tp) / tp_return;
            ctx.record_equation(
                "Rounds and time to WS threshold",
                "TP Gap = WS Threshold − Starting TP\nContinuous Expected Rounds to WS = TP Gap / Total Expected TP per Round\nTime to WS = Seconds per Round × Continuous Expected Rounds to WS",
                "TP Gap = {threshold} − {starting_tp}; continuous expected rounds to WS = ({threshold} − {starting_tp}) / {tp} = {rounds}; time to WS = {seconds} × {rounds} = {time} seconds. This model does not clamp a negative TP gap or round up swings or rounds as in the article's discrete examples; its Regain is continuous rather than discrete ticks.",
                &[("threshold", threshold), ("starting_tp", starting_tp), ("tp", tp_return),
                    ("rounds", rounds_to_ws), ("seconds", seconds_per_round), ("time", time_to_ws)],
            );
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
        ctx.record_equation(
            "Melee legacy pDIF bounds",
            "Attack Ratio = Attack / Target Defense\nCritical-adjusted Ratio w = Attack Ratio + min(Critical Hit Rate, 1)\nUpper Bound U(w) and Lower Bound L(w) are the evaluator's piecewise melee pDIF bounds below.",
            "Upper Bound U(w): w+0.5 for 0<=w<0.5; 1 for 0.5<=w<0.7; w+0.3 for 0.7<=w<1.2; 1.25×w for 1.2<=w<1.5; otherwise w+0.375. Lower Bound L(w): 0 for 0<=w<0.38; (1176/1024)×w−448/1024 for 0.38<=w<1.25; 1 for 1.25<=w<1.51; (1176/1024)×w−755/1024 for 1.51<=w<2.44; otherwise w−0.375.",
            &[],
        );
        ctx.record_equation(
            "Melee PDL, mean spread, and average-hit formula",
            "PDL Cap = (Weapon-skill Cap + PDL Trait) × (1 + PDL Gear)\nLegacy average pDIF = (clamp((Upper Bound + Lower Bound) / 2, 0, PDL Cap) + min(Critical Hit Rate, 1)) × Mean Damage Spread\nExpected critical-damage multiplier = 1 + min(Critical Hit Rate, 1) × min(Critical Damage Bonus, 1)\nAverage landed-hit damage = trunc(Weapon Base Damage + fSTR) × legacy average pDIF × expected critical-damage multiplier",
            "Weapon-skill Cap = 3.25 for Katana/Dagger/Sword/Axe/Club, 3.5 for Great Katana/H2H, 3.75 for Great Sword/Staff/Great Axe/Polearm, otherwise 4. Mean Damage Spread = 1.025, the mean of [random melee damage spread: U(1,1.05)]. The legacy average pDIF inserts fractional critical chance into both the bound ratio and the post-cap addition; this is a midpoint approximation, not the sampled distribution's exact expectation. No truncation follows pDIF or expected-hit weighting.",
            &[],
        );
        ctx.record_calculation(
            "Symbolic sampled melee pDIF counterpart",
            "Icrit = I([random critical roll: U(0,1)] < critical chance); ws = attack/defense+Icrit; sampled pDIF = (clip([random melee pDIF roll: U(L(ws),U(ws))],0,cap)+Icrit)*[random melee damage spread: U(1,1.05)]. No such rolls are added or evaluated by these analytical trace steps.",
            &[],
        );
        ctx.record_equation(
            "Expected weapon proc modifiers",
            "Expected weapon multiplier = 1 + (Proc Damage Multiplier − 1) × Proc Probability\nThe expected Empyrean, Relic, Prime, and Dragon Fangs modifiers are additional damage multipliers, not Base Damage or pDIF.",
            "Empyrean matching weapon = {empyrean_weapon}, Aftermath tier = {aftermath}: multiplier = 1 + 2 × AM proc probability (0.3/0.4/0.5 for active tiers 1/2/3, otherwise inactive) = {empyrean}; proc uses [random aftermath proc roll: U(0,1)]. Relic multiplier = 1 + 2 × 0.13, 1 + 1.5 × 0.16, or 1 + 0.2 for the matching relic tier, otherwise 1 = {relic}; proc uses [random relic proc roll: U(0,1)]. Prime multiplier = 1 + 2 × 0.3 for V, 1 + 0.3 for IV, otherwise 1 = {prime}; proc uses [random Prime proc roll: U(0,1)]. Dragon Fangs kick multiplier = 1 + 0.2 on matching weapon, otherwise 1 = {dragon_fangs}. These are prepared average modifiers, not additional proc rolls.",
            &[("empyrean_weapon", flag(setup.empyrean)), ("aftermath", setup.aftermath),
                ("empyrean", setup.empyrean_bonus), ("relic", setup.relic_bonus),
                ("prime", setup.prime_bonus), ("dragon_fangs", setup.dragon_fangs_bonus)],
        );
        ctx.record_equation(
            "Main-hand legacy average pDIF",
            "Critical-adjusted Ratio w = Main-hand Attack / Target Defense + min(Critical Hit Rate, 1)\nPDL Cap = (Weapon-skill Cap + PDL Trait) × (1 + PDL Gear)\nLegacy average pDIF = (clamp((U(w) + L(w)) / 2, 0, PDL Cap) + min(Critical Hit Rate, 1)) × 1.025",
            "w = {attack} / {defense} + min({crit_rate}, 1); PDL Cap = (Weapon-skill Cap + {pdl_trait}) × (1 + {pdl_gear}); legacy average pDIF = (clamp((U(w) + L(w)) / 2, 0, PDL Cap) + min({crit_rate}, 1)) × 1.025 = {pdif}. This is the legacy analytical midpoint approximation of [random pDIF roll], not an exact sampled expectation.",
            &[("attack", setup.attack1), ("defense", setup.enemy_defense), ("pdl_trait", setup.pdl_trait),
                ("pdl_gear", setup.pdl_gear), ("crit_rate", setup.crit_rate), ("pdif", main_pdif)],
        );
        ctx.record_equation(
            "Main-hand expected physical damage",
            "Main-hand Base Damage = Weapon Base Damage + fSTR\nAverage main-hand landed-hit damage = trunc(Main-hand Base Damage) × legacy average pDIF × expected critical-damage multiplier\nModified ordinary main-hand hit = Average main-hand landed-hit damage × expected Empyrean multiplier\nOrdinary main-hand component = Expected Main-hand Landed Hits × Modified ordinary main-hand hit",
            "Average landed hit = trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {base}; hit after Empyrean modifier = {base} × {empyrean} = {hit}; ordinary main-hand component = {hits} expected landed hits × {hit} = {total}. Hit probabilities are already included in the landed-hit count. Verethragna's ordinary-hit Empyrean modifier is 1.",
            &[("damage", setup.main_damage), ("fstr", setup.fstr_main), ("pdif", main_pdif),
                ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage), ("base", main_base_damage),
                ("empyrean", main_empyrean_multiplier), ("hit", main_hit_damage),
                ("hits", hits.main), ("total", main_expected_damage)],
        );
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
        ctx.record_equation(
            "First-hit ability bonuses",
            "Sneak Attack bonus = DEX × (1 + Sneak Attack Bonus / 100), for active Sneak Attack on THF\nTrick Attack bonus = AGI × (1 + Trick Attack Bonus / 100), for active Trick Attack on THF\nFlourish CHR Multiplier = 1 + Flourish CHR Bonus / 100\nClimactic Flourish bonus = 0.5 × CHR × Flourish CHR Multiplier, when active\nStriking or Ternary Flourish bonus = CHR × Flourish CHR Multiplier, when active\nEach inactive or inapplicable ability contributes 0.",
            "Sneak Attack = {dex} × (1 + {sneak_percent} / 100) × {sneak_active} × {thf} = {sneak}; Trick Attack = {agi} × (1 + {trick_percent} / 100) × {trick_active} × {thf} = {trick}; Flourish CHR factor C = 1 + {chr_percent} / 100; Climactic = 0.5 × {chr} × C × {climactic_active} = {climactic}; Striking = {chr} × C × {striking_active} = {striking}; Ternary = {chr} × C × {ternary_active} = {ternary}",
            &[("dex", stats.get(Stat::Dex)), ("sneak_percent", stats.get(Stat::SneakAttackBonus)),
                ("sneak_active", flag(sneak)), ("thf", flag(player.main_job == Job::Thf)), ("sneak", sneak_bonus),
                ("agi", stats.get(Stat::Agi)), ("trick_percent", stats.get(Stat::TrickAttackBonus)),
                ("trick_active", flag(trick)), ("trick", trick_bonus), ("chr_percent", stats.get(Stat::FlourishChrPct)),
                ("chr", stats.get(Stat::Chr)), ("climactic_active", flag(climactic)), ("climactic", climactic_bonus),
                ("striking_active", flag(striking)), ("striking", striking_bonus),
                ("ternary_active", flag(ternary)), ("ternary", ternary_bonus)],
        );
        ctx.record_equation(
            "First-hit critical adjustment",
            "First-hit Critical Rate = 1 for Sneak Attack, Trick Attack, or Climactic Flourish; otherwise Ordinary Critical Hit Rate + Striking Critical Rate Bonus × I(Ordinary Critical Hit Rate > 0)\nFirst-hit Critical Damage Bonus = (Ordinary Critical Damage Bonus + Vajra bonus) × (1 + Climactic Critical Damage Bonus)\nExpected first-hit critical-damage multiplier = 1 + min(First-hit Critical Rate, 1) × min(First-hit Critical Damage Bonus, 1)\nLegacy first-hit average pDIF uses the same midpoint equation with First-hit Critical Rate substituted.",
            "First-hit Critical Rate = 1 for Sneak/Trick/Climactic, otherwise {crit_rate} + {striking_crit} × I({crit_rate} > 0) = {first_crit}; First-hit Critical Damage Bonus = ({crit_damage} + {vajra}) × (1 + {climactic_crit}) = {adjusted_crit}; legacy first-hit average pDIF = {pdif}",
            &[("crit_rate", setup.crit_rate), ("striking_crit", striking_crit_rate),
                ("first_crit", first_crit_rate), ("crit_damage", setup.crit_damage),
                ("vajra", vajra_crit_damage), ("climactic_crit", climactic_crit_damage),
                ("adjusted_crit", adjusted_crit_damage), ("pdif", first_pdif)],
        );
        ctx.record_equation(
            "First-hit average physical damage",
            "First-hit Base Damage = Weapon Base Damage + fSTR + Sneak Attack bonus + Trick Attack bonus + Climactic Flourish bonus + Striking Flourish bonus + Ternary Flourish bonus\nFirst-hit average physical damage = trunc(First-hit Base Damage) × legacy first-hit average pDIF × expected first-hit critical-damage multiplier",
            "First-hit Base Damage = {damage} + {fstr} + {sneak} + {trick} + {climactic} + {striking} + {ternary}; first average hit = trunc(First-hit Base Damage) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}. The base truncates before pDIF; there is no final damage truncation.",
            &[("damage", setup.main_damage), ("fstr", setup.fstr_main), ("sneak", sneak_bonus),
                ("trick", trick_bonus), ("climactic", climactic_bonus), ("striking", striking_bonus),
                ("ternary", ternary_bonus), ("pdif", first_pdif), ("crit_rate", first_crit_rate),
                ("crit_damage", adjusted_crit_damage), ("hit", first_base_damage)],
        );
        ctx.record_equation(
            "First-hit weapon modifiers and correction",
            "Modified first hit = First-hit average physical damage × expected Relic multiplier × expected Empyrean multiplier × expected Prime multiplier\nFirst-hit correction = Modified first hit × Local First-hit Probability − Modified ordinary main-hand hit × Local First-hit Probability\nCorrected main-hand component = Ordinary main-hand component + First-hit correction",
            "First average hit before weapon modifiers = {base}; modified first hit = {base} × Relic {relic} × Empyrean {empyrean} × Prime {prime} = {first}; replace ordinary first hit: {first} × {first_hit_rate} − {ordinary} × {first_hit_rate} = {correction}; running physical subtotal = {subtotal}. Sneak/Trick force this local hit rate only; the expected-hit matrix and its ordinary hit weighting remain unchanged.",
            &[("base", first_base_damage), ("relic", setup.relic_bonus), ("empyrean", setup.empyrean_bonus),
                ("prime", setup.prime_bonus), ("first", first_damage), ("first_hit_rate", hit_rate11),
                ("ordinary", main_hit_damage), ("correction", first_hit_correction), ("subtotal", physical_damage)],
        );
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
            ctx.record_equation(
                "Striking Flourish additional-hit correction",
                "Additional Striking hit = trunc(First-hit Base Damage) × legacy Striking average pDIF × expected Striking critical-damage multiplier\nStriking correction = (Additional Striking hit − Modified ordinary main-hand hit) × Local First-hit Probability\nThis additional hit uses the ability-augmented base, ordinary Critical Damage Bonus, and no first-hit weapon multipliers.",
                "Additional-hit Critical Rate = {crit_rate}; legacy average pDIF = {pdif}; additional average hit = trunc(First-hit Base Damage) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}; correction = ({hit} − {ordinary}) × {first_hit_rate} = {correction}; running physical subtotal = {subtotal}",
                &[("crit_rate", striking_rate), ("pdif", striking_pdif), ("crit_damage", setup.crit_damage),
                    ("hit", striking_damage), ("ordinary", main_hit_damage), ("first_hit_rate", hit_rate11),
                    ("correction", striking_correction), ("subtotal", physical_damage)],
            );
        }
    }
    let offhand_pdif = get_avg_pdif_melee(setup.attack2, setup.sub_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let offhand_damage = average_hit(setup.sub_damage, setup.fstr_sub, offhand_pdif, setup.crit_rate, setup.crit_damage)?;
    let offhand_expected_damage = offhand_damage * hits.sub;
    physical_damage += offhand_expected_damage;
    if tracing && setup.dual_wield {
        ctx.record_equation(
            if setup.main_skill == SkillType::HandToHand { "Second-fist expected physical damage" } else { "Offhand expected physical damage" },
            "Offhand / second-fist Base Damage = Weapon Base Damage + fSTR\nAverage offhand / second-fist landed-hit damage = trunc(Base Damage) × legacy average melee pDIF × expected critical-damage multiplier\nExpected offhand / second-fist component = Average landed-hit damage × Expected Offhand / Second-fist Landed Hits\nLegacy average melee pDIF uses the offhand / second-fist Attack and Weapon-skill Cap in the midpoint equation above.",
            "Attack Ratio = {attack} / {defense}; PDL Cap = (Weapon-skill Cap + {pdl_trait}) × (1 + {pdl_gear}); Critical Hit Rate = {crit_rate}; legacy average pDIF = {pdif}; average landed hit = trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}; offhand / second-fist component = {hit} × {hits} expected landed hits = {total}; running physical subtotal = {subtotal}",
            &[("attack", setup.attack2), ("defense", setup.enemy_defense), ("pdl_trait", setup.pdl_trait),
                ("pdl_gear", setup.pdl_gear), ("crit_rate", setup.crit_rate), ("pdif", offhand_pdif),
                ("damage", setup.sub_damage), ("fstr", setup.fstr_sub), ("crit_damage", setup.crit_damage),
                ("hit", offhand_damage), ("hits", hits.sub), ("total", offhand_expected_damage),
                ("subtotal", physical_damage)],
        );
    }
    let melee_before_multiattack = physical_damage;
    physical_damage *= (1.0 + setup.da_damage * setup.da) * (1.0 + setup.ta_damage * setup.ta);
    let melee_damage = physical_damage;
    if tracing {
        ctx.record_equation(
            "Expected DA/TA damage modifiers",
            "Expected DA Damage Multiplier = 1 + DA Damage Bonus × DA Rate\nExpected TA Damage Multiplier = 1 + TA Damage Bonus × TA Rate\nExpected melee component = Corrected main-hand and offhand subtotal × (Expected DA Damage Multiplier × Expected TA Damage Multiplier)\nThese are legacy weighted damage modifiers, separate from the plain per-landed-hit physical formula and the expected-hit count.",
            "Melee physical subtotal = {before} × ((1 + {da_damage} × {da}) × (1 + {ta_damage} × {ta})) = {after}; these modifiers apply before kick, Daken, and Zanshin damage",
            &[("before", melee_before_multiattack), ("da_damage", setup.da_damage), ("da", setup.da),
                ("ta_damage", setup.ta_damage), ("ta", setup.ta), ("after", melee_damage)],
        );
    }
    let kick_pdif = get_avg_pdif_melee(setup.attack1 + stats.get(Stat::KickAttacksAttack),
        setup.main_skill.into(), setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let mut kick_damage = average_hit(setup.kick_damage, setup.fstr_kick, kick_pdif, setup.crit_rate, setup.crit_damage)?;
    let kick_base_damage = kick_damage;
    kick_damage *= setup.dragon_fangs_bonus;
    let kick_expected_damage = kick_damage * hits.kick;
    physical_damage += kick_expected_damage;
    if tracing && (setup.kickattacks != 0.0 || hits.kick != 0.0) {
        ctx.record_equation(
            "Kick expected physical damage",
            "Kick Attack = Main-hand Attack + Kick Attacks Attack\nKick Weapon Rank = Kick Damage / 9\nKick fSTR = clamp(adjusted dSTR / 4, −Kick Weapon Rank, 8 + Kick Weapon Rank)\nKick Base Damage = Kick Damage + Kick fSTR\nLegacy kick average pDIF uses Kick Attack and the main-hand Weapon-skill Cap in the midpoint equation above.\nAverage landed kick = trunc(Kick Base Damage) × legacy kick average pDIF × expected critical-damage multiplier\nModified kick = Average landed kick × expected Dragon Fangs multiplier\nExpected kick component = Modified kick × Expected Landed Kicks",
            "Kick dSTR = {strength} − {vitality}; Kick fSTR = clamp(F(dSTR) / 4, −{damage} / 9, 8 + {damage} / 9) = {fstr}; Kick Attack = {attack} + {kick_attack}; legacy average melee pDIF = {pdif}; average landed kick = trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {base}; modified kick = {base} × {dragon_fangs} = {hit}; kick component = {hit} × {hits} expected landed kicks = {total}",
            &[("damage", setup.kick_damage), ("strength", stats.get(Stat::Str)),
                ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_kick),
                ("attack", setup.attack1), ("kick_attack", stats.get(Stat::KickAttacksAttack)),
                ("pdif", kick_pdif), ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage),
                ("base", kick_base_damage), ("dragon_fangs", setup.dragon_fangs_bonus),
                ("hit", kick_damage), ("hits", hits.kick), ("total", kick_expected_damage)],
        );
    }
    let daken_pdif = get_avg_pdif_ranged(setup.ranged_attack, setup.ammo_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let daken_damage = average_hit(setup.ammo_damage, setup.fstr_ammo, daken_pdif, setup.crit_rate, setup.crit_damage)?;
    let daken_expected_damage = divide(hits.daken * daken_damage, setup.empyrean_bonus, OPERATION, "empyrean_am_damage_bonus")?;
    physical_damage += daken_expected_damage;
    if tracing && (setup.daken != 0.0 || hits.daken != 0.0) {
        ctx.record_equation(
            "Daken ranged base and legacy pDIF",
            "Daken Weapon Rank = Ammo Damage / 9\nLimited dSTR = clamp(STR − Target VIT, −2 × (7 + 2 × Weapon Rank), 2 × (14 + 2 × Weapon Rank))\nfSTR2 = clamp(adjusted Limited dSTR / 2, −2 × Weapon Rank, 2 × (Weapon Rank + 8))\nDaken Base Damage = Ammo Damage + fSTR2\nRanged Attack Ratio r = Ranged Attack / Target Defense\nLegacy average ranged pDIF = clamp((Ranged Lower Bound + Ranged Upper Bound) / 2, 0, Ranged PDL Cap) × (1 + 0.25 × min(Critical Hit Rate, 1))",
            "Daken Weapon Rank = {damage} / 9; limited dSTR = clamp({strength} − {vitality}, −2 × (7 + 2 × rank), 2 × (14 + 2 × rank)); fSTR2 = clamp(F(limited dSTR) / 2, −2 × rank, 2 × (rank + 8)) = {fstr}, with no rank or fSTR2 flooring. r = {attack} / {defense}; ranged bounds L,U = r,(10/9)×r for 0<=r<0.9; 1,1 for 0.9<=r<1.1; otherwise (20/19)×r−3/19,r. Ranged PDL Cap = (3.5 for Marksmanship, otherwise 3.25, plus {pdl_trait}) × (1 + {pdl_gear}); legacy ranged pDIF = clamp((L + U) / 2, 0, cap) × (1 + 0.25 × min({crit_rate}, 1)) = {pdif}. Symbolic sampled pDIF = clamp([random ranged pDIF roll: U(L,U)], 0, cap) × (1 + 0.25 × I([random ranged critical roll: U(0,1)] < {crit_rate})), with no melee spread multiplier. The analytical value is a capped midpoint approximation, not an exact sampled expectation.",
            &[("damage", setup.ammo_damage), ("strength", stats.get(Stat::Str)),
                ("vitality", enemy.stats.get(Stat::Vit)), ("fstr", setup.fstr_ammo),
                ("attack", setup.ranged_attack), ("defense", setup.enemy_defense),
                ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear),
                ("crit_rate", setup.crit_rate), ("pdif", daken_pdif)],
        );
        ctx.record_equation(
            "Daken expected physical damage",
            "Daken Base Damage = Ammo Damage + fSTR2\nAverage landed Daken damage = trunc(Daken Base Damage) × legacy average ranged pDIF × expected critical-damage multiplier\nExpected Daken component = (Expected Landed Daken Attacks × Average landed Daken damage) / expected Empyrean multiplier\nThe Empyrean division is a legacy analytical adjustment, not part of the BG Wiki per-hit physical equation.",
            "Average landed Daken damage = trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {hit}; Daken component = ({hits} expected landed Daken × {hit}) / {empyrean} = {total}. The ranged critical pDIF factor and the additional expected critical-damage multiplier remain separate.",
            &[("damage", setup.ammo_damage), ("fstr", setup.fstr_ammo), ("pdif", daken_pdif),
                ("crit_rate", setup.crit_rate), ("crit_damage", setup.crit_damage), ("hit", daken_damage),
                ("hits", hits.daken), ("empyrean", setup.empyrean_bonus), ("total", daken_expected_damage)],
        );
    }
    let zanshin_pdif = get_avg_pdif_melee(setup.attack1 + stats.get(Stat::ZanshinAttack),
        setup.main_skill.into(), setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let mut zanshin_damage = average_hit(setup.main_damage, setup.fstr_main, zanshin_pdif, setup.crit_rate, setup.crit_damage)?;
    let zanshin_base_damage = zanshin_damage;
    zanshin_damage *= setup.empyrean_bonus;
    let zanshin_expected_damage = hits.zanshin * zanshin_damage;
    physical_damage += zanshin_expected_damage;
    if tracing && (setup.zanshin != 0.0 || setup.zanhasso != 0.0 || hits.zanshin != 0.0) {
        ctx.record_equation(
            "Zanshin expected physical damage",
            "Zanshin Attack = Main-hand Attack + Zanshin Attack Bonus\nZanshin fSTR = Main-hand fSTR\nZanshin Base Damage = Weapon Base Damage + Zanshin fSTR\nLegacy Zanshin average pDIF uses Zanshin Attack and the main-hand Weapon-skill Cap in the midpoint equation above.\nAverage landed Zanshin damage = trunc(Zanshin Base Damage) × legacy Zanshin average pDIF × expected critical-damage multiplier\nModified Zanshin hit = Average landed Zanshin damage × expected Empyrean multiplier\nExpected Zanshin component = Expected Landed Zanshin Attacks × Modified Zanshin hit",
            "Zanshin Attack = {attack} + {zanshin_attack}; Zanshin Base Damage = {damage} + {fstr}; legacy average melee pDIF = {pdif}; average landed Zanshin damage = trunc({damage} + {fstr}) × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {base}; modified Zanshin hit = {base} × {empyrean} = {hit}; Zanshin component = {hits} expected landed Zanshin × {hit} = {total}",
            &[("attack", setup.attack1), ("zanshin_attack", stats.get(Stat::ZanshinAttack)), ("pdif", zanshin_pdif),
                ("damage", setup.main_damage), ("fstr", setup.fstr_main), ("crit_rate", setup.crit_rate),
                ("crit_damage", setup.crit_damage), ("base", zanshin_base_damage), ("empyrean", setup.empyrean_bonus),
                ("hit", zanshin_damage), ("hits", hits.zanshin), ("total", zanshin_expected_damage)],
        );
    }
    let damage = physical_damage + magical_damage;
    if tracing {
        ctx.record_equation(
            "Physical and magical damage totals",
            "Expected Physical Damage per Round = Expected melee component + Expected kick component + Expected Daken component + Expected Zanshin component\nExpected Total Damage per Round = Expected Physical Damage per Round + Expected EnSpell Damage",
            "Physical Damage = melee {melee} + kicks {kick} + Daken {daken} + Zanshin {zanshin} = {physical}; Total Damage per Round = {physical} + {magical} = {damage}",
            &[("melee", melee_damage), ("kick", kick_expected_damage), ("daken", daken_expected_damage),
                ("zanshin", zanshin_expected_damage), ("physical", physical_damage),
                ("magical", magical_damage), ("damage", damage)],
        );
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
            ctx.record_calculation(
                "Expected round DPS",
                "DPS = {damage}/0 is undefined; seconds per round = 0. Selecting the DPS metric retains the existing division-by-zero error.",
                &[("damage", damage)],
            );
        } else {
            ctx.record_calculation(
                "Expected round DPS",
                "DPS = {damage}/{seconds} = {dps}",
                &[("damage", damage), ("seconds", seconds_per_round), ("dps", damage / seconds_per_round)],
            );
        }
        let (equation, formula) = match metric.name.as_ref() {
            "Damage dealt" => ("Selected Metric = Expected Damage per Round",
                "selected metric = damage per round {damage} = {value}; maximize (invert = {invert})"),
            "TP return" => ("Selected Metric = Total Expected TP per Round",
                "selected metric = TP per round {tp} = {value}; maximize (invert = {invert})"),
            "DPS" => ("Selected Metric = Expected Damage per Round / Seconds per Round",
                "selected metric = {damage}/{seconds} = {value}; maximize (invert = {invert})"),
            "TP > Damage" => ("Selected Metric = Expected Damage per Round × (Total Expected TP per Round)² / 10000",
                "selected metric = {damage}*{tp}*{tp}/10000 = {value}; maximize (invert = {invert})"),
            "Damage > TP" => ("Selected Metric = (Expected Damage per Round)² × Total Expected TP per Round / 1000000",
                "selected metric = {damage}*{damage}*{tp}/1000000 = {value}; maximize (invert = {invert})"),
            _ => ("Selected Metric = Time to WS (legacy fallback)\nTime to WS = Seconds per Round × ((WS Threshold − Starting TP) / Total Expected TP per Round); 9999 seconds if Total Expected TP per Round = 0",
                "selected metric uses the legacy time-to-WS fallback: {time} seconds = {value}; minimize (invert = {invert})"),
        };
        ctx.record_equation(
            "Selected optimization metric",
            equation,
            formula,
            &[("damage", damage), ("tp", tp_return), ("seconds", seconds_per_round),
                ("time", time_to_ws), ("value", value), ("invert", invert as f64)],
        );
    }
    Ok(ActionResult { damage, tp_return, seconds_per_round: Some(seconds_per_round),
        time_to_ws: Some(time_to_ws), metric: value, invert, physical_damage, magical_damage })
}

#[derive(Clone, Copy)]
struct MultiattackRolls { qa: bool, ta: bool, da: bool }

impl MultiattackRolls {
    fn draw<R: RandomSource>(setup: &RoundSetup, ctx: &mut EvaluationContext<R>) -> Self {
        let qa = ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.qa;
        // Source uses multiplication rather than `and`: all three draws occur.
        let ta = (ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.ta) && !qa;
        let da = (ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.da) && !qa && !ta;
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
    let draw = ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0);
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
        if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < rate {
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
            if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < hit_rate {
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
    if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.hit_rates[0][0] {
        main_connected = true;
        let (pdif, critical) = get_pdif_melee(setup.attack1, setup.main_skill.into(),
            setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
        let mut physical = sampled_hit(setup.main_damage, setup.fstr_main, pdif, critical, setup.crit_damage)?;
        // Membership is checked AFTER each draw, even on ordinary weapons.
        physical *= 1.0 + 2.0 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < 0.13 && setup.relic30);
        physical *= 1.0 + 1.5 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < 0.16 && setup.relic25);
        physical *= 1.0 + 1.0 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < 0.2 && setup.relic20);
        physical *= 1.0 + 2.0 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < 0.3 && setup.prime3);
        physical *= 1.0 + 1.0 * flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < 0.3 && setup.prime2);
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
        if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.hit_rates[0][1] {
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
        if (!main_connected || ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.zanhasso)
            && !setup.dual_wield && TWO_HANDED_SKILLS.contains(&setup.main_skill) {
            if !main_ma_proc {
                let attack = setup.attack1 + player.stats.get(Stat::ZanshinAttack);
                if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.zanshin_oa2 {
                    zanshin_damage += bonus_swings(&setup, 2, setup.zanshin_hit_rate, attack,
                        setup.main_skill, setup.main_damage, setup.fstr_main, setup.main_enspell,
                        1.0, true, player.main_job == Job::Sam, AttackKind::Zanshin,
                        AttackProc::Occasionally(2), &mut totals, ctx)?;
                } else if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.zanshin {
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
        if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.kickattacks && setup.main_skill == SkillType::HandToHand {
            totals.attempts += 1;
            if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.hit_rates[0][0] {
                let (pdif, critical) = get_pdif_melee(setup.attack1 + player.stats.get(Stat::KickAttacksAttack),
                    setup.main_skill.into(), setup.pdl_trait, setup.pdl_gear, setup.enemy_defense,
                    setup.crit_rate, &mut ctx.random)?;
                let physical = sampled_hit(setup.kick_damage, setup.fstr_kick, pdif, critical, setup.crit_damage)?;
                // The sampled source applies this 20% double damage roll to ALL
                // H2H kicks, not only Dragon Fangs. Trace the boosted damage.
                kick_damage = physical * (1.0 + flag(ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < 0.2));
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
        if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.daken && setup.ammo_skill == SkillType::Throwing {
            totals.attempts += 1;
            if ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0) < setup.hit_rate_ranged {
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
