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
use crate::model::CalculationSection as Section;
use crate::stats::Stat;
use crate::weaponskills::{DStatFormula, TpScaling, WsElement, WsScaling, weaponskill_info};

use super::common::{ONE_HANDED_SKILLS, oa_list, oa_tiers, record_tp_gain};

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
        ctx: &mut EvaluationContext<R>,
    ) -> Result<Self, CalcError> {
        let tp_bonus = player.stats.get(Stat::TpBonus);
        let tp = math::python_max(1000.0, math::python_min(3000.0, request.tp + tp_bonus));
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Damage, "Effective TP",
                &[("Effective TP = max(1000, min(3000, TP + TP Bonus))",
                    "max(1000, min(3000, {requested_tp} + {tp_bonus})) = {effective_tp}")],
                &[("requested_tp", request.tp), ("tp_bonus", tp_bonus), ("effective_tp", tp)]);
        }
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
        let setup = Self {
            ws, dual_wield, main_skill, sub_skill, ranged_skill: ctx.state.skill(ranged),
            qa, ta, da, ftp, ftp2, ftp_hybrid, crit_rate, crit_dmg, pdl_gear, pdl_trait,
            wsd, ws_trait, ws_bonus, stp, enemy_defense, enemy_evasion, mdelay,
            ranged_delay, ammo_delay, hover_shot, vajra: main.name.as_ref() == "Vajra",
            fotia_gorget: neck.name.as_ref() == "Fotia Gorget",
            fotia_belt: waist.name.as_ref() == "Fotia Belt",
            obi: waist.name.as_ref() == "Hachirin-no-Obi", magic_accuracy,
        };
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Damage, "WS damage modifiers",
                &[("WSD", "{wsd}"), ("Weapon bonus", "{weapon_bonus}"), ("WSD trait", "{ws_trait}")],
                &[("wsd", wsd), ("weapon_bonus", ws_bonus), ("ws_trait", ws_trait)]);
            if ws.hybrid {
                ctx.record_step(Section::Damage, "Hybrid WS damage",
                    &[("Hybrid damage = Physical + (Physical × Hybrid fTP + mDMG) × Magic multipliers",
                        "Physical + (Physical × {hybrid_ftp} + {magic_damage}) × Magic multipliers")],
                    &[("hybrid_ftp", ftp_hybrid), ("magic_damage", player.stats.get(Stat::MagicDamage))]);
            }
            for &(coefficient, stat) in request.skill.spec().wsc.terms() {
                let stat_value = player.stats.get(stat);
                ctx.record_step(Section::Damage, stat.name(),
                    &[("WSC term = Coefficient × Stat", "{coefficient} × {stat_value} = {term}")],
                    &[("coefficient", coefficient), ("stat_value", stat_value),
                        ("term", coefficient * stat_value)]);
            }
            for contribution in &player.wsc {
                let stat_value = player.stats.get(contribution.stat);
                ctx.record_step(Section::Damage, contribution.stat.name(),
                    &[("WSC term = Stat × Gear WSC% / 100", "{stat_value} × {percent} / 100 = {term}")],
                    &[("stat_value", stat_value), ("percent", contribution.percent),
                        ("term", stat_value * contribution.percent / 100.0)]);
            }
            ctx.record_step(Section::Damage, "WSC",
                &[("WSC = Σ Skill terms + Σ Gear terms", "{wsc}")], &[("wsc", ws.wsc)]);
            match request.skill.spec().ftp {
                TpScaling::Fixed(_) => ctx.record_step(Section::Damage, "Weapon-skill fTP",
                    &[("Skill fTP", "{ftp}")], &[("ftp", ws.ftp)]),
                TpScaling::Interpolated(points) => ctx.record_step(Section::Damage, "Weapon-skill fTP",
                    &[("Skill fTP = lerp(TP; 1000→a, 2000→b, 3000→c)",
                        "lerp({tp}; 1000→{ftp1000}, 2000→{ftp2000}, 3000→{ftp3000}) = {ftp}")],
                    &[("tp", tp), ("ftp1000", points[0]), ("ftp2000", points[1]),
                        ("ftp3000", points[2]), ("ftp", ws.ftp)]),
            }
            ctx.record_step(Section::Damage, "First-hit fTP",
                &[("fTP = Skill fTP + Gear fTP / 256", "{ws_ftp} + {gear_ftp} / 256 = {ftp}")],
                &[("ws_ftp", ws.ftp), ("gear_ftp", player.stats.get(Stat::Ftp)), ("ftp", ftp)]);
            if !ws.magical {
                let lines: &'static [(&'static str, &'static str)] = if ws.ftp_rep {
                    &[("Later-hit fTP = fTP", "{ftp_other}")]
                } else {
                    &[("Later-hit fTP", "{ftp_other}")]
                };
                ctx.record_step(Section::Damage, "Later-hit fTP", lines, &[("ftp_other", ftp2)]);
            }
            if ws.hybrid {
                if let Some(points) = request.skill.spec().ftp_hybrid {
                    ctx.record_step(Section::Damage, "Hybrid fTP",
                        &[("Hybrid fTP = lerp(TP; 1000→a, 2000→b, 3000→c) + Gear fTP / 256",
                            "lerp({tp}; 1000→{ftp1000}, 2000→{ftp2000}, 3000→{ftp3000}) + {gear_ftp} / 256 = {ftp_hybrid}")],
                        &[("tp", tp), ("ftp1000", points[0]), ("ftp2000", points[1]),
                            ("ftp3000", points[2]), ("gear_ftp", player.stats.get(Stat::Ftp)),
                            ("ftp_hybrid", ftp_hybrid)]);
                }
            }
            ctx.record_step(Section::Tp, "Store TP",
                &[("Store TP = STP / 100", "{store_tp} / 100 = {store_tp_input}")],
                &[("store_tp", player.stats.get(Stat::StoreTp)), ("store_tp_input", stp)]);
            if ws.magical || request.ws_type == WsType::Melee {
                ctx.record_step(Section::Tp, "WS TP delay",
                    &[("Delay = ((Delay1 + Delay2) / 2 − Martial Arts) × (1 − DW / 100)",
                        "(({main_delay} + {sub_delay}) / 2 − {martial_arts}) × (1 − {dual_wield} / 100) = {delay}")],
                    &[("main_delay", player.stats.get(Stat::Delay1)),
                        ("sub_delay", player.stats.get(Stat::Delay2)),
                        ("martial_arts", player.stats.get(Stat::MartialArts)),
                        ("dual_wield", player.stats.get(Stat::DualWield)), ("delay", mdelay)]);
            }
        }
        Ok(setup)
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

const PDIF_CAP: (&str, &str) = ("Cap = (Skill cap + PDL trait) × (1 + PDL gear)",
    "(Skill cap + {pdl_trait}) × (1 + {pdl_gear})");
const CRIT_ROLL: (&str, &str) = ("Crit = I([random critical roll: U(0,1)] < Crit rate)",
    "I([random critical roll: U(0,1)] < {crit_rate})");
const PHYSICAL_BASE: (&str, &str) = (
    "Base = trunc((D + fSTR + WSC) × fTP × (1 + WSD) × (1 + Weapon bonus) × (1 + WSD trait))",
    "trunc(({weapon_dmg} + {fstr} + {wsc}) × {ftp} × (1 + {wsd}) × (1 + {weapon_bonus}) × (1 + {ws_trait})) = {base}");
