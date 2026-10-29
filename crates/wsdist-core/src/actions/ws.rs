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
use crate::weaponskills::{DStatFormula, TpScaling, WsElement, WsScaling, weaponskill_info};

use super::common::{ONE_HANDED_SKILLS, oa_list, oa_tiers, record_tp_gain, record_tp_reference};

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
            record_tp_reference(ctx);
            ctx.record_equation("Effective TP",
                "Prebonus TP = Requested TP before TP Bonus\nEffective TP = max(1000, min(3000, Prebonus TP + TP Bonus)).\nEffective TP controls fTP and other TP-dependent WS scaling. Fotia retention uses the original Prebonus TP, without TP Bonus or this effective-TP clamp.",
                "Prebonus TP = {requested_tp}; Effective TP = max(1000, min(3000, {requested_tp} + {tp_bonus})) = {effective_tp}",
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
            if ws.hybrid {
                ctx.record_equation("Hybrid WS damage equation",
                    "BG Wiki reference:\nHybrid WS Damage = (WS Base Damage × pDIF) + (WS Base Damage × pDIF × Magic Damage Modifiers).\nThe reference magical component duplicates the first physical hit. Current evaluator: Hybrid WS Damage = Preceding physical WS total + Current hybrid magical damage.\nCurrent hybrid magical base = Preceding physical WS total × Hybrid fTP + mDMG.\nThe preceding total includes all analytical hit weights or sampled landed hits, including offhand damage and any ranged multipliers; the hybrid magical base is not truncated.",
                    "Hybrid fTP = {hybrid_ftp}; mDMG = {magic_damage}; the physical stages below supply the entire preceding physical WS total, not only its first hit.",
                    &[("hybrid_ftp", ftp_hybrid), ("magic_damage", player.stats.get(Stat::MagicDamage))]);
            }
            if !ws.magical {
                ctx.record_equation("Physical WS damage equation",
                    if request.ws_type == WsType::Ranged {
                        "BG Wiki reference:\nRanged WS Base Damage = floor((Weapon Base Damage + Ammo Damage + fSTR2 + WSC) × fTP)\nWS Damage = WS Base Damage × pDIF.\nCurrent evaluator: Physical WS Damage per landed hit = Current modified WS Base Damage × pDIF × Critical Damage factor.\nCurrent modified WS Base Damage = trunc((Weapon Base Damage + Ammo Damage + fSTR2 + WSC) × fTP × First-hit WSD factor × Weapon Skill Potency Bonus × WS Damage Boost + First-hit ability additions).\nThe current evaluator truncates toward zero only after these base modifiers/additions, not after each wiki stage; WSC and fSTR2 remain fractional."
                    } else {
                        "BG Wiki reference:\nMelee WS Base Damage = floor((Weapon Base Damage + fSTR + WSC) × fTP)\nWS Damage = WS Base Damage × pDIF.\nCurrent evaluator: Physical WS Damage per landed hit = Current modified WS Base Damage × pDIF × Critical Damage factor.\nCurrent modified WS Base Damage = trunc((Weapon Base Damage + fSTR + WSC) × fTP × First-hit WSD factor × Weapon Skill Potency Bonus × WS Damage Boost + First-hit ability additions).\nThe current evaluator truncates toward zero only after these base modifiers/additions, not after each wiki stage; WSC and fSTR remain fractional."
                    },
                    "First-hit WSD factor = 1 + {wsd}; modeled Weapon Skill Potency Bonus = 1 + {weapon_bonus}; WS Damage Boost factor = 1 + {ws_trait}. WSD applies only to the first main-hand/ranged physical hit, not offhand or later hits. Weapon Skill Potency Bonus and WS Damage Boost apply to every physical hit; a separate Augmented Weapon Skill Potency Bonus is not modeled.",
                    &[("wsd", wsd), ("weapon_bonus", ws_bonus), ("ws_trait", ws_trait)]);
            }
            if ws.magical || ws.hybrid {
                ctx.record_equation("Magical WS damage equation",
                    "BG Wiki reference:\nMagical WS Damage = Base Magical WS Damage × Affinity × Staff × Resist × Resistance Rank Reduction × Day & Weather × (MAB / MDB) × TMDA × Magic Critical Hit II Factor × WSD × Potency Multipliers × WS Damage Boost × Weapon Skill Potency Bonus × Augmented Weapon Skill Potency Bonus.\nThe wiki floors after each multiplication/division. Current evaluator: Magical WS Damage = Current magical WS base × Resist × (MAB / MDB) × Elemental Bonus × Day & Weather × Klimaform × TMDA × Affinity × Magic Critical Hit II Factor × WSD × Weapon Skill Potency Bonus × WS Damage Boost × Hover Shot factor × Ranged-hybrid True Shot factor.\nThe current multiplier chain has no intermediate flooring/truncation. Staff, Resistance Rank Reduction and a separate Augmented Weapon Skill Potency Bonus are not modeled; MTDR, MB and MBB are not used by this WS branch. Day & Weather here is only matching Obi/storm weather.",
                    "WSD factor = 1 + {wsd}; modeled Weapon Skill Potency Bonus = 1 + {weapon_bonus}; WS Damage Boost factor = 1 + {ws_trait}. These apply to the whole magical component; for a hybrid they are applied again after the preceding physical total supplied its magical base.",
                    &[("wsd", wsd), ("weapon_bonus", ws_bonus), ("ws_trait", ws_trait)]);
            }
            for &(coefficient, stat) in request.skill.spec().wsc.terms() {
                let stat_value = player.stats.get(stat);
                ctx.record_equation(stat.name(),
                    "WSC skill term = Attribute × Weapon Skill attribute coefficient",
                    "WSC skill term = {coefficient} × {stat_value} = {term}",
                    &[("coefficient", coefficient), ("stat_value", stat_value),
                        ("term", coefficient * stat_value)]);
            }
            for contribution in &player.wsc {
                let stat_value = player.stats.get(contribution.stat);
                ctx.record_equation(contribution.stat.name(),
                    "WSC gear term = Attribute × Gear WSC percentage / 100",
                    "WSC gear term = {stat_value} × {percent}/100 = {term}",
                    &[("stat_value", stat_value), ("percent", contribution.percent),
                        ("term", stat_value * contribution.percent / 100.0)]);
            }
            ctx.record_equation("Weapon-skill stat contribution",
                "BG Wiki reference: WSC = floor((A × A%) + (B × B%)).\nCurrent evaluator: WSC = sum(skill stat terms) + sum(gear WSC terms), retaining fractional values without flooring.",
                "WSC = sum(skill stat terms) + sum(gear WSC terms) = {wsc}; no flooring",
                &[("wsc", ws.wsc)]);
            match request.skill.spec().ftp {
                TpScaling::Fixed(_) => ctx.record_equation("Weapon-skill fTP",
                    "Skill fTP = fixed Weapon Skill TP Multiplier",
                    "Skill fTP = {ftp}", &[("ftp", ws.ftp)]),
                TpScaling::Interpolated(points) => ctx.record_equation("Weapon-skill fTP",
                    "Skill fTP = linear interpolation of the Weapon Skill TP Multiplier at Effective TP",
                    "Skill fTP = linear_interpolation(TP = {tp}; 1000 → {ftp1000}, 2000 → {ftp2000}, 3000 → {ftp3000}) = {ftp}",
                    &[("tp", tp), ("ftp1000", points[0]), ("ftp2000", points[1]),
                        ("ftp3000", points[2]), ("ftp", ws.ftp)]),
            }
            ctx.record_equation("First-hit fTP",
                "First-hit fTP = Skill fTP + Equipment fTP / 256",
                "First-hit fTP = {ws_ftp} + {gear_ftp}/256 = {ftp}",
                &[("ws_ftp", ws.ftp), ("gear_ftp", player.stats.get(Stat::Ftp)), ("ftp", ftp)]);
            if !ws.magical {
                ctx.record_equation("Subsequent-hit fTP",
                    "Later-hit and offhand fTP = First-hit fTP when fTP replicates; otherwise 1.\nAdditional/multiattack hits use the same later-hit rule; replication does not grant first-main-hand WSD to those hits.",
                    "Later-hit / offhand fTP = if fTP replicates ({replicates}), then {ftp_first}, otherwise 1 = {ftp_other}",
                    &[("replicates", flag(ws.ftp_rep)), ("ftp_first", ftp), ("ftp_other", ftp2)]);
            }
            if ws.hybrid {
                if let Some(points) = request.skill.spec().ftp_hybrid {
                    ctx.record_equation("Hybrid magical fTP",
                        "Hybrid fTP = TP-interpolated hybrid magical multiplier + Equipment fTP / 256",
                        "Hybrid fTP = linear_interpolation(TP = {tp}; 1000 → {ftp1000}, 2000 → {ftp2000}, 3000 → {ftp3000}) + {gear_ftp}/256 = {ftp_hybrid}",
                        &[("tp", tp), ("ftp1000", points[0]), ("ftp2000", points[1]),
                            ("ftp3000", points[2]), ("gear_ftp", player.stats.get(Stat::Ftp)),
                            ("ftp_hybrid", ftp_hybrid)]);
                }
            }
            ctx.record_equation("WS Store TP",
                "Store TP Input = Store TP / 100\nFirst-hit full TP uses the delay-to-TP truncation stages; additional WS hits use 10 × (1 + Store TP Input), without a new truncation.",
                "Store TP Input = {store_tp}/100 = {store_tp_input}",
                &[("store_tp", player.stats.get(Stat::StoreTp)), ("store_tp_input", stp)]);
            if !ws.magical {
                ctx.record_equation("WS first-hit TP eligibility",
                    "First main-hand/ranged TP = First-attempt landing weight × Full delay-based TP after Store TP\nFirst offhand/second-fist TP = First offhand-attempt landing weight × Full delay-based TP after Store TP.\nThe analytical landing weights are probabilities; sampled weights are the actual landed outcomes. A first-attempt miss returns 0 TP, and a later landed hit never inherits full-return eligibility. Native later hits and WS multiattacks receive 10 base TP with Store TP instead.",
                    "Full-return and additional-hit stages below use Store TP Input = {store_tp}; no truncation is applied after fractional expected-hit weighting or to additional-hit TP.",
                    &[("store_tp", stp)]);
            }
            if ws.magical || request.ws_type == WsType::Melee {
                ctx.record_equation("WS TP delay",
                    "BG Wiki dual-wield TP delay per weapon = ((Main Delay + Offhand Delay) × (1 − Dual Wield / 100)) / 2.\nCurrent modified melee delay = ((Sheet Main Delay + Sheet Sub Delay) / 2 − Martial Arts) × (1 − Dual Wield / 100).\nThe sheet supplies duplicated main delay for single wield and total H2H delay for fists. Physical H2H first-hit TP halves this modified total below; pure magical WS deliberately uses it unhalved. Haste is not part of TP-generating delay.",
                    "Modified melee delay = (({main_delay} + {sub_delay})/2 − {martial_arts}) × (1 − {dual_wield}/100) = {delay}",
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

// These helpers only describe operands already used by the damage helpers.
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
    if let Some(first) = first.filter(|first| first.sneak_bonus != 0.0
        || first.trick_bonus != 0.0 || first.climactic_bonus != 0.0
        || first.striking_bonus != 0.0 || first.ternary_bonus != 0.0)
    {
        ctx.record_equation(label,
            "Current modified WS Base Damage = trunc(Unmultiplied WS base × fTP × First-hit WSD factor × Weapon Skill Potency Bonus × WS Damage Boost + Sneak Attack + Trick Attack + Climactic Flourish + Striking Flourish + Ternary Flourish).\nUnmultiplied WS base is Weapon Base Damage + fSTR + WSC for melee, or Weapon Base Damage + Ammo Damage + fSTR2 + WSC for ranged. This is the evaluator's combined, once-truncated base, not the wiki's separately floored WS Base Damage.",
            "Current modified WS Base Damage = trunc(({weapon_dmg} + {fstr} + {wsc}) × {ftp} × (1 + {wsd}) × (1 + {weapon_bonus}) × (1 + {ws_trait}) + {sneak} + {trick} + {climactic} + {striking} + {ternary}); additions are Sneak Attack, Trick Attack, Climactic Flourish, Striking Flourish, Ternary Flourish",
            &[("weapon_dmg", weapon_dmg), ("fstr", fstr), ("wsc", setup.ws.wsc),
                ("ftp", ftp), ("wsd", wsd), ("weapon_bonus", setup.ws_bonus),
                ("ws_trait", setup.ws_trait), ("sneak", first.sneak_bonus),
                ("trick", first.trick_bonus), ("climactic", first.climactic_bonus),
                ("striking", first.striking_bonus), ("ternary", first.ternary_bonus)]);
    } else {
        ctx.record_equation(label,
            "Current modified WS Base Damage = trunc(Unmultiplied WS base × fTP × First-hit WSD factor × Weapon Skill Potency Bonus × WS Damage Boost).\nUnmultiplied WS base is Weapon Base Damage + fSTR + WSC for melee, or Weapon Base Damage + Ammo Damage + fSTR2 + WSC for ranged. This is the evaluator's combined, once-truncated base, not the wiki's separately floored WS Base Damage.",
            "Current modified WS Base Damage = trunc(({weapon_dmg} + {fstr} + {wsc}) × {ftp} × (1 + {wsd}) × (1 + {weapon_bonus}) × (1 + {ws_trait})); the weapon-damage operand includes Ammo Damage for ranged",
            &[("weapon_dmg", weapon_dmg), ("fstr", fstr), ("wsc", setup.ws.wsc),
                ("ftp", ftp), ("wsd", wsd), ("weapon_bonus", setup.ws_bonus),
                ("ws_trait", setup.ws_trait)]);
    }
    if let Some(pdif) = average_pdif {
        ctx.record_equation("Expected physical damage per landed hit",
            "Current analytical WS Damage per landed hit = Current modified WS Base Damage × Analytical pDIF × (1 + min(Critical Hit Rate, 1) × min(Critical Damage Bonus, 1)).\nAnalytical pDIF is the evaluator's midpoint approximation, not an integrated sampled expectation; only the modified WS base is truncated.",
            "E[WS Damage | landed] = Current modified WS Base Damage × {legacy_average_pdif} × (1 + min({crit_rate}, 1) × min({crit_damage}, 1)) = {physical}; no truncation after the modified WS base",
            &[("legacy_average_pdif", pdif), ("crit_rate", crit_rate),
                ("crit_damage", crit_dmg), ("physical", physical)]);
    } else {
        ctx.record_equation("Sampled physical damage",
            "Current sampled WS Damage per landed hit = Current modified WS Base Damage × Sampled pDIF × (1 + Critical × min(Critical Damage Bonus, 1)).\nCritical is the same critical-roll outcome already used for pDIF, not another draw; only the modified WS base is truncated.",
            "WS Damage = Current modified WS Base Damage × [random pDIF result] × (1 + indicator([random critical roll: U(0,1)] < {crit_rate}) × min({crit_damage}, 1)) = {physical}; no truncation after the modified WS base",
            &[("crit_rate", crit_rate), ("crit_damage", crit_dmg), ("physical", physical)]);
    }
}

fn record_sampled_pdif<R: RandomSource>(
    ctx: &mut EvaluationContext<R>,
    setup: &Setup,
    attack: f64,
    crit_rate: f64,
    ranged: bool,
) {
    ctx.record_equation(if ranged { "Sampled ranged pDIF" } else { "Sampled melee pDIF" },
        if ranged {
            "Attack/Defense Ratio = Ranged Attack / Target Defense\npDIF cap = (Ranged skill pDIF cap + PDL trait) × (1 + PDL gear)\nCritical = indicator(Critical roll < Critical Hit Rate)\nSampled ranged pDIF = clip(Ratio draw, 0, pDIF cap) × (1 + 0.25 × Critical).\nThe ratio draw uses the current evaluator's ranged lower/upper bounds for Attack/Defense Ratio."
        } else {
            "Attack/Defense Ratio = Attack / Target Defense\npDIF cap = (Melee skill pDIF cap + PDL trait) × (1 + PDL gear)\nCritical = indicator(Critical roll < Critical Hit Rate)\nwRatio = Attack/Defense Ratio + Critical\nSampled melee pDIF = (clip(Ratio draw, 0, pDIF cap) + Critical) × Final spread.\nThe ratio draw uses the current evaluator's melee lower/upper bounds for wRatio."
        },
        if ranged {
            "Attack/Defense Ratio = {attack} / {defense}; Critical = indicator([random critical roll: U(0,1)] < {crit_rate}); pDIF = clip([random pDIF roll: U(lower,upper)], 0, pDIF cap) × (1 + 0.25 × Critical); pDIF cap uses the ranged skill cap, PDL trait {pdl_trait} and PDL gear {pdl_gear}"
        } else {
            "Attack/Defense Ratio = {attack} / {defense}; Critical = indicator([random critical roll: U(0,1)] < {crit_rate}); pDIF = (clip([random pDIF roll: U(lower,upper)], 0, pDIF cap) + Critical) × [random spread: U(1,1.05)]; bounds use wRatio = {attack} / {defense} + Critical; pDIF cap uses the melee skill cap, PDL trait {pdl_trait} and PDL gear {pdl_gear}"
        },
        &[("attack", attack), ("defense", setup.enemy_defense),
            ("pdl_trait", setup.pdl_trait), ("pdl_gear", setup.pdl_gear),
            ("crit_rate", crit_rate)]);
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
    ctx.record_equation(label,
        if ranged {
            "Attack/Defense Ratio = Ranged Attack / Target Defense\npDIF cap = (Ranged skill pDIF cap + PDL trait) × (1 + PDL gear)\nAnalytical ranged pDIF = clip((Lower pDIF bound + Upper pDIF bound) / 2, 0, pDIF cap) × (1 + 0.25 × min(Critical Hit Rate, 1)).\nBounds use the current evaluator's ranged Attack/Defense Ratio windows; this is a clipped-midpoint approximation."
        } else {
            "Attack/Defense Ratio = Attack / Target Defense\npDIF cap = (Melee skill pDIF cap + PDL trait) × (1 + PDL gear)\nwRatio = Attack/Defense Ratio + min(Critical Hit Rate, 1)\nAnalytical melee pDIF = (clip((Lower pDIF bound + Upper pDIF bound) / 2, 0, pDIF cap) + min(Critical Hit Rate, 1)) × 1.025.\nBounds use the current evaluator's melee wRatio windows; this correlated fractional-critical midpoint approximation is not an integrated sampled mean."
        },
        if ranged {
            "Attack/Defense Ratio = {attack} / {defense}; analytical pDIF = clip(midpoint of ranged bounds, 0, pDIF cap) × (1 + 0.25 × min({crit_rate}, 1)) = {pdif}; pDIF cap uses the ranged skill cap, PDL trait {pdl_trait} and PDL gear {pdl_gear}"
        } else {
            "wRatio = {attack} / {defense} + min({crit_rate}, 1); analytical pDIF = (clip(midpoint of melee bounds, 0, pDIF cap) + min({crit_rate}, 1)) × 1.025 = {pdif}; pDIF cap uses the melee skill cap, PDL trait {pdl_trait} and PDL gear {pdl_gear}"
        },
        &[("attack", attack), ("defense", setup.enemy_defense),
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
        ctx.record_calculation("Expected melee landed hits",
            "legacy_MA_expectation(base hits = {base_hits}, QA = {qa}, TA = {ta}, DA = {da}, dual wield = {dual_wield}, first hit rates = [{main_first}, {sub_first}], later hit rates = [{main_later}, {sub_later}], Striking = {striking}, Ternary = {ternary}, eight-expected-landed-hit budget approximation): E[main hits] = {main_hits}, E[offhand hits] = {sub_hits}; this matrix precedes Sneak/Trick weighting overrides",
            &[("base_hits", f64::from(setup.ws.nhits)), ("qa", setup.qa), ("ta", setup.ta),
                ("da", setup.da), ("dual_wield", flag(setup.dual_wield)),
                ("main_first", melee.hit11), ("sub_first", melee.hit21),
                ("main_later", melee.hit12), ("sub_later", melee.hit22),
                ("striking", flag(first.striking)), ("ternary", flag(first.ternary)),
                ("main_hits", hits.main), ("sub_hits", hits.sub)]);
        for (stat, rate) in [Stat::Oa3Main, Stat::Oa2Main, Stat::Oa8Sub, Stat::Oa7Sub,
            Stat::Oa6Sub, Stat::Oa5Sub, Stat::Oa4Sub, Stat::Oa3Sub, Stat::Oa2Sub]
            .into_iter().zip(oa_list(&player.stats))
        {
            if rate != 0.0 {
                ctx.record_calculation(stat.name(),
                    "OA probability supplied to legacy_MA_expectation = {rate}",
                    &[("rate", rate)]);
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
        record_average_pdif(ctx, "Legacy average main-hand pDIF", setup,
            setup.ws.player_attack1, setup.crit_rate, main_pdif, false);
        record_physical(ctx, "Main-hand baseline physical base", setup, melee.main_dmg,
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
        record_average_pdif(ctx, "Legacy average first-hit pDIF", setup,
            setup.ws.player_attack1, first.crit_rate, first_pdif, false);
        record_physical(ctx, "First main-hand physical base", setup, melee.main_dmg,
            melee.fstr_main, setup.ftp, setup.wsd, Some(&first), first.crit_rate,
            first.crit_dmg, Some(first_pdif), first_damage);
        ctx.record_equation("First and subsequent main-hand weighting",
            "Current analytical main-hand WS Damage = Expected main-hand landed hits × Later-hit WS Damage + First-hit landing probability × (First-hit WS Damage − Later-hit WS Damage).\nThe correction replaces one baseline hit with first-hit fTP, WSD and ability additions. Its first-hit weight includes any analytical Sneak/Trick override.",
            "E[main-hand WS Damage] = {main_hits} × {other_damage} + {first_damage} × {first_hit_rate} − {other_damage} × {first_hit_rate} = {physical}; the hit-count matrix was computed before the Sneak/Trick weighting override",
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
            record_average_pdif(ctx, "Legacy average Striking Flourish pDIF", setup,
                setup.ws.player_attack1, striking_rate, pdif, false);
            record_physical(ctx, "Striking Flourish extra-hit physical base", setup,
                melee.main_dmg, melee.fstr_main, setup.ftp2, 0.0, None,
                striking_rate, setup.crit_dmg, Some(pdif), striking_damage);
            ctx.record_equation("Striking Flourish correction",
                "Current analytical Striking Flourish correction = (Striking extra-hit WS Damage − Baseline later-hit WS Damage) × First-hit landing probability.\nThis extra hit uses later-hit fTP and no first-hit WSD; the correction is specific to the analytical evaluator.",
                "E[main-hand WS Damage] += ({striking_damage} − {baseline_damage}) × {first_hit_rate}; corrected E[main-hand WS Damage] = {physical}",
                &[("striking_damage", striking_damage), ("baseline_damage", main_damage),
                    ("first_hit_rate", hit11), ("physical", damage.physical)]);
        }
    }
    let offhand_pdif = get_avg_pdif_melee(attack2, setup.sub_skill.into(), setup.pdl_trait,
        setup.pdl_gear, setup.enemy_defense, setup.crit_rate)?;
    let offhand_damage = get_avg_phys_damage(melee.sub_dmg, melee.fstr_sub, setup.ws.wsc,
        offhand_pdif, setup.ftp2, setup.crit_rate, setup.crit_dmg, 0.0, setup.ws_bonus,
        setup.ws_trait, 0.0, 0.0, 0.0, 0.0, 0.0)?;
    damage.physical += offhand_damage * hits.sub;
    if ctx.calculation_trace.is_some() && setup.dual_wield {
        record_average_pdif(ctx, "Legacy average offhand pDIF", setup,
            attack2, setup.crit_rate, offhand_pdif, false);
        record_physical(ctx, "Offhand physical base", setup, melee.sub_dmg,
            melee.fstr_sub, setup.ftp2, 0.0, None, setup.crit_rate, setup.crit_dmg,
            Some(offhand_pdif), offhand_damage);
        ctx.record_equation("Offhand weighting",
            "Current analytical offhand WS Damage = Offhand WS Damage per landed hit × Expected offhand landed hits.\nThe offhand uses its own weapon damage/fSTR and later-hit fTP, without first-main-hand WSD or first-hit ability additions; Weapon Skill Potency Bonus and WS Damage Boost still apply.",
            "E[physical WS Damage] += {offhand_damage} × {offhand_hits}; combined E[physical WS Damage] = {physical}",
            &[("offhand_damage", offhand_damage), ("offhand_hits", hits.sub),
                ("physical", damage.physical)]);
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
        ctx.record_equation("Expected subsequent-hit TP",
            "Expected additional-hit TP = 10 × (1 + Store TP / 100) × (Expected main-hand landed hits + Expected offhand landed hits − First main-hand landing probability − First offhand landing probability).\nThese include native later hits and WS multiattacks. The evaluator retains fractional hit weights and the complete 10 × Store TP product without truncation; the hit-count matrix still precedes the analytical Sneak/Trick weighting override.",
            "E[other TP] = 10 × (1 + {store_tp}) × ({main_hits} + {sub_hits} − {main_first} − {sub_first}) = {other_tp}; no truncation",
            &[("store_tp", setup.stp), ("main_hits", hits.main), ("sub_hits", hits.sub),
                ("main_first", hit11), ("sub_first", hit21), ("other_tp", other_tp)]);
        if setup.fotia_gorget || setup.fotia_belt {
            ctx.record_equation("Legacy expected Fotia retention",
                "Current expected Fotia TP = Prebonus TP × (0.01 × Fotia Gorget equipped) × (0.01 × Fotia Belt equipped).\nThis analytical product is retained: one piece contributes 0, while both contribute 0.0001 × Prebonus TP. It is not the sampled additive proc chance, and uses original requested TP, not Effective TP. Retention is independent of landed hits and receives no Store TP multiplier.",
                "E[retained TP] = {requested_tp} × (0.01 × {gorget}) × (0.01 × {belt}) = {retained_tp}; analytical product differs from the sampled summed proc chance",
                &[("requested_tp", request.tp), ("gorget", flag(setup.fotia_gorget)),
                    ("belt", flag(setup.fotia_belt)), ("retained_tp", fotia_tp)]);
        }
        if player.stats.get(Stat::ConserveTp) != 0.0 {
            ctx.record_equation("Legacy expected Conserve TP",
                "Current expected Conserve TP = 95 × min(1, Conserve TP / 100).\nOnly the upper probability bound is capped; negative inputs remain negative. The analytical 95 constant is retained rather than the sampled truncated U(10,200) amount. This addition is independent of landed hits and is not multiplied by Store TP.",
                "E[Conserve TP] = 95 × min(1, {conserve_tp}/100) = {retained_tp}",
                &[("conserve_tp", player.stats.get(Stat::ConserveTp)),
                    ("retained_tp", conserve_tp)]);
        }
        ctx.record_equation("Expected melee TP return",
            "Expected melee WS TP return = First main-hand full TP + First offhand/second-fist full TP + Additional-hit TP + Expected Fotia TP + Expected Conserve TP.\nThe evaluator adds these stages in this order without a final TP truncation or cap.",
            "E[TP return] = {main_tp} + {offhand_tp} + {other_tp} + {fotia_tp} + {conserve_tp} = {total_tp}",
            &[("main_tp", main_tp), ("offhand_tp", sub_tp), ("other_tp", other_tp),
                ("fotia_tp", fotia_tp), ("conserve_tp", conserve_tp), ("total_tp", damage.tp)]);
    }
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
            if ctx.calculation_trace.is_some() {
                ctx.record_equation(if kind == AttackKind::Offhand {
                    "Sampled subsequent offhand hit"
                } else {
                    "Sampled subsequent main-hand hit"
                }, "Sampled additional-hit TP = 10 × (1 + Store TP / 100) on a landed later/native/multiattack attempt; 0 on a miss.\nLater hits do not inherit first-hit full TP, even if an earlier attempt missed.",
                    "attempt {attempt}: landed = indicator([random hit roll: U(0,1)] < {hit_rate}) = {landed}; a missed attempt returns 0 TP",
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
                    record_sampled_pdif(ctx, setup, attack, setup.crit_rate, false);
                    record_physical(ctx, "Subsequent-hit physical base", setup,
                        weapon_dmg, fstr, setup.ftp2, 0.0, None, setup.crit_rate,
                        setup.crit_dmg, None, physical);
                    ctx.record_equation("Sampled subsequent-hit TP",
                        "Additional landed WS-hit TP = 10 × (1 + Store TP / 100).\nThis applies to later native and multiattack hits from either hand, without truncation.",
                        "TP_landed = 10 × (1 + {store_tp}) = {tp}; no truncation",
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
    let label = match proc {
        AttackProc::QuadAttack => "Sampled Quad Attack proc",
        AttackProc::TripleAttack => "Sampled Triple Attack proc",
        AttackProc::DoubleAttack => "Sampled Double Attack proc",
        AttackProc::Occasionally(_) => "Sampled occasionally-attacks proc",
        _ => "Sampled multiattack proc",
    };
    ctx.record_calculation(label,
        "hand = {offhand} (0 main, 1 offhand): proc = indicator([random multiattack proc roll: U(0,1)] < {probability}) = {activated}; selected proc adds up to {extra_hits} attempts after {attempts} attempted hits, with a total cap of 8; later tiers are skipped on success",
        &[("offhand", flag(offhand)), ("probability", rate), ("activated", flag(activated)),
            ("extra_hits", f64::from(hits)), ("attempts", f64::from(attempts))]);
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
        ctx.record_equation("Sampled first main-hand hit",
            "First main-hand TP = Full delay-based TP after Store TP when the first main-hand attempt lands; 0 when it misses.\nLater landed hits never inherit this first-attempt full TP return. Sneak/Trick do not override the sampled hit rate.",
            "attempt {attempt}: landed = indicator([random hit roll: U(0,1)] < {hit_rate}) = {landed}; first-attempt miss gives 0 TP; Sneak/Trick do not override this sampled hit rate",
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
            record_sampled_pdif(ctx, setup, setup.ws.player_attack1, first.crit_rate, false);
            record_physical(ctx, "First main-hand physical base", setup,
                melee.main_dmg, melee.fstr_main, setup.ftp, setup.wsd, Some(&first),
                first.crit_rate, first.crit_dmg, None, physical);
            record_tp_gain(ctx, "Sampled first main-hand TP", 1.0,
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
            ctx.record_equation("Sampled first offhand hit",
                "First offhand/second-fist TP = Full delay-based TP after Store TP when its first attempt lands; 0 when it misses.\nThis eligibility is separate from the main-hand first attempt; later offhand hits receive additional-hit TP only.",
                "attempt {attempt}: landed = indicator([random hit roll: U(0,1)] < {hit_rate}) = {landed}; first-offhand-attempt miss gives 0 TP",
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
                record_sampled_pdif(ctx, setup, setup.ws.player_attack2, setup.crit_rate, false);
                record_physical(ctx, "First offhand physical base", setup,
                    melee.sub_dmg, melee.fstr_sub, setup.ftp2, 0.0, None,
                    setup.crit_rate, setup.crit_dmg, None, physical);
                record_tp_gain(ctx, "Sampled first offhand TP", 1.0,
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
        ctx.record_equation("Sampled Fotia TP retention",
            "Sampled Fotia chance = 0.01 × Fotia Gorget equipped + 0.01 × Fotia Belt equipped\nSampled Fotia TP = indicator(Fotia proc roll < Sampled Fotia chance) × Prebonus TP.\nThe additive odds differ from the analytical product. Retention uses original requested TP, not Effective TP, is independent of landed hits, and is not multiplied by Store TP.",
            "proc chance = 0.01 × {gorget} + 0.01 × {belt} = {probability}; TP_added = indicator([random Fotia retention roll: U(0,1)] < {probability}) × {requested_tp}; proc outcome = {activated}; retained amount on activation is Prebonus TP = {requested_tp}",
            &[("gorget", flag(setup.fotia_gorget)), ("belt", flag(setup.fotia_belt)),
                ("probability", fotia_chance), ("requested_tp", request.tp),
                ("activated", flag(fotia_retained))]);
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
        ctx.record_equation("Sampled Conserve TP proc",
            "Sampled Conserve TP chance = Conserve TP / 100\nConserve TP activated = indicator(Conserve TP proc roll < Sampled Conserve TP chance).\nUnlike the analytical expression, the sampled probability input is not explicitly capped. A successful proc adds its separately sampled amount, independently of landed hits and without Store TP.",
            "proc = indicator([random Conserve TP proc roll: U(0,1)] < {probability}) = {activated}",
            &[("probability", conserve_tp_chance), ("activated", flag(conserved))]);
    }
    if conserved {
        let tp = python_int(ctx.random.uniform(RandomStream::NumpyHost, 10.0, 200.0), OPERATION, "Conserve TP")?;
        damage.tp += tp;
        if ctx.calculation_trace.is_some() {
            ctx.record_equation("Sampled Conserve TP amount",
                "Sampled Conserve TP amount = trunc(Conserve TP amount draw from U(10,200)), only after a successful proc.\nThe sampled integer amount is 10 through 199, not the analytical 95 constant; it is not multiplied by Store TP.",
                "TP_added = trunc([random Conserve TP amount: U(10,200)]) = {tp}",
                &[("tp", tp)]);
        }
        ctx.record_hit(HitEvent { kind: AttackKind::TpRetention, proc: AttackProc::ConserveTp,
            attempt: 0, landed: true, critical: false, physical_damage: 0.0,
            magical_damage: 0.0, tp_return: tp });
    }
    if ctx.calculation_trace.is_some() {
        ctx.record_equation("Sampled melee TP return",
            "Sampled melee WS TP return = First main-hand full TP + First offhand/second-fist full TP + Sum(additional landed-hit TP) + Sampled Fotia TP + Sampled Conserve TP.\nMissed attempts add 0 TP. Retention is checked independently after the existing at-most-eight-attempt melee walk; the final TP return is not capped or truncated.",
            "TP return = sum(landed hit TP) + Fotia retention + Conserve TP = {tp}",
            &[("tp", damage.tp)]);
        ctx.record_equation("Sampled melee totals",
            "Current sampled physical melee WS Damage = sum(landed main-hand and offhand WS hit damage).\nThe sampled walk has at most eight attempted hits; each landed hit keeps its own pDIF, first-hit/later-hit fTP and WSD eligibility.",
            "Physical WS Damage = sum(landed hit damage) = {physical}; TP return = sum(landed hit TP) + Fotia retention + Conserve TP = {tp}; attempts = {attempts}, capped at 8",
            &[("physical", damage.physical), ("tp", damage.tp),
                ("attempts", f64::from(damage.attempts))]);
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
        record_average_pdif(ctx, "Legacy average ranged pDIF", setup,
            ranged.attack, ranged.crit_rate, pdif, true);
        record_physical(ctx, "First ranged physical base", setup,
            ranged.weapon_dmg, ranged.fstr, setup.ftp, setup.wsd, None,
            ranged.crit_rate, setup.crit_dmg, Some(pdif), first_base);
        ctx.record_equation("Expected first-hit Hover Shot multiplier",
            "Current analytical first ranged WS Damage per landed hit = First-hit WS Damage × (1 + Hover Shot)",
            "E[first ranged WS Damage | landed] = {physical} × (1 + {hover_shot}) = {first_damage}",
            &[("physical", first_base), ("hover_shot", setup.hover_shot),
                ("first_damage", first_damage)]);
        if setup.ws.nhits > 1 {
            record_physical(ctx, "Subsequent ranged physical base", setup,
                ranged.weapon_dmg, ranged.fstr, setup.ftp2, 0.0, None,
                ranged.crit_rate, setup.crit_dmg, Some(pdif), other_base);
            ctx.record_equation("Expected subsequent-hit Hover Shot multiplier",
                "Current analytical later ranged WS Damage per landed hit = Later-hit WS Damage × (1 + Hover Shot)\nLater hits use replicated fTP or fTP = 1, without first-hit WSD.",
                "E[later ranged WS Damage | landed] = {physical} × (1 + {hover_shot}) = {other_damage}",
                &[("physical", other_base), ("hover_shot", setup.hover_shot),
                    ("other_damage", other_damage)]);
        }
        ctx.record_equation("First and subsequent ranged weighting",
            "Current analytical ranged WS Damage = (First-hit WS Damage × First-hit landing probability + Later-hit WS Damage × Later-hit landing probability × (WS hit count − 1)) × (1 + True Shot).\nFirst-hit and later-hit damage above already include Hover Shot; only first-hit damage receives first-hit WSD.",
            "E[physical WS Damage] = ({first_damage} × {first_hit_rate} + {other_damage} × {other_hit_rate} × ({base_hits} − 1)) × (1 + {true_shot}) = {physical}",
            &[("first_damage", first_damage), ("first_hit_rate", ranged.hit1),
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
            ctx.record_equation("Expected subsequent ranged-hit TP",
                "Expected additional ranged WS TP = 10 × (1 + Store TP / 100) × (WS hit count − 1) × Later-hit landing probability.\nLater ranged WS hits use the 10-base-TP rule, not the ranged-delay full return. There is no additional truncation of the Store TP product or probability-weighted result.",
                "E[other TP] = 10 × (1 + {store_tp}) × ({base_hits} − 1) × {hit_rate} = {tp}; no truncation",
                &[("store_tp", setup.stp), ("base_hits", f64::from(setup.ws.nhits)),
                    ("hit_rate", ranged.hit2), ("tp", other_tp)]);
        }
        ctx.record_equation("Expected ranged TP return",
            "Expected ranged WS TP return = First ranged-hit full TP + Additional ranged-hit TP.\nThe ranged path has no Fotia or Conserve TP additions and no final TP truncation or cap.",
            "E[TP return] = {first_tp} + {other_tp} = {total_tp}",
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
            ctx.record_equation(if n == 0 { "Sampled first ranged hit" } else {
                "Sampled subsequent ranged hit"
            }, if n == 0 {
                "First ranged WS TP = Full ranged-plus-ammo-delay TP after Store TP when the first ranged attempt lands; 0 when it misses.\nA later landed hit never inherits full-return eligibility."
            } else {
                "Additional ranged WS TP = 10 × (1 + Store TP / 100) when the later attempt lands; 0 when it misses, without truncation."
            }, "attempt {attempt}: landed = indicator([random hit roll: U(0,1)] < {hit_rate}) = {landed}; a missed attempt returns 0 TP",
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
                record_sampled_pdif(ctx, setup, ranged.attack, ranged.crit_rate, true);
                record_physical(ctx, if n == 0 { "First ranged physical base" } else {
                    "Subsequent ranged physical base"
                }, setup, ranged.weapon_dmg, ranged.fstr,
                    if n == 0 { setup.ftp } else { setup.ftp2 },
                    if n == 0 { setup.wsd } else { 0.0 }, None,
                    ranged.crit_rate, setup.crit_dmg, None, base_physical);
                ctx.record_equation("Sampled ranged damage multipliers",
                    "Current sampled ranged WS hit damage = Physical WS hit damage × (1 + Hover Shot) × (1 + True Shot).\nPhysical WS hit damage above already includes that hit's fTP, pDIF, Weapon Skill Potency Bonus and WS Damage Boost; WSD applies only to the first hit.",
                    "Ranged WS hit damage = {physical} × (1 + {hover_shot}) × (1 + {true_shot}) = {damage}",
                    &[("physical", base_physical), ("hover_shot", setup.hover_shot),
                        ("true_shot", ranged.true_shot), ("damage", physical)]);
                if n == 0 {
                    record_tp_gain(ctx, "Sampled first ranged-hit TP", 1.0,
                        setup.ranged_delay + setup.ammo_delay, setup.stp, false, tp);
                } else {
                    ctx.record_equation("Sampled subsequent ranged-hit TP",
                        "Additional landed ranged WS-hit TP = 10 × (1 + Store TP / 100).\nThe evaluator retains the fractional Store TP product without truncation; a missed first attempt does not promote later hits to full TP.",
                        "TP_landed = 10 × (1 + {store_tp}) = {tp}; no truncation",
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
        ctx.record_equation("Sampled ranged TP return",
            "Sampled ranged WS TP return = First ranged-hit full TP + Sum(additional landed ranged-hit TP).\nMissed attempts add 0 TP. The ranged path has no Fotia or Conserve TP additions, and no final TP truncation or cap.",
            "TP return = sum(landed ranged-hit TP) = {tp}",
            &[("tp", damage.tp)]);
        ctx.record_equation("Sampled ranged totals",
            "Current sampled physical ranged WS Damage = sum(landed ranged WS hit damage).\nEach landed hit keeps its own sampled pDIF and the first-hit/later-hit fTP and WSD rules.",
            "Physical WS Damage = sum(landed ranged-hit damage) = {physical}; TP return = sum(landed ranged-hit TP) = {tp}; attempts = {attempts}",
            &[("physical", damage.physical), ("tp", damage.tp),
                ("attempts", f64::from(damage.attempts))]);
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
        DStatFormula::IntCapped => ctx.record_equation("Magical WS dSTAT",
            "Current magical WS dSTAT = min(32, (Player INT − Target INT) / 2 + 8)",
            "dSTAT = min(32, ({player_int} − {enemy_int})/2 + 8) = {dstat}",
            &[("player_int", player.stats.get(Stat::Int)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::IntDouble => ctx.record_equation("Magical WS dSTAT",
            "Current magical WS dSTAT = (Player INT − Target INT) × 2",
            "dSTAT = ({player_int} − {enemy_int}) × 2 = {dstat}",
            &[("player_int", player.stats.get(Stat::Int)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::AgiDouble => ctx.record_equation("Magical WS dSTAT",
            "Current magical WS dSTAT = (Player AGI − Target INT) × 2",
            "dSTAT = ({player_agi} − {enemy_int}) × 2 = {dstat}",
            &[("player_agi", player.stats.get(Stat::Agi)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::MndDouble => ctx.record_equation("Magical WS dSTAT",
            "Current magical WS dSTAT = (Player MND − Target MND) × 2",
            "dSTAT = ({player_mnd} − {enemy_mnd}) × 2 = {dstat}",
            &[("player_mnd", player.stats.get(Stat::Mnd)), ("enemy_mnd", enemy.stats.get(Stat::Mnd)),
                ("dstat", dstat)]),
        DStatFormula::PrimalRend => ctx.record_equation("Magical WS dSTAT",
            "Current magical WS dSTAT = min(651, (Player CHR − Target INT) × 1.5)",
            "dSTAT = min(651, ({player_chr} − {enemy_int}) × 1.5) = {dstat}",
            &[("player_chr", player.stats.get(Stat::Chr)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat", dstat)]),
        DStatFormula::Wildfire => ctx.record_equation("Magical WS dSTAT",
            "Current magical WS dSTAT = min(1276, (Player AGI − Target INT) × 2)",
            "dSTAT = min(1276, ({player_agi} − {enemy_int}) × 2) = {dstat}",
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
        ctx.record_calculation("Magical WS accuracy",
            "dSTAT_accuracy = dSTAT_macc(INT = {player_int}, enemy INT = {enemy_int}) = {dstat_accuracy}; magic accuracy = {base_accuracy} + {dstat_accuracy} = {magic_accuracy}",
            &[("player_int", player.stats.get(Stat::Int)), ("enemy_int", enemy.stats.get(Stat::Int)),
                ("dstat_accuracy", dstat_macc), ("base_accuracy", setup.magic_accuracy),
                ("magic_accuracy", *magic_accuracy)]);
        if enemy_magic_evasion > 0.0 {
            ctx.record_calculation("Magical WS hit probability",
                if *magic_accuracy - enemy_magic_evasion < 0.0 {
                    "p = clamp(0.50 + trunc(({magic_accuracy} − {magic_evasion})/2)/100, 0, 0.95) = {hit_rate}"
                } else {
                    "p = clamp(0.50 + trunc({magic_accuracy} − {magic_evasion})/100, 0, 0.95) = {hit_rate}"
                },
                &[("magic_accuracy", *magic_accuracy), ("magic_evasion", enemy_magic_evasion),
                    ("hit_rate", hit_rate)]);
        } else {
            ctx.record_calculation("Magical WS hit probability",
                "p = 1 because enemy magic evasion = {magic_evasion} is not positive",
                &[("magic_evasion", enemy_magic_evasion)]);
        }
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
            ctx.record_equation("Pure magical WS base",
                "BG Wiki reference:\nBase Magical WS Damage = floor(((152 + floor((WeaponLevel − 99) × 2.45) + WSC) × fTP) + dSTAT + mDMG).\nCurrent evaluator: Current magical WS base = trunc(((152 + trunc((119 − 99) × 2.45) + WSC) × fTP) × (1 + Elemental WS Damage / 100) + dSTAT + mDMG).\nWeaponLevel is fixed at 119, rather than read from equipment; there is no lower-level base branch. WSC remains fractional and Elemental WS Damage is applied inside this base. Only the level adjustment and the complete base are truncated toward zero; there is no physical stage.",
                "Current magical WS base = trunc(((152 + trunc((119 − 99) × 2.45) + {wsc}) × {ftp}) × (1 + {elemental_ws_damage}/100) + {dstat} + {magic_damage}) = {base}",
                &[("wsc", setup.ws.wsc), ("ftp", setup.ftp),
                    ("elemental_ws_damage", player.stats.get(Stat::ElementalWsDamagePct)),
                    ("dstat", setup.ws.dstat), ("magic_damage", magic_damage_stat), ("base", base)]);
            ctx.record_equation("Pure magical WS TP convention",
                "Current pure magical WS TP = Magic hit probability × Full TP after Store TP from the unhalved modified melee delay.\nThis probability-weighted TP is retained in both analytical and sampled modes: it is not a sampled landed/missed full-TP result. Even H2H uses setup.mdelay without the physical-fist halving, and a ranged magical WS does not switch to ranged-plus-ammo delay. Fotia and Conserve TP are not added; sampled resistance and magic-critical outcomes do not change this TP return.",
                "Magic hit probability = {hit_rate}; unhalved modified melee delay = {delay}; Store TP Input = {store_tp}; probability-weighted TP = {tp}, including in sampled mode",
                &[("hit_rate", hit_rate), ("delay", setup.mdelay),
                    ("store_tp", setup.stp), ("tp", magic_tp)]);
            record_tp_gain(ctx, "Magical WS TP return (hit-probability weighted)", hit_rate,
                setup.mdelay, setup.stp, false, magic_tp);
        }
        base
    };
    if ctx.calculation_trace.is_some() && setup.ws.hybrid {
        ctx.record_equation("Hybrid magical base",
            "Current hybrid magical base = Preceding physical WS total × Hybrid fTP + mDMG.\nUnlike the BG Wiki first-hit duplicate, this uses the entire analytical weighted or sampled landed physical total, including offhand and ranged modifiers. There is no additional base truncation; the magical chain then reapplies WSD, Weapon Skill Potency Bonus and WS Damage Boost.",
            "Current hybrid magical base = {physical_damage} × {hybrid_ftp} + {magic_damage} = {base}; no additional base truncation",
            &[("physical_damage", damage.physical), ("hybrid_ftp", setup.ftp_hybrid),
                ("magic_damage", magic_damage_stat), ("base", base)]);
        ctx.record_equation("Hybrid magical TP",
            "Hybrid WS TP return = Preceding physical-path TP return + 0 from the magical component.\nThe hybrid keeps its analytical or sampled melee/ranged hit TP, including melee retention where applicable; there is no second delay-based TP return from its magical component.",
            "Magical-component TP = {magic_tp}; WS TP return remains {tp}",
            &[("magic_tp", magic_tp), ("tp", damage.tp)]);
    }
    let resist = if simulation {
        get_resist_state(hit_rate, &mut ctx.random)
    } else {
        get_resist_state_average(hit_rate)
    };
    if ctx.calculation_trace.is_some() {
        if simulation {
            ctx.record_equation("Sampled magical resistance",
                "Resist = 2^(−failed checks before first success, at most 3).\nThe existing sampled resistance checks select Resist from 1, 1/2, 1/4 and 1/8; Resistance Rank Reduction is a different, unmodeled factor.",
                "Resist = 2^(−failed checks before first success, at most 3); each success is [random resistance roll: U(0,1)] ≤ {hit_rate}; possible multipliers are [1, 1/2, 1/4, 1/8]",
                &[("hit_rate", hit_rate)]);
        } else {
            ctx.record_equation("Expected magical resistance",
                "Expected Resist = p + (1/2) × p × (1 − p) + (1/4) × p × (1 − p)² + (1/8) × (1 − p)³, where p is the magic hit probability.\nThis averages resist states, not per-state wiki flooring; Resistance Rank Reduction is a different, unmodeled factor.",
                "E[Resist] = {p} + 0.5 × {p} × (1 − {p}) + 0.25 × {p} × (1 − {p})² + 0.125 × (1 − {p})³ = {resistance}",
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
        ctx.record_equation("Magic Attack / Defense ratio",
            "MAB / MDB = (100 + Magic Attack Bonus) / (100 + Target Magic Defense Bonus)",
            "MAB / MDB = (100 + {magic_attack}) / (100 + {magic_defense}) = {ratio}",
            &[("magic_attack", magic_attack), ("magic_defense", enemy_magic_defense),
                ("ratio", magic_attack_ratio)]);
        ctx.record_equation(setup.ws.element.name(),
            "Current Elemental Bonus = 1 + (Element-specific bonus + All-elements bonus) / 100\nAffinity = 1 + 0.05 × Affinity level + 0.05 × indicator(Affinity level > 0)",
            "Elemental Bonus = 1 + ({element_bonus}/100 + {all_elements}/100) = {elemental}; Affinity = 1 + 0.05 × {affinity_value} + 0.05 × indicator({affinity_value} > 0) = {affinity}",
            &[("element_bonus", bonus_stat.map_or(0.0, |stat| player.stats.get(stat))),
                ("all_elements", player.stats.get(Stat::ElementalBonus)), ("elemental", element_bonus),
                ("affinity_value", affinity_value), ("affinity", affinity)]);
        ctx.record_equation("Magic element / weather factors",
            "Current element/weather factors = Elemental Bonus × Day & Weather × Klimaform × Affinity.\nDay & Weather is 1.25 for a matching tier-II storm with Hachirin-no-Obi, 1.10 for a matching tier-I storm with Obi, otherwise 1; day bonuses are not modeled here. Klimaform = 1 + indicator(Klimaform enabled) × Klimaform Damage / 100.",
            "Elemental Bonus × Day & Weather × Klimaform × Affinity = {elemental} × {dayweather} × {klimaform} × {affinity}",
            &[("elemental", element_bonus), ("dayweather", dayweather),
                ("klimaform", klimaform), ("affinity", affinity)]);
        if simulation {
            ctx.record_equation("Sampled magic before WS modifiers",
                "Current sampled magical WS Damage before WS modifiers = Current magical WS base × Sampled Resist × (MAB / MDB) × Elemental Bonus × Day & Weather × Klimaform × TMDA × Affinity × Magic Critical Hit II Factor.\nTMDA = 1 − Target Magic Damage Taken / 100. Magic Critical Hit II Factor = 1 + 0.25 × indicator(Magic critical roll < Magic Critical Hit II Rate). No truncation/flooring follows the base.",
                "Magical WS Damage before WS modifiers = {base} × [random resistance multiplier] × {attack_ratio} × {elemental} × {dayweather} × {klimaform} × (1 − {enemy_mdt}/100) × {affinity} × (1 + 0.25 × indicator([random magic critical roll: U(0,1)] < {crit_rate})) = {pre_ws}; no truncation after the base",
                &[("base", base), ("attack_ratio", magic_attack_ratio), ("enemy_mdt", enemy_mdt),
                    ("crit_rate", magic_crit_rate), ("pre_ws", pre_ws_damage),
                    ("elemental", element_bonus), ("dayweather", dayweather),
                    ("klimaform", klimaform), ("affinity", affinity)]);
        } else {
            ctx.record_equation("Expected magic before WS modifiers",
                "Current analytical magical WS Damage before WS modifiers = Current magical WS base × Expected Resist × (MAB / MDB) × Elemental Bonus × Day & Weather × Klimaform × TMDA × Affinity × Expected Magic Critical Hit II Factor.\nTMDA = 1 − Target Magic Damage Taken / 100. Expected Magic Critical Hit II Factor = 1 + 0.25 × Magic Critical Hit II Rate; the evaluator still consumes its existing critical roll. This analytical result does not apply per-state or per-factor wiki flooring.",
                "E[Magical WS Damage before WS modifiers] = {base} × {expected_resistance} × {attack_ratio} × {elemental} × {dayweather} × {klimaform} × (1 − {enemy_mdt}/100) × {affinity} × (1 + 0.25 × {crit_rate}) = {pre_ws}; no truncation after the base",
                &[("base", base), ("expected_resistance", resist),
                    ("attack_ratio", magic_attack_ratio), ("enemy_mdt", enemy_mdt),
                    ("crit_rate", magic_crit_rate), ("pre_ws", pre_ws_damage),
                    ("elemental", element_bonus), ("dayweather", dayweather),
                    ("klimaform", klimaform), ("affinity", affinity)]);
        }
        ctx.record_equation(if simulation { "Sampled magical WS total" } else {
            "Expected magical WS total"
        }, "Current magical WS Damage = Magical WS Damage before WS modifiers × WSD factor × Weapon Skill Potency Bonus × WS Damage Boost × Hover Shot factor × Ranged-hybrid True Shot factor.\nWSD applies to the entire magical component. These modifiers are multiplied without the wiki's intermediate flooring, including when they were already present in a hybrid's physical contribution.",
            "Magical WS Damage = {pre_ws} × (1 + {wsd}) × (1 + {weapon_bonus}) × (1 + {ws_trait}) × (1 + {hover_shot}) × (1 + {ranged_hybrid_true_shot}) = {magic}; the final factor applies True Shot only to ranged hybrids",
            &[("pre_ws", pre_ws_damage), ("wsd", setup.wsd),
                ("weapon_bonus", setup.ws_bonus), ("ws_trait", setup.ws_trait),
                ("hover_shot", setup.hover_shot),
                ("ranged_hybrid_true_shot", true_shot * flag(setup.ws.hybrid)
                    * flag(request.ws_type == WsType::Ranged)), ("magic", damage.magical)]);
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
    if request.ws_type == WsType::Melee && !setup.ws.magical {
        let melee = Melee::new(player, enemy, request, &setup)?;
        if ctx.calculation_trace.is_some() {
            ctx.record_equation("Main-hand / kick base and fSTR",
                "Unmultiplied melee WS base = Weapon Base Damage + fSTR + WSC.\nfSTR is the current fractional Strength–Vitality contribution, capped using Weapon Base Damage / 9; the evaluator does not floor fSTR or WSC here.",
                "fSTR = melee_fSTR(Weapon Base Damage = {weapon_dmg}, STR = {str}, VIT = {vit}) = {fstr}; Unmultiplied melee WS base = {weapon_dmg} + {fstr} + {wsc} = {base}",
                &[("weapon_dmg", melee.main_dmg), ("str", player.stats.get(Stat::Str)),
                    ("vit", enemy.stats.get(Stat::Vit)), ("fstr", melee.fstr_main),
                    ("wsc", setup.ws.wsc),
                    ("base", melee.main_dmg + melee.fstr_main + setup.ws.wsc)]);
            let accuracy = setup.ws.player_accuracy1 + player.stats.get(Stat::WeaponSkillAccuracy);
            let cap = if ONE_HANDED_SKILLS.contains(&setup.main_skill)
                || setup.main_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
            ctx.record_calculation("Main-hand hit probabilities",
                "p_first = clamp((75 + 0.5 × ({accuracy} + 100 − {evasion}))/100, 0.2, {cap}) = {first}; p_other = clamp((75 + 0.5 × ({accuracy} − {evasion}))/100, 0.2, {cap}) = {other}",
                &[("accuracy", accuracy), ("evasion", setup.enemy_evasion), ("cap", cap),
                    ("first", melee.hit11), ("other", melee.hit12)]);
            if setup.dual_wield {
                ctx.record_equation("Offhand base and fSTR",
                    "Unmultiplied offhand WS base = Offhand Weapon Base Damage + Offhand fSTR + WSC.\nThe offhand uses later-hit fTP and no first-main-hand WSD or ability additions; offhand fSTR and WSC remain fractional.",
                    "Offhand fSTR = melee_fSTR(Offhand Weapon Base Damage = {weapon_dmg}, STR = {str}, VIT = {vit}) = {fstr}; Unmultiplied offhand WS base = {weapon_dmg} + {fstr} + {wsc} = {base}",
                    &[("weapon_dmg", melee.sub_dmg), ("str", player.stats.get(Stat::Str)),
                        ("vit", enemy.stats.get(Stat::Vit)), ("fstr", melee.fstr_sub),
                        ("wsc", setup.ws.wsc),
                        ("base", melee.sub_dmg + melee.fstr_sub + setup.ws.wsc)]);
                let accuracy = setup.ws.player_accuracy2 + player.stats.get(Stat::WeaponSkillAccuracy);
                let cap = if setup.sub_skill == SkillType::HandToHand { 0.99 } else { 0.95 };
                ctx.record_calculation("Offhand hit probabilities",
                    "p_first = clamp((75 + 0.5 × ({accuracy} + 100 − {evasion}))/100, 0.2, {cap}) = {first}; p_other = clamp((75 + 0.5 × ({accuracy} − {evasion}))/100, 0.2, {cap}) = {other}",
                    &[("accuracy", accuracy), ("evasion", setup.enemy_evasion), ("cap", cap),
                        ("first", melee.hit21), ("other", melee.hit22)]);
            }
            if setup.main_skill == SkillType::HandToHand {
                ctx.record_equation("Hand-to-hand first-hit TP delay",
                    "Physical H2H TP delay per fist = Total modified H2H delay after Martial Arts / 2.\nBoth the first main fist and first offhand/second fist receive full TP at this delay when landed; additional WS hits use 10 base TP instead.",
                    "Physical first-hit TP delay = {melee_delay}/2 = {tp_delay}",
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
            ctx.record_equation("Ranged weapon, ammo and fSTR2",
                "Unmultiplied ranged WS base = Weapon Base Damage + Ammo Damage + fSTR2 + WSC.\nfSTR2 uses the ranged weapon's damage/rank, not weapon plus ammo damage; the evaluator retains fractional fSTR2 and WSC.",
                "Weapon Base Damage + Ammo Damage = {ranged_dmg} + {ammo_dmg} = {weapon_dmg}; fSTR2 = ranged_fSTR2(Weapon Base Damage = {ranged_dmg}, STR = {str}, VIT = {vit}) = {fstr}; Unmultiplied ranged WS base = {weapon_dmg} + {fstr} + {wsc} = {base}",
                &[("ranged_dmg", player.stats.get(Stat::RangedDmg)),
                    ("ammo_dmg", player.stats.get(Stat::AmmoDmg)), ("weapon_dmg", ranged.weapon_dmg),
                    ("str", player.stats.get(Stat::Str)), ("vit", enemy.stats.get(Stat::Vit)),
                    ("fstr", ranged.fstr), ("wsc", setup.ws.wsc),
                    ("base", ranged.weapon_dmg + ranged.fstr + setup.ws.wsc)]);
            let accuracy = setup.ws.player_rangedaccuracy + player.stats.get(Stat::WeaponSkillAccuracy);
            let cap = if player.abilities.enabled(Ability::Sharpshot) { 0.99 } else { 0.95 };
            ctx.record_calculation("Ranged hit probabilities",
                "p_first = clamp((75 + 0.5 × ({accuracy} + 100 − {evasion}))/100, 0.2, {cap}) = {first}; p_other = clamp((75 + 0.5 × ({accuracy} − {evasion}))/100, 0.2, {cap}) = {other}",
                &[("accuracy", accuracy), ("evasion", setup.enemy_evasion), ("cap", cap),
                    ("first", ranged.hit1), ("other", ranged.hit2)]);
            ctx.record_calculation("Ranged critical probability",
                "p_critical = min(1, {ws_crit_rate} + {ranged_crit}/100) = {crit_rate}",
                &[("ws_crit_rate", setup.crit_rate), ("ranged_crit", player.stats.get(Stat::RangedCritRate)),
                    ("crit_rate", ranged.crit_rate)]);
            ctx.record_equation("Ranged first-hit TP delay",
                "Ranged WS full-return TP delay = Ranged Weapon Delay + Ammo Delay.\nThis delay determines the first ranged hit's full TP after Store TP; later landed ranged WS hits instead use 10 base TP. Haste is not applied.",
                "Ranged first-hit TP delay = {ranged_delay} + {ammo_delay} = {tp_delay}",
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
        ctx.record_equation(if simulation { "Combined sampled WS damage" } else {
            "Combined expected WS damage"
        }, "WS Damage = Physical WS Damage + Magical WS Damage",
            "WS Damage = physical {physical} + magical {magical} = {total}",
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
                ctx.record_calculation("Magic accuracy metric",
                    "metric = applicable first-hit probability {hit_rate} × magic accuracy {magic_accuracy} = {metric}",
                    &[("hit_rate", hit_rate), ("magic_accuracy", magic_accuracy),
                        ("metric", weighted_accuracy)]);
            }
            weighted_accuracy
        }
        _ => total,
    };
    if simulation && player.abilities.enabled(Ability::DamageLimitDps) && total > 99999.0 {
        if ctx.calculation_trace.is_some() {
            ctx.record_calculation("Sampled WS damage limit",
                "reported damage = min({uncapped_damage}, 99999) = 99999; the selected metric was evaluated before this limit",
                &[("uncapped_damage", total)]);
        }
        total = 99999.0;
    }
    if ctx.calculation_trace.is_some() {
        ctx.record_calculation("WS result",
            "reported damage = {damage}; TP return = {tp}; selected metric = {metric}",
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
