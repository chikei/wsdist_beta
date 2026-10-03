//! Analytical and sampled weapon-skill damage from actions.py:1354–1860.
//!
//! The analytical hit approximation and sampled eight-attempt walk deliberately
//! differ. Shared setup does not merge their first-hit, TP, or pDIF formulas.

use crate::math::{
    self, PdifSkill, WeaponBonusSkill, WeaponBonusWeapon, get_avg_pdif_melee,
    get_avg_pdif_ranged, get_avg_phys_damage, get_dstat_macc, get_fstr, get_fstr2,
    get_hit_rate, get_ma_rate3, get_magic_hit_rate, get_pdif_melee, get_pdif_ranged,
    get_phys_damage, get_resist_state, get_resist_state_average, get_tp,
    get_weapon_bonus, python_int,
};
use crate::model::{
    Ability, ActionResult, AttackKind, AttackProc, CalcError, Enemy, EvaluationContext,
    HitEvent, PieceKind, Player, RandomSource, RandomStream, SkillType, Slot, Storm,
    WsRequest, WsSample, WsType,
};
use crate::stats::Stat;
use crate::weaponskills::{WsElement, WsScaling, weaponskill_info};

use super::common::{ONE_HANDED_SKILLS, oa_list, oa_tiers};

const OPERATION: &str = "average_ws";

#[inline]
fn flag(value: bool) -> f64 { if value { 1.0 } else { 0.0 } }

// The source's `x if x < 1 else 1` is not min(x, 1): NaN selects 1.
#[inline]
fn source_rate(value: f64) -> f64 { if value < 1.0 { value } else { 1.0 } }

struct Setup {
    ws: WsScaling,
    dual_wield: bool,
    main_skill: SkillType,
    sub_skill: SkillType,
    ranged_skill: SkillType,
    qa: f64,
    ta: f64,
    da: f64,
    ftp: f64,
    ftp2: f64,
    ftp_hybrid: f64,
    crit_rate: f64,
    crit_dmg: f64,
    pdl_gear: f64,
    pdl_trait: f64,
    wsd: f64,
    ws_trait: f64,
    ws_bonus: f64,
    stp: f64,
    enemy_defense: f64,
    enemy_evasion: f64,
    mdelay: f64,
    ranged_delay: f64,
    ammo_delay: f64,
    hover_shot: f64,
    vajra: bool,
    fotia_gorget: bool,
    fotia_belt: bool,
    obi: bool,
    magic_accuracy: f64,
}