const ABILITY_BASE: (&str, &str) = (
    "Base = trunc((D + fSTR + WSC) × fTP × (1 + WSD) × (1 + Weapon bonus) × (1 + WSD trait) + SA + TA + CF + SF + TF)",
    "trunc(({weapon_dmg} + {fstr} + {wsc}) × {ftp} × (1 + {wsd}) × (1 + {weapon_bonus}) × (1 + {ws_trait}) + {sneak} + {trick} + {climactic} + {striking} + {ternary}) = {base}");
const AVERAGE_HIT: (&str, &str) = ("Hit damage = Base × pDIF × (1 + min(Crit rate, 1) × min(Crit dmg, 1))",
    "{base} × {pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {physical}");
const SAMPLED_HIT: (&str, &str) = ("Hit damage = Base × pDIF × (1 + Crit × min(Crit dmg, 1))",
    "{base} × [random pDIF result] × (1 + I([random critical roll: U(0,1)] < {crit_rate}) × min({crit_damage}, 1)) = {physical}");

// These helpers only describe operands already used by the damage helpers; the
// displayed physical base repeats the kernel's own once-truncated arithmetic.
// Callers guard them so metric-only evaluation does not construct trace values.
#[allow(clippy::too_many_arguments)]
fn record_physical<R: RandomSource>(
    ctx: &mut EvaluationContext<R>,
    label: &'static str,
    setup: &Setup,
    weapon_dmg: f64,
    fstr: f64,
    ftp: f64,
    wsd: f64,
    first: Option<&FirstHit>,
    crit_rate: f64,
    crit_dmg: f64,
    average_pdif: Option<f64>,
    physical: f64,
) {
    let ability = first.filter(|first| first.sneak_bonus != 0.0
        || first.trick_bonus != 0.0 || first.climactic_bonus != 0.0
        || first.striking_bonus != 0.0 || first.ternary_bonus != 0.0);
    let (sneak, trick, climactic, striking, ternary) = ability.map_or((0.0, 0.0, 0.0, 0.0, 0.0),
        |first| (first.sneak_bonus, first.trick_bonus, first.climactic_bonus,
            first.striking_bonus, first.ternary_bonus));
    let base = ((weapon_dmg + fstr + setup.ws.wsc) * ftp * (1.0 + wsd) * (1.0 + setup.ws_bonus)
        * (1.0 + setup.ws_trait) + sneak + trick + climactic + striking + ternary).trunc();
    let lines: &'static [(&'static str, &'static str)] = match (ability.is_some(), average_pdif.is_some()) {
        (false, true) => &[PHYSICAL_BASE, AVERAGE_HIT],
        (false, false) => &[PHYSICAL_BASE, SAMPLED_HIT],
        (true, true) => &[ABILITY_BASE, AVERAGE_HIT],
        (true, false) => &[ABILITY_BASE, SAMPLED_HIT],
    };
    let values = [("weapon_dmg", weapon_dmg), ("fstr", fstr), ("wsc", setup.ws.wsc), ("ftp", ftp),
        ("wsd", wsd), ("weapon_bonus", setup.ws_bonus), ("ws_trait", setup.ws_trait),
        ("sneak", sneak), ("trick", trick), ("climactic", climactic), ("striking", striking),
        ("ternary", ternary), ("base", base), ("crit_rate", crit_rate), ("crit_damage", crit_dmg),
        ("physical", physical), ("pdif", average_pdif.unwrap_or(0.0))];
    // A sampled pDIF stays a random placeholder, so it has no operand.
    let values = if average_pdif.is_some() { &values[..] } else { &values[..values.len() - 1] };
    ctx.record_step(Section::Damage, label, lines, values);
}

fn record_sampled_pdif<R: RandomSource>(
    ctx: &mut EvaluationContext<R>,
    label: &'static str,
    setup: &Setup,
    attack: f64,
    crit_rate: f64,
    ranged: bool,
) {
    let lines: &'static [(&'static str, &'static str)] = if ranged {
        &[CRIT_ROLL, ("Ratio = Ranged Attack / Defense", "{attack} / {defense} = {ratio}"), PDIF_CAP,
            ("pDIF = clamp([random pDIF roll: U(L(Ratio), U(Ratio))], 0, Cap) × (1 + 0.25 × Crit)", "")]
    } else {
        &[CRIT_ROLL, ("wRatio = Attack / Defense + Crit", "{attack} / {defense} + Crit"), PDIF_CAP,
            ("pDIF = (clamp([random pDIF roll: U(L(wRatio), U(wRatio))], 0, Cap) + Crit) × [random spread: U(1,1.05)]", "")]
    };
    ctx.record_step(Section::Damage, label, lines,
        &[("attack", attack), ("defense", setup.enemy_defense), ("ratio", attack / setup.enemy_defense),
            ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear), ("crit_rate", crit_rate)]);
}

#[allow(clippy::too_many_arguments)]
fn record_average_pdif<R: RandomSource>(
    ctx: &mut EvaluationContext<R>,
    label: &'static str,
    setup: &Setup,
    attack: f64,
    crit_rate: f64,
    pdif: f64,
    ranged: bool,
) {
    // The same ratio the pDIF kernel bounds; melee adds the capped crit rate.
    let ratio = attack / setup.enemy_defense;
    let (lines, ratio): (&'static [(&'static str, &'static str)], f64) = if ranged {
        (&[("Ratio = Ranged Attack / Defense", "{attack} / {defense} = {ratio}"), PDIF_CAP,
            ("pDIF = clamp((L(Ratio) + U(Ratio)) / 2, 0, Cap) × (1 + 0.25 × min(Crit rate, 1))",
                "clamp((L({ratio}) + U({ratio})) / 2, 0, Cap) × (1 + 0.25 × min({crit_rate}, 1)) = {pdif}")],
            ratio)
    } else {
        (&[("wRatio = Attack / Defense + min(Crit rate, 1)",
                "{attack} / {defense} + min({crit_rate}, 1) = {ratio}"), PDIF_CAP,
            ("pDIF = (clamp((L(wRatio) + U(wRatio)) / 2, 0, Cap) + min(Crit rate, 1)) × 1.025",
                "(clamp((L({ratio}) + U({ratio})) / 2, 0, Cap) + min({crit_rate}, 1)) × 1.025 = {pdif}")],
            ratio + if crit_rate > 1.0 { 1.0 } else { crit_rate })
    };
    ctx.record_step(Section::Damage, label, lines,
        &[("attack", attack), ("defense", setup.enemy_defense), ("ratio", ratio),
            ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear),
            ("crit_rate", crit_rate), ("pdif", pdif)]);
}

