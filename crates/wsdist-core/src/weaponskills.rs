//! Ordered legacy weapon-skill setup, before analytical or sampled damage.
//!
//! The registry is immutable. Names are resolved once at the input boundary;
//! evaluation uses an ID, named hook enums and fixed numeric fields only.

use serde::Serialize;

use crate::math::get_dex_crit;
use crate::model::{Ability, CalcError, Enemy, EvaluationContext, Player, RandomSource, SkillType, Slot, WscContribution};
use crate::stats::Stat;

mod registry;
pub use registry::{WS_TABLE, WscFormula};

const OPERATION: &str = "weaponskill_info";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WeaponSkillId(pub(crate) u16);

impl WeaponSkillId {
    pub const fn index(self) -> usize { self.0 as usize }
    pub fn name(self) -> &'static str { self.spec().name }
    pub fn spec(self) -> &'static WeaponSkillSpec { &WS_TABLE[self.index()] }
}

pub fn resolve_name(name: &str) -> Result<WeaponSkillId, CalcError> {
    registry::resolve(name).ok_or_else(|| CalcError::legacy(
        OPERATION, "ws_name", "KeyError", python_key_repr(name),
    ))
}

pub(crate) fn python_key_repr(name: &str) -> String {
    let quote = if name.contains('\'') && !name.contains('"') { '"' } else { '\'' };
    let mut result = String::with_capacity(name.len() + 2);
    result.push(quote);
    for ch in name.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch == quote => { result.push('\\'); result.push(ch); }
            ch if ch.is_control() => {
                use std::fmt::Write;
                let code = ch as u32;
                if code <= 0xff { write!(result, "\\x{code:02x}").expect("writing to String"); }
                else { write!(result, "\\u{code:04x}").expect("writing to String"); }
            }
            ch => result.push(ch),
        }
    }
    result.push(quote);
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum WsElement { None, Fire, Ice, Wind, Earth, Thunder, Light, Dark, Water }