impl Setup {
    fn new<R: RandomSource>(
        player: &Player,
        enemy: &Enemy,
        request: &WsRequest,
        ctx: &EvaluationContext<R>,
    ) -> Result<Self, CalcError> {
        let tp_bonus = player.stats.get(Stat::TpBonus);
        let tp = math::python_max(1000.0, math::python_min(3000.0, request.tp + tp_bonus));
        let main = ctx.catalog.piece(player.gearset[Slot::Main.index()]);
        let sub = ctx.catalog.piece(player.gearset[Slot::Sub.index()]);
        let ranged = ctx.catalog.piece(player.gearset[Slot::Ranged.index()]);
        let main_skill = ctx.state.skill(main);
        let dual_wield = sub.kind == PieceKind::Weapon || main_skill == SkillType::HandToHand;
        let hover_shot = flag(player.abilities.enabled(Ability::HoverShot))
            * flag(request.ws_type == WsType::Ranged);
        let ws = weaponskill_info(request.skill, tp, player, enemy, &player.wsc, dual_wield, ctx)?;
        let crit_rate = source_rate(ws.crit_rate);
        let ftp = ws.ftp + player.stats.get(Stat::Ftp) / 256.0;
        let mut ftp_hybrid = ws.ftp_hybrid;
        if ws.hybrid { ftp_hybrid += player.stats.get(Stat::Ftp) / 256.0; }
        let ftp2 = if ws.ftp_rep { ftp } else { 1.0 };
        let qa = source_rate(player.stats.get(Stat::Qa) / 100.0);
        let ta = source_rate(player.stats.get(Stat::Ta) / 100.0);
        let da = source_rate(player.stats.get(Stat::Da) / 100.0);
        let pdl_gear = player.stats.get(Stat::Pdl) / 100.0;
        let pdl_trait = player.stats.get(Stat::PdlTrait) / 100.0;
        let wsd = player.stats.get(Stat::WeaponSkillDamage) / 100.0;
        let ws_trait = player.stats.get(Stat::WeaponSkillDamageTrait) / 100.0;
        let ws_bonus = get_weapon_bonus(
            WeaponBonusWeapon::from_name(&main.name2),
            WeaponBonusWeapon::from_name(&ranged.name2),
            WeaponBonusSkill::from_name(request.skill.name()),
        );
        let crit_dmg = player.stats.get(Stat::CritDamage) / 100.0;
        let stp = player.stats.get(Stat::StoreTp) / 100.0;
        let enemy_defense = math::python_max(1.0, ws.enemy_def);
        let enemy_evasion = enemy.stats.require(Stat::Evasion, OPERATION)?;
        let base_delay = (player.stats.require(Stat::Delay1, OPERATION)?
            + player.stats.require(Stat::Delay2, OPERATION)?) / 2.0;
        let mdelay = (base_delay - player.stats.get(Stat::MartialArts))
            * (1.0 - player.stats.get(Stat::DualWield) / 100.0);
        let ranged_delay = player.stats.get(Stat::RangedDelay);
        let ammo_delay = player.stats.get(Stat::AmmoDelay);
        let sub_skill = if main_skill == SkillType::HandToHand {
            SkillType::HandToHand
        } else {
            ctx.state.skill(sub)
        };
        let magic_accuracy = player.stats.get(Stat::MagicAccuracy)
            + player.stats.get(Stat::MainMagicAccuracySkill)
            + 100.0 * flag(player.abilities.enabled(Ability::HoverShot))
                * flag(request.ws_type == WsType::Ranged);
        let neck = ctx.catalog.piece(player.gearset[Slot::Neck.index()]);
        let waist = ctx.catalog.piece(player.gearset[Slot::Waist.index()]);
        Ok(Self {
            ws, dual_wield, main_skill, sub_skill, ranged_skill: ctx.state.skill(ranged),
            qa, ta, da, ftp, ftp2, ftp_hybrid, crit_rate, crit_dmg, pdl_gear, pdl_trait,
            wsd, ws_trait, ws_bonus, stp, enemy_defense, enemy_evasion, mdelay,
            ranged_delay, ammo_delay, hover_shot, vajra: main.name.as_ref() == "Vajra",
            fotia_gorget: neck.name.as_ref() == "Fotia Gorget",
            fotia_belt: waist.name.as_ref() == "Fotia Belt",
            obi: waist.name.as_ref() == "Hachirin-no-Obi", magic_accuracy,
        })
    }

    fn melee_tp_delay(&self) -> f64 {
        if self.main_skill == SkillType::HandToHand { self.mdelay / 2.0 } else { self.mdelay }
    }
}

struct FirstHit {
    striking: bool,
    ternary: bool,
    sneak: bool,
    trick: bool,
    sneak_bonus: f64,
    trick_bonus: f64,
    climactic_bonus: f64,
    striking_bonus: f64,
    ternary_bonus: f64,
    striking_crit_rate: f64,
    crit_rate: f64,
    crit_dmg: f64,
}

impl FirstHit {
    // Analytical bonuses are always active; sampled bonuses require single=true.
    fn new(player: &Player, setup: &Setup, enabled: bool) -> Result<Self, CalcError> {
        let striking = enabled && player.abilities.enabled(Ability::StrikingFlourish);
        let ternary = enabled && player.abilities.enabled(Ability::TernaryFlourish);
        let climactic = enabled && player.abilities.enabled(Ability::ClimacticFlourish);
        let sneak = enabled && player.abilities.enabled(Ability::SneakAttack);
        let trick = enabled && player.abilities.enabled(Ability::TrickAttack);
        let sneak_bonus = (player.stats.require(Stat::Dex, OPERATION)?
            * (1.0 + player.stats.get(Stat::SneakAttackBonus) / 100.0)) * flag(sneak);
        let trick_bonus = (player.stats.require(Stat::Agi, OPERATION)?
            * (1.0 + player.stats.get(Stat::TrickAttackBonus) / 100.0)) * flag(trick);
        let chr = player.stats.require(Stat::Chr, OPERATION)?;
        let flourish_chr = 1.0 + player.stats.get(Stat::FlourishChrPct) / 100.0;
        let climactic_bonus = (0.5 * chr * flourish_chr) * flag(climactic);
        let striking_bonus = (1.0 * chr * flourish_chr) * flag(striking);
        let ternary_bonus = (1.0 * chr * flourish_chr) * flag(ternary);
        let climactic_crit_dmg = player.stats.get(Stat::ClimacticCritDamage) / 100.0 * flag(climactic);
        let striking_crit_rate = player.stats.get(Stat::StrikingCritRate) / 100.0 * flag(striking);
        let vajra_crit_dmg = 0.3 * flag(setup.vajra && (sneak || trick));
        let crit_rate = source_rate(if sneak || trick || climactic {
            1.0
        } else {
            setup.crit_rate + striking_crit_rate * flag(setup.crit_rate > 0.0)
        });
        let crit_dmg = (setup.crit_dmg + vajra_crit_dmg) * (1.0 + climactic_crit_dmg);
        Ok(Self { striking, ternary, sneak, trick, sneak_bonus, trick_bonus, climactic_bonus,
            striking_bonus, ternary_bonus, striking_crit_rate, crit_rate, crit_dmg })
    }
}