fn average_melee<R: RandomSource>(
    player: &Player,
    request: &WsRequest,
    setup: &Setup,
    melee: &Melee,
    damage: &mut Damage,
    ctx: &mut EvaluationContext<R>,
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
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Hits, "Expected melee landed hits",
            &[("(E[Main hits], E[Off hits]) = MA(Hits, QA, TA, DA, OA, Hit rates)",
                "MA({base_hits}, {qa}, {ta}, {da}, OA, [{main_first}, {main_later}, {sub_first}, {sub_later}]) = ({main_hits}, {sub_hits})")],
            &[("base_hits", f64::from(setup.ws.nhits)), ("qa", setup.qa), ("ta", setup.ta),
                ("da", setup.da), ("main_first", melee.hit11), ("sub_first", melee.hit21),
                ("main_later", melee.hit12), ("sub_later", melee.hit22),
                ("main_hits", hits.main), ("sub_hits", hits.sub)]);
        for (stat, rate) in [Stat::Oa3Main, Stat::Oa2Main, Stat::Oa8Sub, Stat::Oa7Sub,
            Stat::Oa6Sub, Stat::Oa5Sub, Stat::Oa4Sub, Stat::Oa3Sub, Stat::Oa2Sub]
            .into_iter().zip(oa_list(&player.stats))
        {
            if rate != 0.0 {
                ctx.record_step(Section::Hits, stat.name(), &[("OA rate", "{rate}")], &[("rate", rate)]);
            }
        }
    }
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
    if ctx.calculation_trace.is_some() {
        record_average_pdif(ctx, "Main-hand pDIF", setup,
            setup.ws.player_attack1, setup.crit_rate, main_pdif, false);
        record_physical(ctx, "Main-hand later-hit damage", setup, melee.main_dmg,
            melee.fstr_main, setup.ftp2, 0.0, None, setup.crit_rate, setup.crit_dmg,
            Some(main_pdif), main_damage);
    }
    damage.physical += hits.main * main_damage;
    let first_pdif = get_avg_pdif_melee(setup.ws.player_attack1, setup.main_skill.into(),
        setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, first.crit_rate)?;
    let first_damage = get_avg_phys_damage(melee.main_dmg, melee.fstr_main, setup.ws.wsc,
        first_pdif, setup.ftp, first.crit_rate, first.crit_dmg, setup.wsd, setup.ws_bonus,
        setup.ws_trait, first.sneak_bonus, first.trick_bonus, first.climactic_bonus,
        first.striking_bonus, first.ternary_bonus)?;
    damage.physical += first_damage * hit11 - main_damage * hit11;
    if ctx.calculation_trace.is_some() {
        record_average_pdif(ctx, "First-hit pDIF", setup,
            setup.ws.player_attack1, first.crit_rate, first_pdif, false);
        record_physical(ctx, "First main-hand hit damage", setup, melee.main_dmg,
            melee.fstr_main, setup.ftp, setup.wsd, Some(&first), first.crit_rate,
            first.crit_dmg, Some(first_pdif), first_damage);
        ctx.record_step(Section::Damage, "Main-hand damage",
            &[("Main damage = E[Main hits] × Later dmg + First dmg × p1 − Later dmg × p1",
                "{main_hits} × {other_damage} + {first_damage} × {first_hit_rate} − {other_damage} × {first_hit_rate} = {physical}")],
            &[("main_hits", hits.main), ("other_damage", main_damage),
                ("first_damage", first_damage), ("first_hit_rate", hit11),
                ("physical", damage.physical)]);
    }
    if first.striking {
        let striking_rate = source_rate(setup.crit_rate
            + first.striking_crit_rate * flag(setup.crit_rate > 0.0));
        let pdif = get_avg_pdif_melee(setup.ws.player_attack1, setup.main_skill.into(),
            setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, striking_rate)?;
        let striking_damage = get_avg_phys_damage(melee.main_dmg, melee.fstr_main, setup.ws.wsc,
            pdif, setup.ftp2, striking_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
            setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
        damage.physical += (striking_damage - main_damage) * hit11;
        if ctx.calculation_trace.is_some() {
            record_average_pdif(ctx, "Striking Flourish pDIF", setup,
                setup.ws.player_attack1, striking_rate, pdif, false);
            record_physical(ctx, "Striking Flourish hit damage", setup,
                melee.main_dmg, melee.fstr_main, setup.ftp2, 0.0, None,
                striking_rate, setup.crit_dmg, Some(pdif), striking_damage);
            ctx.record_step(Section::Damage, "Striking Flourish correction",
                &[("Correction = (Striking dmg − Later dmg) × p1",
                    "({striking_damage} − {baseline_damage}) × {first_hit_rate} = {correction}"),
                    ("Main damage + Correction", "{physical}")],
                &[("striking_damage", striking_damage), ("baseline_damage", main_damage),
                    ("first_hit_rate", hit11), ("correction", (striking_damage - main_damage) * hit11),
                    ("physical", damage.physical)]);
        }
    }
    let offhand_pdif = get_avg_pdif_melee(attack2, setup.sub_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let offhand_damage = get_avg_phys_damage(melee.sub_dmg, melee.fstr_sub, setup.ws.wsc,
        offhand_pdif, setup.ftp2, setup.crit_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
    damage.physical += offhand_damage * hits.sub;
    if ctx.calculation_trace.is_some() && setup.dual_wield {
        record_average_pdif(ctx, "Offhand pDIF", setup,
            attack2, setup.crit_rate, offhand_pdif, false);
        record_physical(ctx, "Offhand hit damage", setup, melee.sub_dmg,
            melee.fstr_sub, setup.ftp2, 0.0, None, setup.crit_rate, setup.crit_dmg,
            Some(offhand_pdif), offhand_damage);
        ctx.record_step(Section::Damage, "Offhand damage",
            &[("Offhand = Offhand dmg × E[Off hits]", "{offhand_damage} × {offhand_hits} = {offhand}"),
                ("Main damage + Offhand", "{physical}")],
            &[("offhand_damage", offhand_damage), ("offhand_hits", hits.sub),
                ("offhand", offhand_damage * hits.sub), ("physical", damage.physical)]);
    }
    let main_tp = get_tp(hit11, setup.melee_tp_delay(), setup.stp, false)?;
    damage.tp += main_tp;
    let sub_tp = get_tp(hit21, setup.melee_tp_delay(), setup.stp, false)?;
    damage.tp += sub_tp;
    let other_tp = 10.0 * (1.0 + setup.stp) * (hits.main + hits.sub - hit11 - hit21);
    damage.tp += other_tp;
    let fotia_tp = request.tp * (0.01 * flag(setup.fotia_gorget)) * (0.01 * flag(setup.fotia_belt));
    damage.tp += fotia_tp;
    let conserve_tp = 95.0 * math::python_min(1.0, player.stats.get(Stat::ConserveTp) / 100.0);
    damage.tp += conserve_tp;
    if ctx.calculation_trace.is_some() {
        record_tp_gain(ctx, "Expected first main-hand TP", hit11,
            setup.melee_tp_delay(), setup.stp, false, main_tp);
        if setup.dual_wield || hit21 != 0.0 {
            record_tp_gain(ctx, "Expected first offhand TP", hit21,
                setup.melee_tp_delay(), setup.stp, false, sub_tp);
        }
        ctx.record_step(Section::Tp, "Expected later-hit TP",
            &[("Later-hit TP = 10 × (1 + Store TP) × (E[Main hits] + E[Off hits] − Main p1 − Off p1)",
                "10 × (1 + {store_tp}) × ({main_hits} + {sub_hits} − {main_first} − {sub_first}) = {other_tp}")],
            &[("store_tp", setup.stp), ("main_hits", hits.main), ("sub_hits", hits.sub),
                ("main_first", hit11), ("sub_first", hit21), ("other_tp", other_tp)]);
        if setup.fotia_gorget || setup.fotia_belt {
            ctx.record_step(Section::Tp, "Expected Fotia TP",
                &[("Fotia TP = TP × (0.01 × I(Gorget)) × (0.01 × I(Belt))",
                    "{requested_tp} × (0.01 × {gorget}) × (0.01 × {belt}) = {retained_tp}")],
                &[("requested_tp", request.tp), ("gorget", flag(setup.fotia_gorget)),
                    ("belt", flag(setup.fotia_belt)), ("retained_tp", fotia_tp)]);
        }
        if player.stats.get(Stat::ConserveTp) != 0.0 {
            ctx.record_step(Section::Tp, "Expected Conserve TP",
                &[("Conserve TP = 95 × min(1, Conserve TP% / 100)", "95 × min(1, {conserve_tp} / 100) = {retained_tp}")],
                &[("conserve_tp", player.stats.get(Stat::ConserveTp)),
                    ("retained_tp", conserve_tp)]);
        }
        ctx.record_step(Section::Tp, "Expected WS TP return",
            &[("TP return = Main TP + Off TP + Later-hit TP + Fotia TP + Conserve TP",
                "{main_tp} + {offhand_tp} + {other_tp} + {fotia_tp} + {conserve_tp} = {total_tp}")],
            &[("main_tp", main_tp), ("offhand_tp", sub_tp), ("other_tp", other_tp),
                ("fotia_tp", fotia_tp), ("conserve_tp", conserve_tp), ("total_tp", damage.tp)]);
    }
    Ok(hit11)
}

#[inline]
fn uniform<R: RandomSource>(ctx: &mut EvaluationContext<R>) -> f64 {
    ctx.random.uniform(RandomStream::Primary, 0.0, 1.0)
}

const ATTEMPT: (&str, &str) = ("Attempt", "{attempt}");
const HIT_ROLL: (&str, &str) = ("Landed = I([random hit roll: U(0,1)] < Hit rate)",
    "I([random hit roll: U(0,1)] < {hit_rate}) = {landed}");

// Each successful/missed swing has exactly one Primary hit-rate draw. pDIF owns
// its separate Pdif draws, and optional events never consume randomness.
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
            if ctx.calculation_trace.is_some() {
                ctx.record_step(Section::Hits, if kind == AttackKind::Offhand {
                    "Offhand later hit"
                } else {
                    "Main-hand later hit"
                }, &[ATTEMPT, HIT_ROLL],
                    &[("attempt", f64::from(damage.attempts)), ("hit_rate", hit_rate),
                        ("landed", flag(landed))]);
            }
            let (physical_damage, critical, tp_return) = if landed {
                let (pdif, crit) = get_pdif_melee(attack, skill, setup.pdl_trait, setup.pdl_gear,
                    setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
                let physical = get_phys_damage(weapon_dmg, fstr, setup.ws.wsc, pdif,
                    setup.ftp2, crit, setup.crit_dmg, 0.0, setup.ws_bonus, setup.ws_trait,
                    0.0, 0.0, 0.0, 0.0, 0.0, 0.0)?;
                damage.physical += physical;
                let tp = 10.0 * (1.0 + setup.stp);
                damage.tp += tp;
                if ctx.calculation_trace.is_some() {
                    let offhand = kind == AttackKind::Offhand;
                    record_sampled_pdif(ctx, if offhand { "Offhand later-hit pDIF" } else {
                        "Main-hand later-hit pDIF"
                    }, setup, attack, setup.crit_rate, false);
                    record_physical(ctx, if offhand { "Offhand later-hit damage" } else {
                        "Main-hand later-hit damage"
                    }, setup, weapon_dmg, fstr, setup.ftp2, 0.0, None, setup.crit_rate,
                        setup.crit_dmg, None, physical);
                    ctx.record_step(Section::Tp, "Later-hit TP",
                        &[("TP = 10 × (1 + Store TP)", "10 × (1 + {store_tp}) = {tp}")],
                        &[("store_tp", setup.stp), ("tp", tp)]);
                }
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

#[allow(clippy::too_many_arguments)]
fn record_multiattack_proc<R: RandomSource>(
    ctx: &mut EvaluationContext<R>,
    offhand: bool,
    proc: AttackProc,
    rate: f64,
    hits: u8,
    activated: bool,
    attempts: u8,
) {
    let label = match (offhand, proc) {
        (false, AttackProc::QuadAttack) => "Main-hand Quad Attack proc",
        (true, AttackProc::QuadAttack) => "Offhand Quad Attack proc",
        (false, AttackProc::TripleAttack) => "Main-hand Triple Attack proc",
        (true, AttackProc::TripleAttack) => "Offhand Triple Attack proc",
        (false, AttackProc::DoubleAttack) => "Main-hand Double Attack proc",
        (true, AttackProc::DoubleAttack) => "Offhand Double Attack proc",
        (false, AttackProc::Occasionally(_)) => "Main-hand OA proc",
        (true, AttackProc::Occasionally(_)) => "Offhand OA proc",
        (false, _) => "Main-hand multiattack proc",
        (true, _) => "Offhand multiattack proc",
    };
    // A selected proc swings until its extra hits or the eight-attempt cap run out.
    let added = flag(activated) * f64::from(hits).min(8.0 - f64::from(attempts));
    ctx.record_step(Section::Hits, label,
        &[("Proc = I([random multiattack proc roll: U(0,1)] < Proc rate)",
            "I([random multiattack proc roll: U(0,1)] < {probability}) = {activated}"),
            ("Added attempts = Proc × min(Extra hits, 8 − Attempts)",
                "{activated} × min({extra_hits}, 8 − {attempts}) = {added}")],
        &[("probability", rate), ("activated", flag(activated)), ("extra_hits", f64::from(hits)),
            ("attempts", f64::from(attempts)), ("added", added)]);
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
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Hits, "First main-hand hit", &[ATTEMPT, HIT_ROLL],
            &[("attempt", f64::from(damage.attempts)), ("hit_rate", melee.hit11),
                ("landed", flag(landed))]);
    }
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
        if ctx.calculation_trace.is_some() {
            record_sampled_pdif(ctx, "First main-hand pDIF", setup, setup.ws.player_attack1,
                first.crit_rate, false);
            record_physical(ctx, "First main-hand hit damage", setup,
                melee.main_dmg, melee.fstr_main, setup.ftp, setup.wsd, Some(&first),
                first.crit_rate, first.crit_dmg, None, physical);
            record_tp_gain(ctx, "First main-hand TP", 1.0,
                setup.melee_tp_delay(), setup.stp, false, tp);
        }
        (physical, crit, tp)
    } else {
        (0.0, false, 0.0)
    };
    ctx.record_hit(HitEvent { kind: AttackKind::Main, proc: AttackProc::Normal,
        attempt: damage.attempts, landed, critical, physical_damage, magical_damage: 0.0, tp_return });
    if setup.dual_wield {
        damage.attempts += 1;
        let landed = uniform(ctx) < melee.hit21;
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Hits, "First offhand hit", &[ATTEMPT, HIT_ROLL],
                &[("attempt", f64::from(damage.attempts)), ("hit_rate", melee.hit21),
                    ("landed", flag(landed))]);
        }
        let (physical_damage, critical, tp_return) = if landed {
            let (pdif, crit) = get_pdif_melee(setup.ws.player_attack2, setup.sub_skill.into(),
                setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, setup.crit_rate, &mut ctx.random)?;
            let physical = get_phys_damage(melee.sub_dmg, melee.fstr_sub, setup.ws.wsc,
                pdif, setup.ftp2, crit, setup.crit_dmg, 0.0, setup.ws_bonus, setup.ws_trait,
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0)?;
            damage.physical += physical;
            let tp = get_tp(1.0, setup.melee_tp_delay(), setup.stp, false)?;
            damage.tp += tp;
            if ctx.calculation_trace.is_some() {
                record_sampled_pdif(ctx, "First offhand pDIF", setup, setup.ws.player_attack2,
                    setup.crit_rate, false);
                record_physical(ctx, "First offhand hit damage", setup,
                    melee.sub_dmg, melee.fstr_sub, setup.ftp2, 0.0, None,
                    setup.crit_rate, setup.crit_dmg, None, physical);
                record_tp_gain(ctx, "First offhand TP", 1.0,
                    setup.melee_tp_delay(), setup.stp, false, tp);
            }
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
                let activated = uniform(ctx) < rate;
                if ctx.calculation_trace.is_some() && rate != 0.0 {
                    record_multiattack_proc(ctx, false, proc, rate, hits, activated, damage.attempts);
                }
                if activated {
                    melee_swings(hits, melee.hit12, setup.ws.player_attack1, setup.main_skill.into(),
                        melee.main_dmg, melee.fstr_main, AttackKind::Main, proc, setup, damage, ctx)?;
                    break;
                }
            }
        }
    }
    if setup.dual_wield && damage.attempts < 8 {
        for (proc, rate, hits) in multiattack_tiers(player, setup, true) {
            let activated = uniform(ctx) < rate;
            if ctx.calculation_trace.is_some() && rate != 0.0 {
                record_multiattack_proc(ctx, true, proc, rate, hits, activated, damage.attempts);
            }
            if activated {
                melee_swings(hits, melee.hit22, setup.ws.player_attack2, setup.sub_skill.into(),
                    melee.sub_dmg, melee.fstr_sub, AttackKind::Offhand, proc, setup, damage, ctx)?;
                break;
            }
        }
    }
    let fotia_chance = 0.01 * flag(setup.fotia_gorget) + 0.01 * flag(setup.fotia_belt);
    let fotia_retained = uniform(ctx) < fotia_chance;
    if ctx.calculation_trace.is_some() && fotia_chance != 0.0 {
        ctx.record_step(Section::Tp, "Fotia TP retention",
            &[("Fotia rate = 0.01 × I(Gorget) + 0.01 × I(Belt)",
                "0.01 × {gorget} + 0.01 × {belt} = {probability}"),
                ("Fotia TP = I([random Fotia retention roll: U(0,1)] < Fotia rate) × TP",
                    "I([random Fotia retention roll: U(0,1)] < {probability}) × {requested_tp} = {retained_tp}")],
            &[("gorget", flag(setup.fotia_gorget)), ("belt", flag(setup.fotia_belt)),
                ("probability", fotia_chance), ("requested_tp", request.tp),
                ("retained_tp", if fotia_retained { request.tp } else { 0.0 })]);
    }
    if fotia_retained {
        damage.tp += request.tp;
        ctx.record_hit(HitEvent { kind: AttackKind::TpRetention, proc: AttackProc::Fotia,
            attempt: 0, landed: true, critical: false, physical_damage: 0.0,
            magical_damage: 0.0, tp_return: request.tp });
    }
    let conserve_tp_chance = player.stats.get(Stat::ConserveTp) / 100.0;
    let conserved = uniform(ctx) < conserve_tp_chance;
    if ctx.calculation_trace.is_some() && conserve_tp_chance != 0.0 {
        ctx.record_step(Section::Tp, "Conserve TP proc",
            &[("Proc = I([random Conserve TP proc roll: U(0,1)] < Conserve TP% / 100)",
                "I([random Conserve TP proc roll: U(0,1)] < {conserve_tp} / 100) = {activated}")],
            &[("conserve_tp", player.stats.get(Stat::ConserveTp)), ("activated", flag(conserved))]);
    }
    if conserved {
        let tp = python_int(ctx.random.uniform(RandomStream::Primary, 10.0, 200.0), OPERATION, "Conserve TP")?;
        damage.tp += tp;
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Tp, "Conserve TP amount",
                &[("Conserve TP = trunc([random Conserve TP amount: U(10,200)])", "{tp}")],
                &[("tp", tp)]);
        }
        ctx.record_hit(HitEvent { kind: AttackKind::TpRetention, proc: AttackProc::ConserveTp,
            attempt: 0, landed: true, critical: false, physical_damage: 0.0,
            magical_damage: 0.0, tp_return: tp });
    }
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Hits, "Attempted hits", &[("Attempted hits", "{attempts}")],
            &[("attempts", f64::from(damage.attempts))]);
        ctx.record_step(Section::Damage, "Physical damage", &[("Physical = Σ Hit damage", "{physical}")],
            &[("physical", damage.physical)]);
        ctx.record_step(Section::Tp, "WS TP return",
            &[("TP return = Σ Hit TP + Fotia TP + Conserve TP", "{tp}")], &[("tp", damage.tp)]);
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