impl WsElement {
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None", Self::Fire => "Fire", Self::Ice => "Ice", Self::Wind => "Wind",
            Self::Earth => "Earth", Self::Thunder => "Thunder", Self::Light => "Light",
            Self::Dark => "Dark", Self::Water => "Water",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TpScaling { Fixed(f64), Interpolated([f64; 3]) }

impl TpScaling {
    pub fn at(self, tp: f64) -> f64 {
        match self { Self::Fixed(value) => value, Self::Interpolated(points) => interpolate_tp(tp, points) }
    }
}

/// Literal three-anchor `np.interp` arithmetic, including clamped endpoints.
pub fn interpolate_tp(tp: f64, points: [f64; 3]) -> f64 {
    if tp.is_nan() { return tp; }
    if tp < 1000.0 { return points[0]; }
    if tp > 3000.0 { return points[2]; }
    if tp == 3000.0 { return points[2]; }
    let index = if tp < 2000.0 { 0 } else { 1 };
    let anchor = if index == 0 { 1000.0 } else { 2000.0 };
    if tp == anchor { return points[index]; }
    let slope = (points[index + 1] - points[index]) / 1000.0;
    let mut value = slope * (tp - anchor) + points[index];
    // NumPy retries from the other endpoint for an invalid interpolated result.
    if value.is_nan() {
        value = slope * (tp - (anchor + 1000.0)) + points[index + 1];
        if value.is_nan() && points[index] == points[index + 1] { value = points[index]; }
    }
    value
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CritScaling { None, Fixed(f64), Melee([f64; 3]), Ranged([f64; 3]) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DStatFormula { Zero, IntCapped, IntDouble, AgiDouble, MndDouble, PrimalRend, Wildfire }

impl DStatFormula {
    pub const fn source_name(self) -> Option<&'static str> {
        match self {
            Self::Zero => None, Self::IntCapped => Some("dstat_int_capped"),
            Self::IntDouble => Some("dstat_int_double"), Self::AgiDouble => Some("dstat_agi_double"),
            Self::MndDouble => Some("dstat_mnd_double"), Self::PrimalRend => Some("dstat_primal_rend"),
            Self::Wildfire => Some("dstat_wildfire"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WsHook {
    Accuracy { points: [f64; 3], both_hands: bool },
    RangedAccuracy { points: [f64; 3] },
    Attack { modifier: f64, both_hands: bool },
    AttackUnguarded { modifier: f64 },
    InterpolatedAttack { points: [f64; 3], both_hands: bool },
    RangedAttack { modifier: f64 },
    KickAttack,
    DefenseDown { points: [f64; 3] },
}

impl WsHook {
    pub const fn source_name(self) -> &'static str {
        match self {
            Self::Accuracy { .. } => "acc_hook", Self::RangedAccuracy { .. } => "ranged_acc_hook",
            Self::Attack { .. } => "atk_hook", Self::AttackUnguarded { .. } => "atk_hook_unguarded",
            Self::InterpolatedAttack { .. } => "interp_atk_hook", Self::RangedAttack { .. } => "ranged_atk_hook",
            Self::KickAttack => "kick_atk_hook", Self::DefenseDown { .. } => "def_down_hook",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponSkillSpec {
    pub name: &'static str,
    pub skill_type: SkillType,
    pub source_line: u16,
    pub nhits: u8,
    pub wsc: WscFormula,
    pub ftp: TpScaling,
    pub ftp_rep: bool,
    pub magical: bool,
    pub hybrid: bool,
    pub element: WsElement,
    pub dstat: DStatFormula,
    pub ftp_hybrid: Option<[f64; 3]>,
    pub crit: CritScaling,
    pub hooks: &'static [WsHook],
}

impl WeaponSkillSpec {
    const fn new(name: &'static str, skill_type: SkillType, source_line: u16, nhits: u8, wsc: WscFormula, ftp: TpScaling) -> Self {
        Self { name, skill_type, source_line, nhits, wsc, ftp, ftp_rep: false, magical: false,
            hybrid: false, element: WsElement::None, dstat: DStatFormula::Zero,
            ftp_hybrid: None, crit: CritScaling::None, hooks: &[] }
    }
    const fn replicated(mut self) -> Self { self.ftp_rep = true; self }
    const fn magical(mut self, element: WsElement, dstat: DStatFormula) -> Self {
        self.magical = true; self.element = element; self.dstat = dstat; self
    }
    const fn hybrid(mut self, element: WsElement, points: [f64; 3]) -> Self {
        self.hybrid = true; self.element = element; self.ftp_hybrid = Some(points); self
    }
    const fn critical(mut self, crit: CritScaling) -> Self { self.crit = crit; self }
    const fn hooks(mut self, hooks: &'static [WsHook]) -> Self { self.hooks = hooks; self }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct WsScaling {
    pub hybrid: bool,
    pub magical: bool,
    #[serde(rename = "dSTAT", serialize_with = "crate::model::serialize_legacy_f64")]
    pub dstat: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub wsc: f64,
    pub nhits: u8,
    pub element: WsElement,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub ftp: f64,
    pub ftp_rep: bool,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub player_attack1: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub player_attack2: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub player_accuracy1: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub player_accuracy2: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub player_rangedattack: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub player_rangedaccuracy: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub enemy_def: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub crit_rate: f64,
    #[serde(serialize_with = "crate::model::serialize_legacy_f64")]
    pub ftp_hybrid: f64,
}

struct WsContext<'a> {
    tp: f64,
    player: &'a Player,
    enemy_basedef: f64,
    #[expect(dead_code, reason = "Legacy context requires VIT before table lookup although no current hook uses it")]
    enemy_vit: f64,
    enemy_agi: f64,
    enemy_int: f64,
    enemy_mnd: f64,
    #[expect(dead_code, reason = "Preserve WSContext.enemy_chr's MND source rather than correcting it to CHR")]
    enemy_chr: f64,
    crit_ws: bool,
    scaling: WsScaling,
}

impl<'a> WsContext<'a> {
    fn new(tp: f64, player: &'a Player, enemy: &Enemy) -> Result<Self, CalcError> {
        // Required enemy reads retain WSContext.__init__'s exact missing-key order.
        let enemy_basedef = enemy.stats.require(Stat::BaseDefense, OPERATION)?;
        let enemy_def = enemy.stats.require(Stat::Defense, OPERATION)?;
        let enemy_vit = enemy.stats.require(Stat::Vit, OPERATION)?;
        let enemy_agi = enemy.stats.require(Stat::Agi, OPERATION)?;
        let enemy_int = enemy.stats.require(Stat::Int, OPERATION)?;
        let enemy_mnd = enemy.stats.require(Stat::Mnd, OPERATION)?;
        let enemy_chr = enemy.stats.require(Stat::Mnd, OPERATION)?;
        Ok(Self { tp, player, enemy_basedef, enemy_vit, enemy_agi, enemy_int, enemy_mnd,
            enemy_chr, crit_ws: false, scaling: WsScaling {
                hybrid: false, magical: false, dstat: 0.0, wsc: 0.0, nhits: 0, element: WsElement::None,
                ftp: 0.0, ftp_rep: false, player_attack1: player.stats.get(Stat::Attack1),
                player_attack2: player.stats.get(Stat::Attack2), player_accuracy1: player.stats.get(Stat::Accuracy1),
                player_accuracy2: player.stats.get(Stat::Accuracy2), player_rangedattack: player.stats.get(Stat::RangedAttack),
                player_rangedaccuracy: player.stats.get(Stat::RangedAccuracy), enemy_def, crit_rate: 0.0, ftp_hybrid: 0.0,
            } })
    }

    fn apply_spec(&mut self, spec: &WeaponSkillSpec) -> Result<(), CalcError> {
        self.scaling.nhits = spec.nhits;
        self.scaling.wsc = spec.wsc.evaluate(self.player);
        self.scaling.ftp_rep = spec.ftp_rep;
        self.scaling.ftp = spec.ftp.at(self.tp);
        if spec.magical {
            self.scaling.magical = true;
            self.scaling.element = spec.element;
            self.scaling.dstat = match spec.dstat {
                DStatFormula::Zero => 0.0,
                DStatFormula::IntCapped => {
                    let value = (self.player.stats.get(Stat::Int) - self.enemy_int) / 2.0 + 8.0;
                    if value > 32.0 { 32.0 } else { value }
                }
                DStatFormula::IntDouble => (self.player.stats.get(Stat::Int) - self.enemy_int) * 2.0,
                DStatFormula::AgiDouble => (self.player.stats.get(Stat::Agi) - self.enemy_int) * 2.0,
                DStatFormula::MndDouble => (self.player.stats.get(Stat::Mnd) - self.enemy_mnd) * 2.0,
                DStatFormula::PrimalRend => {
                    let value = (self.player.stats.get(Stat::Chr) - self.enemy_int) * 1.5;
                    if value > 651.0 { 651.0 } else { value }
                }
                DStatFormula::Wildfire => {
                    let value = (self.player.stats.get(Stat::Agi) - self.enemy_int) * 2.0;
                    if value > 1276.0 { 1276.0 } else { value }
                }
            };
        }
        if spec.hybrid {
            self.scaling.hybrid = true;
            self.scaling.element = spec.element;
            self.scaling.ftp_hybrid = interpolate_tp(self.tp, spec.ftp_hybrid.expect("hybrid registry has fTP anchors"));
        }
        match spec.crit {
            CritScaling::None => {}
            CritScaling::Fixed(rate) => { self.crit_ws = true; self.scaling.crit_rate = rate; }
            CritScaling::Melee(points) => {
                self.crit_ws = true;
                self.scaling.crit_rate += self.player.stats.get(Stat::CritRate) / 100.0;
                self.scaling.crit_rate += interpolate_tp(self.tp, points);
                self.scaling.crit_rate += get_dex_crit(self.player.stats.get(Stat::Dex), self.enemy_agi);
            }
            CritScaling::Ranged(points) => {
                self.crit_ws = true;
                self.scaling.crit_rate += self.player.stats.get(Stat::CritRate) / 100.0;
                self.scaling.crit_rate += interpolate_tp(self.tp, points);
                self.scaling.crit_rate += ((self.player.stats.get(Stat::Agi) - self.enemy_agi) / 10.0) / 100.0;
            }
        }
        for hook in spec.hooks { self.apply_hook(*hook)?; }
        Ok(())
    }

    fn apply_hook(&mut self, hook: WsHook) -> Result<(), CalcError> {
        match hook {
            WsHook::Accuracy { points, both_hands } => {
                let bonus = interpolate_tp(self.tp, points);
                self.scaling.player_accuracy1 += bonus;
                if both_hands { self.scaling.player_accuracy2 += bonus; }
            }
            WsHook::RangedAccuracy { points } => self.scaling.player_rangedaccuracy += interpolate_tp(self.tp, points),
            WsHook::Attack { modifier, both_hands } => {
                self.scaling.player_attack1 = scale_attack(self.player, self.scaling.player_attack1, modifier)?;
                if both_hands && self.scaling.player_attack2 > 0.0 {
                    self.scaling.player_attack2 = scale_attack(self.player, self.scaling.player_attack2, modifier)?;
                }
            }
            WsHook::AttackUnguarded { modifier } => {
                self.scaling.player_attack1 = scale_attack(self.player, self.scaling.player_attack1, modifier)?;
                self.scaling.player_attack2 = scale_attack(self.player, self.scaling.player_attack2, modifier)?;
            }
            WsHook::InterpolatedAttack { points, both_hands } => {
                let modifier = interpolate_tp(self.tp, points) - 1.0;
                self.scaling.player_attack1 = scale_attack(self.player, self.scaling.player_attack1, modifier)?;
                if both_hands && self.scaling.player_attack2 > 0.0 {
                    self.scaling.player_attack2 = scale_attack(self.player, self.scaling.player_attack2, modifier)?;
                }
            }
            WsHook::RangedAttack { modifier } => {
                let food = self.player.stats.get(Stat::FoodRangedAttack);
                let attack_pct = self.player.stats.get(Stat::RangedAttackPct);
                self.scaling.player_rangedattack -= food;
                self.scaling.player_rangedattack *= divide(1.0 + attack_pct + modifier, 1.0 + attack_pct, "Ranged Attack%")?;
                self.scaling.player_rangedattack += food;
            }
            WsHook::KickAttack => {
                let modifier = self.player.stats.get(Stat::KickAttacksAttackPct);
                self.scaling.player_attack1 = kick_attack(self.player, self.scaling.player_attack1, modifier)?;
                self.scaling.player_attack2 = kick_attack(self.player, self.scaling.player_attack2, modifier)?;
            }
            WsHook::DefenseDown { points } => self.scaling.enemy_def -= self.enemy_basedef * interpolate_tp(self.tp, points),
        }
        Ok(())
    }
}

fn divide(numerator: f64, denominator: f64, field: &'static str) -> Result<f64, CalcError> {
    if denominator == 0.0 { return Err(CalcError::legacy(OPERATION, field, "ZeroDivisionError", "float division by zero")); }
    Ok(numerator / denominator)
}

fn scale_attack(player: &Player, mut attack: f64, modifier: f64) -> Result<f64, CalcError> {
    let food = player.stats.get(Stat::FoodAttack);
    let attack_pct = player.stats.get(Stat::AttackPct);
    attack -= food;
    attack *= divide(1.0 + attack_pct + modifier, 1.0 + attack_pct, "Attack%")?;
    attack += food;
    Ok(attack)
}

fn kick_attack(player: &Player, mut attack: f64, modifier: f64) -> Result<f64, CalcError> {
    let food = player.stats.get(Stat::FoodAttack);
    let attack_pct = player.stats.get(Stat::AttackPct);
    attack -= food;
    attack = divide(attack, 1.0 + attack_pct, "Attack%")?;
    attack += if player.abilities.enabled(Ability::Footwork) { player.stats.get(Stat::KickAttacksAttack) } else { 0.0 };
    attack *= 1.0 + attack_pct + modifier;
    attack += food;
    Ok(attack)
}

// CPython 3.13.6's built-in sum uses Neumaier compensation for float terms.
// Do not replace it with a left fold: both WSC sums call Python sum in the source.
#[derive(Default)]
struct FloatSum { value: f64, compensation: f64 }

impl FloatSum {
    fn add(&mut self, term: f64) {
        let total = self.value + term;
        if self.value.abs() >= term.abs() { self.compensation += (self.value - total) + term; }
        else { self.compensation += (term - total) + self.value; }
        self.value = total;
    }
    fn finish(self) -> f64 {
        if self.compensation != 0.0 && self.compensation.is_finite() { self.value + self.compensation }
        else { self.value }
    }
}

fn python_sum<const N: usize>(terms: [f64; N]) -> f64 {
    let mut sum = FloatSum::default();
    for term in terms { sum.add(term); }
    sum.finish()
}

/// `weaponskill_info.py` setup order, without damage evaluation or RNG draws.
/// `dual_wield` remains in the boundary because the canonical helper accepts it;
/// its actual setup does not consult that argument.
pub fn weaponskill_info(
    skill: WeaponSkillId,
    tp: f64,
    player: &Player,
    enemy: &Enemy,
    wsc_bonus: &[WscContribution],
    _dual_wield: bool,
    ctx: &EvaluationContext<impl RandomSource>,
) -> Result<WsScaling, CalcError> {
    let mut ws = WsContext::new(tp, player, enemy)?;
    let main_name = ctx.catalog.piece(player.gearset[Slot::Main.index()]).name.as_ref();
    if main_name == "Naegling" {
        ws.scaling.player_attack1 = scale_attack(player, ws.scaling.player_attack1, 0.13)?;
        if ws.scaling.player_attack2 > 0.0 {
            ws.scaling.player_attack2 = scale_attack(player, ws.scaling.player_attack2, 0.13)?;
        }
    } else if main_name == "Nandaka" {
        ws.scaling.enemy_def -= 0.03 * ws.enemy_basedef;
    }
    ws.apply_spec(skill.spec())?;
    // Keep the literal ranged-name exclusion list. Dulling Arrow, Sniper Shot,
    // and the newer ranged skills are deliberately NOT on the legacy list.
    if main_name == "Shining One" && !registry::shining_one_excluded(skill) {
        if !ws.crit_ws {
            ws.scaling.crit_rate += player.stats.get(Stat::CritRate) / 100.0;
            ws.scaling.crit_rate += get_dex_crit(player.stats.get(Stat::Dex), ws.enemy_agi);
        }
        ws.scaling.crit_rate += interpolate_tp(tp, [0.05, 0.10, 0.15]);
    }
    if player.abilities.enabled(Ability::MightyStrikes) { ws.scaling.crit_rate = 1.0; }
    if ws.scaling.enemy_def < 1.0 { ws.scaling.enemy_def = 1.0; }
    let mut bonus = FloatSum::default();
    for contribution in wsc_bonus {
        let stat = player.stats.require(contribution.stat, OPERATION)?;
        bonus.add(stat * contribution.percent / 100.0);
    }
    ws.scaling.wsc += bonus.finish();
    Ok(ws.scaling)
}