struct Melee {
    main_dmg: f64,
    sub_dmg: f64,
    fstr_main: f64,
    fstr_sub: f64,
    hit11: f64,
    hit12: f64,
    hit21: f64,
    hit22: f64,
}

impl Melee {
    fn new(player: &Player, enemy: &Enemy, request: &WsRequest, setup: &Setup) -> Result<Self, CalcError> {
        let footwork_kick = player.abilities.enabled(Ability::Footwork)
            && matches!(request.skill.name(), "Tornado Kick" | "Dragon Kick");
        let main_dmg = player.stats.require(if footwork_kick { Stat::KickDmg } else { Stat::Dmg1 }, OPERATION)?;
        let sub_dmg = player.stats.require(if footwork_kick { Stat::KickDmg } else { Stat::Dmg2 }, OPERATION)?;
        let fstr_main = get_fstr(main_dmg, player.stats.require(Stat::Str, OPERATION)?, enemy.stats.require(Stat::Vit, OPERATION)?);
        let fstr_sub = get_fstr(sub_dmg, player.stats.require(Stat::Str, OPERATION)?, enemy.stats.require(Stat::Vit, OPERATION)?);
        let accuracy1 = setup.ws.player_accuracy1 + player.stats.get(Stat::WeaponSkillAccuracy);
        let accuracy2 = setup.ws.player_accuracy2 + player.stats.get(Stat::WeaponSkillAccuracy);
        let main_cap = if ONE_HANDED_SKILLS.contains(&setup.main_skill)
            || setup.main_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
        let sub_cap = if setup.sub_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
        let hit11 = get_hit_rate(accuracy1 + 100.0, setup.enemy_evasion, main_cap);
        let hit12 = get_hit_rate(accuracy1, setup.enemy_evasion, main_cap);
        let mut hit21 = get_hit_rate(accuracy2 + 100.0, setup.enemy_evasion, sub_cap);
        let mut hit22 = get_hit_rate(accuracy2, setup.enemy_evasion, sub_cap);
        if !setup.dual_wield { hit21 = 0.0; hit22 = 0.0; }
        Ok(Self { main_dmg, sub_dmg, fstr_main, fstr_sub, hit11, hit12, hit21, hit22 })
    }
}

#[derive(Default)]
struct Damage {
    physical: f64,
    magical: f64,
    tp: f64,
    attempts: u8,
}