fn average_ranged<R: RandomSource>(
    setup: &Setup,
    ranged: &Ranged,
    damage: &mut Damage,
    ctx: &mut EvaluationContext<R>,
) -> Result<(), CalcError> {
    let pdif = get_avg_pdif_ranged(ranged.attack, setup.ranged_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, ranged.crit_rate)?;
    let first_base = get_avg_phys_damage(ranged.weapon_dmg, ranged.fstr, setup.ws.wsc,
        pdif, setup.ftp, ranged.crit_rate, setup.crit_dmg, setup.wsd, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
    let first_damage = first_base * (1.0 + setup.hover_shot);
    let other_base = get_avg_phys_damage(ranged.weapon_dmg, ranged.fstr, setup.ws.wsc,
        pdif, setup.ftp2, ranged.crit_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
    let other_damage = other_base * (1.0 + setup.hover_shot);
    damage.physical += first_damage * ranged.hit1
        + other_damage * ranged.hit2 * (f64::from(setup.ws.nhits) - 1.0);
    damage.physical *= 1.0 + ranged.true_shot;
    if ctx.calculation_trace.is_some() {
        record_average_pdif(ctx, "Ranged pDIF", setup,
            ranged.attack, ranged.crit_rate, pdif, true);
        record_physical(ctx, "First ranged hit damage", setup,
            ranged.weapon_dmg, ranged.fstr, setup.ftp, setup.wsd, None,
            ranged.crit_rate, setup.crit_dmg, Some(pdif), first_base);
        if setup.ws.nhits > 1 {
            record_physical(ctx, "Later ranged hit damage", setup,
                ranged.weapon_dmg, ranged.fstr, setup.ftp2, 0.0, None,
                ranged.crit_rate, setup.crit_dmg, Some(pdif), other_base);
        }
        const FIRST: (&str, &str) = ("First dmg = First hit damage × (1 + Hover Shot)",
            "{first_base} × (1 + {hover_shot}) = {first_damage}");
        const LATER: (&str, &str) = ("Later dmg = Later hit damage × (1 + Hover Shot)",
            "{other_base} × (1 + {hover_shot}) = {other_damage}");
        const TOTAL: (&str, &str) = ("Physical = (First dmg × p1 + Later dmg × p2 × (Hits − 1)) × (1 + True Shot)",
            "({first_damage} × {first_hit_rate} + {other_damage} × {other_hit_rate} × ({base_hits} − 1)) × (1 + {true_shot}) = {physical}");
        let lines: &'static [(&'static str, &'static str)] = if setup.ws.nhits > 1 {
            &[FIRST, LATER, TOTAL]
        } else {
            &[FIRST, TOTAL]
        };
        ctx.record_step(Section::Damage, "Ranged damage", lines,
            &[("first_base", first_base), ("other_base", other_base), ("hover_shot", setup.hover_shot),
                ("first_damage", first_damage), ("first_hit_rate", ranged.hit1),
                ("other_damage", other_damage), ("other_hit_rate", ranged.hit2),
                ("base_hits", f64::from(setup.ws.nhits)), ("true_shot", ranged.true_shot),
                ("physical", damage.physical)]);
    }
    damage.tp = 0.0;
    let first_tp = get_tp(ranged.hit1, setup.ranged_delay + setup.ammo_delay, setup.stp, false)?;
    damage.tp += first_tp;
    let other_tp = 10.0 * (1.0 + setup.stp) * (f64::from(setup.ws.nhits) - 1.0) * ranged.hit2;
    damage.tp += other_tp;
    if ctx.calculation_trace.is_some() {
        record_tp_gain(ctx, "Expected first ranged-hit TP", ranged.hit1,
            setup.ranged_delay + setup.ammo_delay, setup.stp, false, first_tp);
        if setup.ws.nhits > 1 {
            ctx.record_step(Section::Tp, "Expected later ranged-hit TP",
                &[("Later-hit TP = 10 × (1 + Store TP) × (Hits − 1) × p2",
                    "10 × (1 + {store_tp}) × ({base_hits} − 1) × {hit_rate} = {tp}")],
                &[("store_tp", setup.stp), ("base_hits", f64::from(setup.ws.nhits)),
                    ("hit_rate", ranged.hit2), ("tp", other_tp)]);
        }
        ctx.record_step(Section::Tp, "Expected WS TP return",
            &[("TP return = First-hit TP + Later-hit TP", "{first_tp} + {other_tp} = {total_tp}")],
            &[("first_tp", first_tp), ("other_tp", other_tp), ("total_tp", damage.tp)]);
    }
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
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Hits, if n == 0 { "First ranged hit" } else { "Later ranged hit" },
                &[ATTEMPT, HIT_ROLL],
                &[("attempt", f64::from(damage.attempts)), ("hit_rate", hit_rate),
                    ("landed", flag(landed))]);
        }
        let (physical_damage, critical, tp_return) = if landed {
            let (pdif, crit) = get_pdif_ranged(ranged.attack, setup.ranged_skill.into(),
                setup.pdl_trait, setup.pdl_gear, setup.enemy_defense, ranged.crit_rate, &mut ctx.random)?;
            let base_physical = get_phys_damage(ranged.weapon_dmg, ranged.fstr, setup.ws.wsc,
                pdif, if n == 0 { setup.ftp } else { setup.ftp2 }, crit, setup.crit_dmg,
                if n == 0 { setup.wsd } else { 0.0 }, setup.ws_bonus, setup.ws_trait,
                if n == 0 { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0, 0.0, 0.0)?;
            let physical = base_physical * (1.0 + setup.hover_shot) * (1.0 + ranged.true_shot);
            damage.physical += physical;
            let tp = if n == 0 {
                get_tp(1.0, setup.ranged_delay + setup.ammo_delay, setup.stp, false)?
            } else {
                10.0 * (1.0 + setup.stp)
            };
            damage.tp += tp;
            if ctx.calculation_trace.is_some() {
                record_sampled_pdif(ctx, if n == 0 { "First ranged pDIF" } else { "Later ranged pDIF" },
                    setup, ranged.attack, ranged.crit_rate, true);
                record_physical(ctx, if n == 0 { "First ranged hit damage" } else {
                    "Later ranged hit damage"
                }, setup, ranged.weapon_dmg, ranged.fstr,
                    if n == 0 { setup.ftp } else { setup.ftp2 },
                    if n == 0 { setup.wsd } else { 0.0 }, None,
                    ranged.crit_rate, setup.crit_dmg, None, base_physical);
                ctx.record_step(Section::Damage, "Ranged hit multipliers",
                    &[("Ranged hit = Hit damage × (1 + Hover Shot) × (1 + True Shot)",
                        "{hit} × (1 + {hover_shot}) × (1 + {true_shot}) = {damage}")],
                    &[("hit", base_physical), ("hover_shot", setup.hover_shot),
                        ("true_shot", ranged.true_shot), ("damage", physical)]);
                if n == 0 {
                    record_tp_gain(ctx, "First ranged-hit TP", 1.0,
                        setup.ranged_delay + setup.ammo_delay, setup.stp, false, tp);
                } else {
                    ctx.record_step(Section::Tp, "Later ranged-hit TP",
                        &[("TP = 10 × (1 + Store TP)", "10 × (1 + {store_tp}) = {tp}")],
                        &[("store_tp", setup.stp), ("tp", tp)]);
                }
            }
            (physical, crit, tp)
        } else {
            (0.0, false, 0.0)
        };
        ctx.record_hit(HitEvent { kind: AttackKind::Ranged, proc: AttackProc::Normal,
            attempt: damage.attempts, landed, critical, physical_damage, magical_damage: 0.0, tp_return });
    }
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Hits, "Attempted hits", &[("Attempted hits", "{attempts}")],
            &[("attempts", f64::from(damage.attempts))]);
        ctx.record_step(Section::Damage, "Physical damage", &[("Physical = Σ Hit damage", "{physical}")],
            &[("physical", damage.physical)]);
        ctx.record_step(Section::Tp, "WS TP return", &[("TP return = Σ Hit TP", "{tp}")],
            &[("tp", damage.tp)]);
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