fn average_melee(
    player: &Player,
    request: &WsRequest,
    setup: &Setup,
    melee: &Melee,
    damage: &mut Damage,
) -> Result<f64, CalcError> {
    // Source makes this matrix BEFORE SA/TA change the scalar first-hit rates.
    let matrix = [[melee.hit11, melee.hit21], [melee.hit12, melee.hit22]];
    let first = FirstHit::new(player, setup, true)?;
    let mut hit11 = melee.hit11;
    let mut hit21 = melee.hit21;
    if first.sneak || first.trick { hit11 = 1.0; hit21 = 1.0; }
    let hits = get_ma_rate3(player.main_job, f64::from(setup.ws.nhits), setup.qa, setup.ta, setup.da,
        &oa_list(&player.stats), &[0.0; 9], setup.dual_wield, &matrix,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, first.striking, first.ternary, false);
    // This override exists ONLY in the analytical branch.
    let attack2 = if setup.main_skill == SkillType::HandToHand {
        setup.ws.player_attack1
    } else {
        setup.ws.player_attack2
    };
    let main_pdif = get_avg_pdif_melee(setup.ws.player_attack1, setup.main_skill.into(),
        setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let main_damage = get_avg_phys_damage(melee.main_dmg, melee.fstr_main, setup.ws.wsc,
        main_pdif, setup.ftp2, setup.crit_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
    damage.physical += hits.main * main_damage;
    let first_pdif = get_avg_pdif_melee(setup.ws.player_attack1, setup.main_skill.into(),
        setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, first.crit_rate)?;
    let first_damage = get_avg_phys_damage(melee.main_dmg, melee.fstr_main, setup.ws.wsc,
        first_pdif, setup.ftp, first.crit_rate, first.crit_dmg, setup.wsd, setup.ws_bonus,
        setup.ws_trait, first.sneak_bonus, first.trick_bonus, first.climactic_bonus,
        first.striking_bonus, first.ternary_bonus)?;
    damage.physical += first_damage * hit11 - main_damage * hit11;
    if first.striking {
        let striking_rate = source_rate(setup.crit_rate
            + first.striking_crit_rate * flag(setup.crit_rate > 0.0));
        let pdif = get_avg_pdif_melee(setup.ws.player_attack1, setup.main_skill.into(),
            setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, striking_rate)?;
        let striking_damage = get_avg_phys_damage(melee.main_dmg, melee.fstr_main, setup.ws.wsc,
            pdif, setup.ftp2, striking_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
            setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
        damage.physical += (striking_damage - main_damage) * hit11;
    }
    let offhand_pdif = get_avg_pdif_melee(attack2, setup.sub_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let offhand_damage = get_avg_phys_damage(melee.sub_dmg, melee.fstr_sub, setup.ws.wsc,
        offhand_pdif, setup.ftp2, setup.crit_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
    damage.physical += offhand_damage * hits.sub;
    damage.tp += get_tp(hit11, setup.melee_tp_delay(), setup.stp, false)?;
    damage.tp += get_tp(hit21, setup.melee_tp_delay(), setup.stp, false)?;
    damage.tp += 10.0 * (1.0 + setup.stp) * (hits.main + hits.sub - hit11 - hit21);
    damage.tp += request.tp * (0.01 * flag(setup.fotia_gorget)) * (0.01 * flag(setup.fotia_belt));
    damage.tp += 95.0 * math::python_min(1.0, player.stats.get(Stat::ConserveTp) / 100.0);
    Ok(hit11)
}

#[inline]
fn uniform<R: RandomSource>(ctx: &mut EvaluationContext<R>) -> f64 {
    ctx.random.uniform(RandomStream::NumpyHost, 0.0, 1.0)
}

// Each successful/missed swing has exactly one host hit-rate draw. pDIF owns its
// separate Numba-Python draws, and optional events never consume randomness.
#[allow(clippy::too_many_arguments)]
fn melee_swings<R: RandomSource>(
    count: u8,
    hit_rate: f64,
    attack: f64,
    skill: PdifSkill,
    weapon_dmg: f64,
    fstr: f64,
    kind: AttackKind,
    proc: AttackProc,
    setup: &Setup,
    damage: &mut Damage,
    ctx: &mut EvaluationContext<R>,
) -> Result<(), CalcError> {
    for _ in 0..count {
        if damage.attempts < 8 {
            damage.attempts += 1;
            let landed = uniform(ctx) < hit_rate;
            let (physical_damage, critical, tp_return) = if landed {
                let (pdif, crit) = get_pdif_melee(attack, skill, setup.pdl_trait, setup.pdl_gear,
                    setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
                let physical = get_phys_damage(weapon_dmg, fstr, setup.ws.wsc, pdif,
                    setup.ftp2, crit, setup.crit_dmg, 0.0, setup.ws_bonus, setup.ws_trait,
                    0.0, 0.0, 0.0, 0.0, 0.0, 0.0)?;
                damage.physical += physical;
                let tp = 10.0 * (1.0 + setup.stp);
                damage.tp += tp;
                (physical, crit, tp)
            } else {
                (0.0, false, 0.0)
            };
            ctx.record_hit(HitEvent { kind, proc, attempt: damage.attempts, landed,
                critical, physical_damage, magical_damage: 0.0, tp_return });
        }
    }
    Ok(())
}

#[inline]
fn multiattack_tiers(player: &Player, setup: &Setup, offhand: bool) -> [(AttackProc, f64, u8); 10] {
    let oa = oa_tiers(&player.stats, offhand);
    [
        (AttackProc::QuadAttack, setup.qa, 3),
        (AttackProc::TripleAttack, setup.ta, 2),
        (AttackProc::DoubleAttack, setup.da, 1),
        oa[0], oa[1], oa[2], oa[3], oa[4], oa[5], oa[6],
    ]
}

fn sample_melee<R: RandomSource>(
    player: &Player,
    request: &WsRequest,
    single: bool,
    setup: &Setup,
    melee: &Melee,
    damage: &mut Damage,
    ctx: &mut EvaluationContext<R>,
) -> Result<f64, CalcError> {
    let first = FirstHit::new(player, setup, single)?;
    damage.attempts += 1;
    // SA/TA do not override the sampled hit rate. Nor do flourishes override the
    // sampled QA/TA/DA tiers; their only sampled effect is on this first hit.
    let landed = uniform(ctx) < melee.hit11;
    let (physical_damage, critical, tp_return) = if landed {
        let (pdif, crit) = get_pdif_melee(setup.ws.player_attack1, setup.main_skill.into(),
            setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, first.crit_rate, &mut ctx.random)?;
        let physical = get_phys_damage(melee.main_dmg, melee.fstr_main, setup.ws.wsc,
            pdif, setup.ftp, crit, first.crit_dmg, setup.wsd, setup.ws_bonus, setup.ws_trait,
            0.0, first.sneak_bonus, first.trick_bonus, first.climactic_bonus,
            first.striking_bonus, first.ternary_bonus)?;
        damage.physical += physical;
        let tp = get_tp(1.0, setup.melee_tp_delay(), setup.stp, false)?;
        damage.tp += tp;
        (physical, crit, tp)
    } else {
        (0.0, false, 0.0)
    };
    ctx.record_hit(HitEvent { kind: AttackKind::Main, proc: AttackProc::Normal,
        attempt: damage.attempts, landed, critical, physical_damage, magical_damage: 0.0, tp_return });
    if setup.dual_wield {
        damage.attempts += 1;
        let landed = uniform(ctx) < melee.hit21;
        let (physical_damage, critical, tp_return) = if landed {
            let (pdif, crit) = get_pdif_melee(setup.ws.player_attack2, setup.sub_skill.into(),
                setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
            let physical = get_phys_damage(melee.sub_dmg, melee.fstr_sub, setup.ws.wsc,
                pdif, setup.ftp2, crit, setup.crit_dmg, 0.0, setup.ws_bonus, setup.ws_trait,
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0)?;
            damage.physical += physical;
            let tp = get_tp(1.0, setup.melee_tp_delay(), setup.stp, false)?;
            damage.tp += tp;
            (physical, crit, tp)
        } else {
            (0.0, false, 0.0)
        };
        ctx.record_hit(HitEvent { kind: AttackKind::Offhand, proc: AttackProc::Normal,
            attempt: damage.attempts, landed, critical, physical_damage, magical_damage: 0.0, tp_return });
    }
    melee_swings(setup.ws.nhits - 1, melee.hit12, setup.ws.player_attack1,
        setup.main_skill.into(), melee.main_dmg, melee.fstr_main, AttackKind::Main,
        AttackProc::Normal, setup, damage, ctx)?;
    let main_checks = if !setup.dual_wield && setup.ws.nhits > 1 { 2 } else { 1 };
    let main_tiers = multiattack_tiers(player, setup, false);
    for _ in 0..main_checks {
        if damage.attempts < 8 {
            for (proc, rate, hits) in main_tiers {
                // Zero-rate tiers still consume a uniform before moving on.
                if uniform(ctx) < rate {
                    melee_swings(hits, melee.hit12, setup.ws.player_attack1, setup.main_skill.into(),
                        melee.main_dmg, melee.fstr_main, AttackKind::Main, proc, setup, damage, ctx)?;
                    break;
                }
            }
        }
    }
    if setup.dual_wield && damage.attempts < 8 {
        for (proc, rate, hits) in multiattack_tiers(player, setup, true) {
            if uniform(ctx) < rate {
                melee_swings(hits, melee.hit22, setup.ws.player_attack2, setup.sub_skill.into(),
                    melee.sub_dmg, melee.fstr_sub, AttackKind::Offhand, proc, setup, damage, ctx)?;
                break;
            }
        }
    }
    let fotia_chance = 0.01 * flag(setup.fotia_gorget) + 0.01 * flag(setup.fotia_belt);
    if uniform(ctx) < fotia_chance {
        damage.tp += request.tp;
        ctx.record_hit(HitEvent { kind: AttackKind::TpRetention, proc: AttackProc::Fotia,
            attempt: 0, landed: true, critical: false, physical_damage: 0.0,
            magical_damage: 0.0, tp_return: request.tp });
    }
    let conserve_tp_chance = player.stats.get(Stat::ConserveTp) / 100.0;
    if uniform(ctx) < conserve_tp_chance {
        let tp = python_int(ctx.random.uniform(RandomStream::NumpyHost, 10.0, 200.0), OPERATION, "Conserve TP")?;
        damage.tp += tp;
        ctx.record_hit(HitEvent { kind: AttackKind::TpRetention, proc: AttackProc::ConserveTp,
            attempt: 0, landed: true, critical: false, physical_damage: 0.0,
            magical_damage: 0.0, tp_return: tp });
    }
    Ok(melee.hit11)
}

struct Ranged {
    weapon_dmg: f64,
    fstr: f64,
    attack: f64,
    crit_rate: f64,
    true_shot: f64,
    hit1: f64,
    hit2: f64,
}

impl Ranged {
    fn new(player: &Player, enemy: &Enemy, setup: &Setup) -> Result<Self, CalcError> {
        let ranged_dmg = player.stats.get(Stat::RangedDmg);
        let ammo_dmg = player.stats.get(Stat::AmmoDmg);
        let fstr = get_fstr2(ranged_dmg, player.stats.require(Stat::Str, OPERATION)?, enemy.stats.require(Stat::Vit, OPERATION)?);
        let true_shot = player.stats.get(Stat::TrueShot) / 100.0;
        let mut crit_rate = setup.crit_rate + player.stats.get(Stat::RangedCritRate) / 100.0;
        if crit_rate > 1.0 { crit_rate = 1.0; }
        let accuracy = setup.ws.player_rangedaccuracy + player.stats.get(Stat::WeaponSkillAccuracy);
        let cap = if player.abilities.enabled(Ability::Sharpshot) { 0.99 } else { 0.95 };
        let hit1 = get_hit_rate(accuracy + 100.0, setup.enemy_evasion, cap);
        let hit2 = get_hit_rate(accuracy, setup.enemy_evasion, cap);
        Ok(Self { weapon_dmg: ranged_dmg + ammo_dmg, fstr, attack: setup.ws.player_rangedattack,
            crit_rate, true_shot, hit1, hit2 })
    }
}

fn average_ranged(setup: &Setup, ranged: &Ranged, damage: &mut Damage) -> Result<(), CalcError> {
    let pdif = get_avg_pdif_ranged(ranged.attack, setup.ranged_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, ranged.crit_rate)?;
    let first_damage = get_avg_phys_damage(ranged.weapon_dmg, ranged.fstr, setup.ws.wsc,
        pdif, setup.ftp, ranged.crit_rate, setup.crit_dmg, setup.wsd, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)? * (1.0 + setup.hover_shot);
    let other_damage = get_avg_phys_damage(ranged.weapon_dmg, ranged.fstr, setup.ws.wsc,
        pdif, setup.ftp2, ranged.crit_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)? * (1.0 + setup.hover_shot);
    damage.physical += first_damage * ranged.hit1
        + other_damage * ranged.hit2 * (f64::from(setup.ws.nhits) - 1.0);
    damage.physical *= 1.0 + ranged.true_shot;
    damage.tp = 0.0;
    damage.tp += get_tp(ranged.hit1, setup.ranged_delay + setup.ammo_delay, setup.stp, false)?;
    damage.tp += 10.0 * (1.0 + setup.stp) * (f64::from(setup.ws.nhits) - 1.0) * ranged.hit2;
    Ok(())
}

fn sample_ranged<R: RandomSource>(
    setup: &Setup,
    ranged: &Ranged,
    damage: &mut Damage,
    ctx: &mut EvaluationContext<R>,
) -> Result<(), CalcError> {
    for n in 0..setup.ws.nhits {
        // The source increments its attempted_hits only for the first ranged
        // hit, so its later attempted_hits < 8 guard is always true. Trace the
        // actual attempts without introducing a different cap or draw walk.
        damage.attempts += 1;
        let hit_rate = if n == 0 { ranged.hit1 } else { ranged.hit2 };
        let landed = uniform(ctx) < hit_rate;
        let (physical_damage, critical, tp_return) = if landed {
            let (pdif, crit) = get_pdif_ranged(ranged.attack, setup.ranged_skill.into(),
                setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, ranged.crit_rate, &mut ctx.random)?;
            let physical = get_phys_damage(ranged.weapon_dmg, ranged.fstr, setup.ws.wsc,
                pdif, if n == 0 { setup.ftp } else { setup.ftp2 }, crit, setup.crit_dmg,
                if n == 0 { setup.wsd } else { 0.0 }, setup.ws_bonus, setup.ws_trait,
                if n == 0 { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0, 0.0, 0.0)?
                * (1.0 + setup.hover_shot) * (1.0 + ranged.true_shot);
            damage.physical += physical;
            let tp = if n == 0 {
                get_tp(1.0, setup.ranged_delay + setup.ammo_delay, setup.stp, false)?
            } else {
                10.0 * (1.0 + setup.stp)
            };
            damage.tp += tp;
            (physical, crit, tp)
        } else {
            (0.0, false, 0.0)
        };
        ctx.record_hit(HitEvent { kind: AttackKind::Ranged, proc: AttackProc::Normal,
            attempt: damage.attempts, landed, critical, physical_damage, magical_damage: 0.0, tp_return });
    }
    Ok(())
}

// Source formats the element into these exact stat keys. The immutable table
// uses capitalized elements; no strings need constructing at action/hit time.
fn element_stats(element: WsElement) -> (Option<Stat>, Option<Stat>) {
    match element {
        WsElement::None => (None, None),
        WsElement::Fire => (Some(Stat::FireElementalBonus), Some(Stat::FireAffinity)),
        WsElement::Ice => (Some(Stat::IceElementalBonus), Some(Stat::IceAffinity)),
        WsElement::Wind => (Some(Stat::WindElementalBonus), Some(Stat::WindAffinity)),
        WsElement::Earth => (Some(Stat::EarthElementalBonus), Some(Stat::EarthAffinity)),
        WsElement::Thunder => (Some(Stat::ThunderElementalBonus), Some(Stat::ThunderAffinity)),
        WsElement::Light => (Some(Stat::LightElementalBonus), Some(Stat::LightAffinity)),
        WsElement::Dark => (Some(Stat::DarkElementalBonus), Some(Stat::DarkAffinity)),
        WsElement::Water => (Some(Stat::WaterElementalBonus), Some(Stat::WaterAffinity)),
    }
}

fn storm_element(storm: Storm) -> WsElement {
    match storm {
        Storm::None => WsElement::None,
        Storm::Aurora | Storm::AuroraII => WsElement::Light,
        Storm::Void | Storm::VoidII => WsElement::Dark,
        Storm::Fire | Storm::FireII => WsElement::Fire,
        Storm::Sand | Storm::SandII => WsElement::Earth,
        Storm::Rain | Storm::RainII => WsElement::Water,
        Storm::Wind | Storm::WindII => WsElement::Wind,
        Storm::Hail | Storm::HailII => WsElement::Ice,
        Storm::Thunder | Storm::ThunderII => WsElement::Thunder,
    }
}

fn magical_damage<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    request: &WsRequest,
    simulation: bool,
    setup: &Setup,
    damage: &mut Damage,
    magic_accuracy: &mut f64,
    ctx: &mut EvaluationContext<R>,
) -> Result<f64, CalcError> {
    let true_shot = player.stats.get(Stat::TrueShot) / 100.0;
    let magic_damage_stat = player.stats.get(Stat::MagicDamage);
    let magic_attack = player.stats.get(Stat::MagicAttack);
    let enemy_magic_defense = enemy.stats.get(Stat::MagicDefense);
    let enemy_magic_evasion = enemy.stats.get(Stat::MagicEvasion);
    let dstat_macc = get_dstat_macc(player.stats.require(Stat::Int, OPERATION)?, enemy.stats.require(Stat::Int, OPERATION)?);
    *magic_accuracy += dstat_macc;
    let hit_rate = if enemy_magic_evasion > 0.0 {
        get_magic_hit_rate(*magic_accuracy, enemy_magic_evasion)?
    } else {
        1.0
    };
    let magic_crit_rate = player.stats.get(Stat::MagicCritRateIi) / 100.0;
    // This host draw is also required in deterministic analytical evaluation.
    let magic_crit = uniform(ctx) < magic_crit_rate;
    let mut magic_tp = 0.0;
    let base = if setup.ws.hybrid {
        damage.physical * setup.ftp_hybrid + magic_damage_stat
    } else {
        let base = python_int(
            ((152.0 + python_int((119.0 - 99.0) * 2.45, OPERATION, "weapon_level")? + setup.ws.wsc)
                * setup.ftp) * (1.0 + player.stats.get(Stat::ElementalWsDamagePct) / 100.0)
                + setup.ws.dstat + magic_damage_stat,
            OPERATION, "base_magical_damage",
        )?;
        damage.tp = get_tp(hit_rate, setup.mdelay, setup.stp, false)?;
        magic_tp = damage.tp;
        base
    };
    let resist = if simulation {
        get_resist_state(hit_rate, &mut ctx.random)
    } else {
        get_resist_state_average(hit_rate)
    };
    let magic_attack_ratio = math::divide(
        100.0 + magic_attack, 100.0 + enemy_magic_defense, OPERATION, "Magic Defense",
    )?;
    let (bonus_stat, affinity_stat) = element_stats(setup.ws.element);
    let element_bonus = 1.0 + (bonus_stat.map_or(0.0, |stat| player.stats.get(stat)) / 100.0
        + player.stats.get(Stat::ElementalBonus) / 100.0);
    let mut dayweather = 1.0;
    if setup.obi && player.abilities.storm_name.as_deref().is_some_and(|name| !name.is_empty())
        && player.abilities.storm.is_some_and(|storm| storm != Storm::None && storm_element(storm) == setup.ws.element)
    {
        dayweather = if player.abilities.storm_contains_ii { 1.25 } else { 1.1 };
    }
    let klimaform = 1.0 + flag(player.abilities.enabled(Ability::Klimaform))
        * player.stats.get(Stat::KlimaformDamagePct) / 100.0;
    let enemy_mdt = enemy.stats.get(Stat::MagicDamageTaken);
    let affinity_value = affinity_stat.map_or(0.0, |stat| player.stats.get(stat));
    let affinity = 1.0 + 0.05 * affinity_value + 0.05 * flag(affinity_value > 0.0);
    // Do not add spell burst/resist-rank logic: this source WS branch ignores it.
    let multiplier = resist * magic_attack_ratio * element_bonus * dayweather * klimaform
        * (1.0 - enemy_mdt / 100.0) * affinity
        * (1.0 + 0.25 * if simulation { flag(magic_crit) } else { magic_crit_rate });
    damage.magical = base * multiplier;
    damage.magical *= (1.0 + setup.wsd) * (1.0 + setup.ws_bonus) * (1.0 + setup.ws_trait)
        * (1.0 + setup.hover_shot)
        * (1.0 + true_shot * flag(setup.ws.hybrid) * flag(request.ws_type == WsType::Ranged));
    if simulation {
        ctx.record_hit(HitEvent { kind: AttackKind::Magical, proc: AttackProc::Normal,
            attempt: if setup.ws.magical { 1 } else { 0 }, landed: true, critical: magic_crit,
            physical_damage: 0.0, magical_damage: damage.magical, tp_return: magic_tp });
    }
    Ok(hit_rate)
}

fn calculate<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    request: &WsRequest,
    simulation: bool,
    single: bool,
    ctx: &mut EvaluationContext<R>,
) -> Result<ActionResult, CalcError> {
    let setup = Setup::new(player, enemy, request, ctx)?;
    let mut damage = Damage::default();
    let mut first_melee_hit_rate = 1.0;
    let mut first_ranged_hit_rate = 1.0;
    if request.ws_type == WsType::Melee && !setup.ws.magical {
        let melee = Melee::new(player, enemy, request, &setup)?;
        first_melee_hit_rate = if simulation {
            sample_melee(player, request, single, &setup, &melee, &mut damage, ctx)?
        } else {
            average_melee(player, request, &setup, &melee, &mut damage)?
        };
    } else if request.ws_type == WsType::Ranged && !setup.ws.magical {
        let ranged = Ranged::new(player, enemy, &setup)?;
        first_ranged_hit_rate = ranged.hit1;
        if simulation { sample_ranged(&setup, &ranged, &mut damage, ctx)?; }
        else { average_ranged(&setup, &ranged, &mut damage)?; }
    }
    let mut total = 0.0;
    total += damage.physical;
    let mut magic_accuracy = setup.magic_accuracy;
    let mut magic_hit_rate = 1.0;
    if setup.ws.hybrid || setup.ws.magical {
        magic_hit_rate = magical_damage(player, enemy, request, simulation, &setup,
            &mut damage, &mut magic_accuracy, ctx)?;
        total += damage.magical;
    }
    // Exact boundary strings, not a GUI-normalized/inferred universal metric.
    let metric = match request.metric.name.as_ref() {
        "TP return" => damage.tp,
        "Damage dealt" => total,
        "Magic accuracy" => {
            let hit_rate = if setup.ws.magical { magic_hit_rate }
                else if request.ws_type == WsType::Melee { first_melee_hit_rate }
                else if request.ws_type == WsType::Ranged { first_ranged_hit_rate }
                else { 1.0 };
            hit_rate * magic_accuracy
        }
        _ => total,
    };
    if simulation && player.abilities.enabled(Ability::DamageLimitDps) && total > 99999.0 {
        total = 99999.0;
    }
    Ok(ActionResult { damage: total, tp_return: damage.tp, seconds_per_round: None,
        time_to_ws: None, metric, invert: 1, physical_damage: damage.physical,
        magical_damage: damage.magical })
}

pub fn evaluate_ws<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    request: &WsRequest,
    ctx: &mut EvaluationContext<R>,
) -> Result<ActionResult, CalcError> {
    calculate(player, enemy, request, false, false, ctx)
}

pub fn sample_ws<R: RandomSource>(
    player: &Player,
    enemy: &Enemy,
    request: &WsRequest,
    single: bool,
    ctx: &mut EvaluationContext<R>,
) -> Result<WsSample, CalcError> {
    let result = calculate(player, enemy, request, true, single, ctx)?;
    Ok(WsSample { damage: result.damage, tp_return: result.tp_return,
        physical_damage: result.physical_damage, magical_damage: result.magical_damage })
}