fn record_magical_dstat<R: RandomSource>(
    ctx: &mut EvaluationContext<R>,
    player: &Player,
    enemy: &Enemy,
    request: &WsRequest,
    dstat: f64,
) {
    match request.skill.spec().dstat {
        DStatFormula::Zero => {}
        DStatFormula::IntCapped => ctx.record_step(Section::Damage, "Magical WS dSTAT",
            &[("dSTAT = min(32, (INT − Target INT) / 2 + 8)", "min(32, ({player_int} − {enemy_int}) / 2 + 8) = {dstat}")],
            &[("player_int", player.stats.get(Stat::Int)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::IntDouble => ctx.record_step(Section::Damage, "Magical WS dSTAT",
            &[("dSTAT = (INT − Target INT) × 2", "({player_int} − {enemy_int}) × 2 = {dstat}")],
            &[("player_int", player.stats.get(Stat::Int)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::AgiDouble => ctx.record_step(Section::Damage, "Magical WS dSTAT",
            &[("dSTAT = (AGI − Target INT) × 2", "({player_agi} − {enemy_int}) × 2 = {dstat}")],
            &[("player_agi", player.stats.get(Stat::Agi)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::MndDouble => ctx.record_step(Section::Damage, "Magical WS dSTAT",
            &[("dSTAT = (MND − Target MND) × 2", "({player_mnd} − {enemy_mnd}) × 2 = {dstat}")],
            &[("player_mnd", player.stats.get(Stat::Mnd)), ("enemy_mnd", enemy.stats.get(Stat::Mnd)),
                ("dstat", dstat)]),
        DStatFormula::PrimalRend => ctx.record_step(Section::Damage, "Magical WS dSTAT",
            &[("dSTAT = min(651, (CHR − Target INT) × 1.5)", "min(651, ({player_chr} − {enemy_int}) × 1.5) = {dstat}")],
            &[("player_chr", player.stats.get(Stat::Chr)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::Wildfire => ctx.record_step(Section::Damage, "Magical WS dSTAT",
            &[("dSTAT = min(1276, (AGI − Target INT) × 2)", "min(1276, ({player_agi} − {enemy_int}) × 2) = {dstat}")],
            &[("player_agi", player.stats.get(Stat::Agi)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
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
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Hits, "Magical WS accuracy",
            &[("dSTAT MAcc = dMAcc(INT − Target INT)", "dMAcc({player_int} − {enemy_int}) = {dstat_accuracy}"),
                ("MAcc = Base MAcc + dSTAT MAcc", "{base_accuracy} + {dstat_accuracy} = {magic_accuracy}")],
            &[("player_int", player.stats.get(Stat::Int)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat_accuracy", dstat_macc), ("base_accuracy", setup.magic_accuracy),
                ("magic_accuracy", *magic_accuracy)]);
        let lines: &'static [(&'static str, &'static str)] = if enemy_magic_evasion > 0.0 {
            if *magic_accuracy - enemy_magic_evasion < 0.0 {
                &[("Magic hit rate = clamp(0.5 + trunc((MAcc − MEva) / 2) / 100, 0, 0.95)",
                    "clamp(0.5 + trunc(({magic_accuracy} − {magic_evasion}) / 2) / 100, 0, 0.95) = {hit_rate}")]
            } else {
                &[("Magic hit rate = clamp(0.5 + trunc(MAcc − MEva) / 100, 0, 0.95)",
                    "clamp(0.5 + trunc({magic_accuracy} − {magic_evasion}) / 100, 0, 0.95) = {hit_rate}")]
            }
        } else {
            &[("Target MEva", "{magic_evasion}"), ("Magic hit rate", "{hit_rate}")]
        };
        ctx.record_step(Section::Hits, "Magical WS hit probability", lines,
            &[("magic_accuracy", *magic_accuracy), ("magic_evasion", enemy_magic_evasion),
                ("hit_rate", hit_rate)]);
    }
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
        if ctx.calculation_trace.is_some() {
            record_magical_dstat(ctx, player, enemy, request, setup.ws.dstat);
            ctx.record_step(Section::Damage, "Magical WS base",
                &[("Base = trunc((152 + trunc((119 − 99) × 2.45) + WSC) × fTP × (1 + Elemental WSD / 100) + dSTAT + mDMG)",
                    "trunc((152 + 49 + {wsc}) × {ftp} × (1 + {elemental_ws_damage} / 100) + {dstat} + {magic_damage}) = {base}")],
                &[("wsc", setup.ws.wsc), ("ftp", setup.ftp),
                    ("elemental_ws_damage", player.stats.get(Stat::ElementalWsDamagePct)),
                    ("dstat", setup.ws.dstat), ("magic_damage", magic_damage_stat), ("base", base)]);
            record_tp_gain(ctx, "Magical WS TP", hit_rate, setup.mdelay, setup.stp, false, magic_tp);
        }
        base
    };
    if ctx.calculation_trace.is_some() && setup.ws.hybrid {
        ctx.record_step(Section::Damage, "Hybrid magical base",
            &[("Base = Physical × Hybrid fTP + mDMG", "{physical_damage} × {hybrid_ftp} + {magic_damage} = {base}")],
            &[("physical_damage", damage.physical), ("hybrid_ftp", setup.ftp_hybrid),
                ("magic_damage", magic_damage_stat), ("base", base)]);
        ctx.record_step(Section::Tp, "Hybrid magical TP", &[("Magical TP", "{magic_tp}")],
            &[("magic_tp", magic_tp)]);
    }
    let resist = if simulation {
        get_resist_state(hit_rate, &mut ctx.random)
    } else {
        get_resist_state_average(hit_rate)
    };
    if ctx.calculation_trace.is_some() {
        if simulation {
            ctx.record_step(Section::Damage, "Magical resistance",
                &[("Fail = I([random resistance roll: U(0,1)] > Magic hit rate)",
                    "I([random resistance roll: U(0,1)] > {hit_rate})"),
                    ("Resist = 0.5^min(3, Fails before first success)", "")],
                &[("hit_rate", hit_rate)]);
        } else {
            ctx.record_step(Section::Damage, "Expected magical resistance",
                &[("E[Resist] = p + 0.5 × p × (1 − p) + 0.25 × p × (1 − p)² + 0.125 × (1 − p)³",
                    "{p} + 0.5 × {p} × (1 − {p}) + 0.25 × {p} × (1 − {p})² + 0.125 × (1 − {p})³ = {resistance}")],
                &[("p", hit_rate), ("resistance", resist)]);
        }
    }
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
    let pre_ws_damage = base * multiplier;
    damage.magical = pre_ws_damage;
    damage.magical *= (1.0 + setup.wsd) * (1.0 + setup.ws_bonus) * (1.0 + setup.ws_trait)
        * (1.0 + setup.hover_shot)
        * (1.0 + true_shot * flag(setup.ws.hybrid) * flag(request.ws_type == WsType::Ranged));
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Damage, "Magic Attack / Defense ratio",
            &[("MAB / MDB = (100 + MAB) / (100 + MDB)",
                "(100 + {magic_attack}) / (100 + {magic_defense}) = {ratio}")],
            &[("magic_attack", magic_attack), ("magic_defense", enemy_magic_defense),
                ("ratio", magic_attack_ratio)]);
        ctx.record_step(Section::Damage, setup.ws.element.name(),
            &[("Elemental bonus = 1 + (Element bonus / 100 + All-element bonus / 100)",
                "1 + ({element_bonus} / 100 + {all_elements} / 100) = {elemental}"),
                ("Affinity = 1 + 0.05 × Affinity level + 0.05 × I(Affinity level > 0)",
                    "1 + 0.05 × {affinity_value} + 0.05 × I({affinity_value} > 0) = {affinity}")],
            &[("element_bonus", bonus_stat.map_or(0.0, |stat| player.stats.get(stat))),
                ("all_elements", player.stats.get(Stat::ElementalBonus)), ("elemental", element_bonus),
                ("affinity_value", affinity_value), ("affinity", affinity)]);
        ctx.record_step(Section::Damage, "Day/weather and Klimaform",
            &[("Day & Weather", "{dayweather}"),
                ("Klimaform = 1 + I(Klimaform) × Klimaform dmg / 100",
                    "1 + {klimaform_on} × {klimaform_damage} / 100 = {klimaform}")],
            &[("dayweather", dayweather),
                ("klimaform_on", flag(player.abilities.enabled(Ability::Klimaform))),
                ("klimaform_damage", player.stats.get(Stat::KlimaformDamagePct)), ("klimaform", klimaform)]);
        let lines: &'static [(&'static str, &'static str)] = if simulation {
            &[("Multiplier = Resist × MAB/MDB × Elemental × Day & Weather × Klimaform × (1 − MDT / 100) × Affinity × (1 + 0.25 × Magic crit)",
                "[random resistance multiplier] × {attack_ratio} × {elemental} × {dayweather} × {klimaform} × (1 − {enemy_mdt} / 100) × {affinity} × (1 + 0.25 × I([random magic critical roll: U(0,1)] < {crit_rate})) = {multiplier}"),
                ("Pre-WS magic = Base × Multiplier", "{base} × {multiplier} = {pre_ws}")]
        } else {
            &[("Multiplier = E[Resist] × MAB/MDB × Elemental × Day & Weather × Klimaform × (1 − MDT / 100) × Affinity × (1 + 0.25 × Magic crit rate)",
                "{expected_resistance} × {attack_ratio} × {elemental} × {dayweather} × {klimaform} × (1 − {enemy_mdt} / 100) × {affinity} × (1 + 0.25 × {crit_rate}) = {multiplier}"),
                ("Pre-WS magic = Base × Multiplier", "{base} × {multiplier} = {pre_ws}")]
        };
        ctx.record_step(Section::Damage, "Magic before WS modifiers", lines,
            &[("expected_resistance", resist), ("attack_ratio", magic_attack_ratio),
                ("elemental", element_bonus), ("dayweather", dayweather), ("klimaform", klimaform),
                ("enemy_mdt", enemy_mdt), ("affinity", affinity), ("crit_rate", magic_crit_rate),
                ("multiplier", multiplier), ("base", base), ("pre_ws", pre_ws_damage)]);
        ctx.record_step(Section::Damage, "Magical WS damage",
            &[("Magical = Pre-WS magic × (1 + WSD) × (1 + Weapon bonus) × (1 + WSD trait) × (1 + Hover Shot) × (1 + True Shot × I(Ranged hybrid))",
                "{pre_ws} × (1 + {wsd}) × (1 + {weapon_bonus}) × (1 + {ws_trait}) × (1 + {hover_shot}) × (1 + {true_shot} × {ranged_hybrid}) = {magic}")],
            &[("pre_ws", pre_ws_damage), ("wsd", setup.wsd),
                ("weapon_bonus", setup.ws_bonus), ("ws_trait", setup.ws_trait),
                ("hover_shot", setup.hover_shot), ("true_shot", true_shot),
                ("ranged_hybrid", flag(setup.ws.hybrid) * flag(request.ws_type == WsType::Ranged)),
                ("magic", damage.magical)]);
    }
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
    const HIT_RATES: &[(&str, &str)] = &[
        ("p1 = clamp((75 + 0.5 × (Accuracy + 100 − Evasion)) / 100, 0.2, Cap)",
            "clamp((75 + 0.5 × ({accuracy} + 100 − {evasion})) / 100, 0.2, {cap}) = {first}"),
        ("p2 = clamp((75 + 0.5 × (Accuracy − Evasion)) / 100, 0.2, Cap)",
            "clamp((75 + 0.5 × ({accuracy} − {evasion})) / 100, 0.2, {cap}) = {other}"),
    ];
    if request.ws_type == WsType::Melee && !setup.ws.magical {
        let melee = Melee::new(player, enemy, request, &setup)?;
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Damage, "Main-hand / kick fSTR",
                &[("fSTR = melee_fSTR(D, STR, VIT)", "melee_fSTR({weapon_dmg}, {str}, {vit}) = {fstr}")],
                &[("weapon_dmg", melee.main_dmg), ("str", player.stats.get(Stat::Str)),
                    ("vit", enemy.stats.get(Stat::Vit)), ("fstr", melee.fstr_main)]);
            let accuracy = setup.ws.player_accuracy1 + player.stats.get(Stat::WeaponSkillAccuracy);
            let cap = if ONE_HANDED_SKILLS.contains(&setup.main_skill)
                || setup.main_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
            ctx.record_step(Section::Hits, "Main-hand hit rates", HIT_RATES,
                &[("accuracy", accuracy), ("evasion", setup.enemy_evasion), ("cap", cap),
                    ("first", melee.hit11), ("other", melee.hit12)]);
            if setup.dual_wield {
                ctx.record_step(Section::Damage, "Offhand fSTR",
                    &[("fSTR = melee_fSTR(D, STR, VIT)", "melee_fSTR({weapon_dmg}, {str}, {vit}) = {fstr}")],
                    &[("weapon_dmg", melee.sub_dmg), ("str", player.stats.get(Stat::Str)),
                        ("vit", enemy.stats.get(Stat::Vit)), ("fstr", melee.fstr_sub)]);
                let accuracy = setup.ws.player_accuracy2 + player.stats.get(Stat::WeaponSkillAccuracy);
                let cap = if setup.sub_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
                ctx.record_step(Section::Hits, "Offhand hit rates", HIT_RATES,
                    &[("accuracy", accuracy), ("evasion", setup.enemy_evasion), ("cap", cap),
                        ("first", melee.hit21), ("other", melee.hit22)]);
            }
            if setup.main_skill == SkillType::HandToHand {
                ctx.record_step(Section::Tp, "Hand-to-hand TP delay",
                    &[("TP delay = Delay / 2", "{melee_delay} / 2 = {tp_delay}")],
                    &[("melee_delay", setup.mdelay), ("tp_delay", setup.melee_tp_delay())]);
            }
        }
        first_melee_hit_rate = if simulation {
            sample_melee(player, request, single, &setup, &melee, &mut damage, ctx)?
        } else {
            average_melee(player, request, &setup, &melee, &mut damage, ctx)?
        };
    } else if request.ws_type == WsType::Ranged && !setup.ws.magical {
        let ranged = Ranged::new(player, enemy, &setup)?;
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Damage, "Ranged D and fSTR2",
                &[("D = Ranged D + Ammo D", "{ranged_dmg} + {ammo_dmg} = {weapon_dmg}"),
                    ("fSTR2 = ranged_fSTR2(Ranged D, STR, VIT)", "ranged_fSTR2({ranged_dmg}, {str}, {vit}) = {fstr}")],
                &[("ranged_dmg", player.stats.get(Stat::RangedDmg)),
                    ("ammo_dmg", player.stats.get(Stat::AmmoDmg)), ("weapon_dmg", ranged.weapon_dmg),
                    ("str", player.stats.get(Stat::Str)), ("vit", enemy.stats.get(Stat::Vit)),
                    ("fstr", ranged.fstr)]);
            let accuracy = setup.ws.player_rangedaccuracy + player.stats.get(Stat::WeaponSkillAccuracy);
            let cap = if player.abilities.enabled(Ability::Sharpshot) { 0.99 } else { 0.95 };
            ctx.record_step(Section::Hits, "Ranged hit rates", HIT_RATES,
                &[("accuracy", accuracy), ("evasion", setup.enemy_evasion), ("cap", cap),
                    ("first", ranged.hit1), ("other", ranged.hit2)]);
            ctx.record_step(Section::Damage, "Ranged crit rate",
                &[("Crit rate = min(1, WS crit rate + Ranged crit / 100)",
                    "min(1, {ws_crit_rate} + {ranged_crit} / 100) = {crit_rate}")],
                &[("ws_crit_rate", setup.crit_rate), ("ranged_crit", player.stats.get(Stat::RangedCritRate)),
                    ("crit_rate", ranged.crit_rate)]);
            ctx.record_step(Section::Tp, "Ranged TP delay",
                &[("TP delay = Ranged delay + Ammo delay", "{ranged_delay} + {ammo_delay} = {tp_delay}")],
                &[("ranged_delay", setup.ranged_delay), ("ammo_delay", setup.ammo_delay),
                    ("tp_delay", setup.ranged_delay + setup.ammo_delay)]);
        }
        first_ranged_hit_rate = ranged.hit1;
        if simulation { sample_ranged(&setup, &ranged, &mut damage, ctx)?; }
        else { average_ranged(&setup, &ranged, &mut damage, ctx)?; }
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
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Damage, "WS damage",
            &[("WS damage = Physical + Magical", "{physical} + {magical} = {total}")],
            &[("physical", damage.physical), ("magical", damage.magical), ("total", total)]);
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
            let weighted_accuracy = hit_rate * magic_accuracy;
            if ctx.calculation_trace.is_some() {
                ctx.record_step(Section::Result, "Magic accuracy metric",
                    &[("Metric = Hit rate × MAcc", "{hit_rate} × {magic_accuracy} = {metric}")],
                    &[("hit_rate", hit_rate), ("magic_accuracy", magic_accuracy),
                        ("metric", weighted_accuracy)]);
            }
            weighted_accuracy
        }
        _ => total,
    };
    if simulation && player.abilities.enabled(Ability::DamageLimitDps) && total > 99999.0 {
        if ctx.calculation_trace.is_some() {
            ctx.record_step(Section::Damage, "WS damage limit",
                &[("Damage = min(Damage, 99999)", "min({uncapped_damage}, 99999) = 99999")],
                &[("uncapped_damage", total)]);
        }
        total = 99999.0;
    }
    if ctx.calculation_trace.is_some() {
        ctx.record_step(Section::Result, "WS result",
            &[("Damage", "{damage}"), ("TP return", "{tp}"), ("Metric", "{metric}")],
            &[("damage", total), ("tp", damage.tp), ("metric", metric)]);
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
