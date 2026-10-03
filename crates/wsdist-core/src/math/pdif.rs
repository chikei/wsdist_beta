use crate::model::{CalcError, RandomSource, RandomStream, SkillType};

use super::divide;

/// Exact-match pDIF classification. Unknown names take the source's final
/// `else` branches, including the distinct literal `"Great sword"` spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdifSkill {
    Known(SkillType),
    Other,
}

impl PdifSkill {
    pub fn from_name(name: &str) -> Self {
        SkillType::from_name(name).map_or(Self::Other, Self::Known)
    }

    #[inline]
    const fn melee_cap(self) -> f64 {
        match self {
            Self::Known(SkillType::Katana | SkillType::Dagger | SkillType::Sword | SkillType::Axe | SkillType::Club) => 3.25,
            Self::Known(SkillType::GreatKatana | SkillType::HandToHand) => 3.5,
            Self::Known(SkillType::GreatSword | SkillType::Staff | SkillType::GreatAxe | SkillType::Polearm) => 3.75,
            _ => 4.0,
        }
    }

    #[inline]
    const fn ranged_cap(self) -> f64 {
        if matches!(self, Self::Known(SkillType::Marksmanship)) { 3.5 } else { 3.25 }
    }
}

impl From<SkillType> for PdifSkill {
    fn from(skill: SkillType) -> Self { Self::Known(skill) }
}

#[inline]
fn melee_limits(wratio: f64) -> (f64, f64) {
    let upper = if wratio >= 0.0 && wratio < 0.5 { wratio + 0.5 }
        else if wratio >= 0.5 && wratio < 0.7 { 1.0 }
        else if wratio >= 0.7 && wratio < 1.2 { wratio + 0.3 }
        else if wratio >= 1.2 && wratio < 1.5 { 1.25 * wratio }
        else { wratio + 0.375 };
    let lower = if wratio >= 0.0 && wratio < 0.38 { 0.0 }
        else if wratio >= 0.38 && wratio < 1.25 { (1176.0 / 1024.0) * wratio - (448.0 / 1024.0) }
        else if wratio >= 1.25 && wratio < 1.51 { 1.0 }
        else if wratio >= 1.51 && wratio < 2.44 { (1176.0 / 1024.0) * wratio - (755.0 / 1024.0) }
        else { wratio - 0.375 };
    (lower, upper)
}

#[inline]
fn ranged_limits(wratio: f64) -> (f64, f64) {
    let upper = if wratio >= 0.0 && wratio < 0.9 { wratio * (10.0 / 9.0) }
        else if wratio >= 0.9 && wratio < 1.1 { 1.0 }
        else { wratio };
    let lower = if wratio >= 0.0 && wratio < 0.9 { wratio }
        else if wratio >= 0.9 && wratio < 1.1 { 1.0 }
        else { wratio * (20.0 / 19.0) - (3.0 / 19.0) };
    (lower, upper)
}

#[inline]
fn clipped(qratio: f64, cap: f64) -> f64 {
    if qratio <= 0.0 { 0.0 } else if qratio >= cap { cap } else { qratio }
}

/// get_pdif.py:10–78. Always draw crit first and the final multiplier even
/// when damage clips to zero; errors after crit still consume that first draw.
#[inline]
pub fn get_pdif_melee(player_attack: f64, wpn_type_skill: PdifSkill, pdl_trait: f64, pdl_gear: f64, enemy_defense: f64, crit_rate: f64, random: &mut impl RandomSource) -> Result<(f64, bool), CalcError> {
    let pdif_cap = (wpn_type_skill.melee_cap() + pdl_trait) * (1.0 + pdl_gear);
    let crit = random.uniform(RandomStream::NumbaPython, 0.0, 1.0) < crit_rate;
    let ratio = divide(player_attack, enemy_defense, "get_pdif_melee", "enemy_defense")?;
    let wratio = if crit { ratio + 1.0 } else { ratio };
    let (lower, upper) = melee_limits(wratio);
    let qratio = random.uniform(RandomStream::NumbaPython, lower, upper);
    let mut pdif = clipped(qratio, pdif_cap);
    if crit { pdif += 1.0; }
    pdif *= random.uniform(RandomStream::NumbaPython, 1.0, 1.05);
    Ok((pdif, crit))
}

/// get_pdif.py:82–149. Preserve the correlated fractional-crit midpoint
/// approximation rather than replacing it with an integrated sampled mean.
#[inline]
pub fn get_avg_pdif_melee(player_attack: f64, wpn_type_skill: PdifSkill, pdl_trait: f64, pdl_gear: f64, enemy_defense: f64, crit_rate: f64) -> Result<f64, CalcError> {
    let pdif_cap = (wpn_type_skill.melee_cap() + pdl_trait) * (1.0 + pdl_gear);
    let ratio = divide(player_attack, enemy_defense, "get_avg_pdif_melee", "enemy_defense")?;
    let crit_rate = if crit_rate > 1.0 { 1.0 } else { crit_rate };
    let wratio = ratio + 1.0 * crit_rate;
    let (lower, upper) = melee_limits(wratio);
    let qratio = 0.5 * (upper + lower);
    let mut pdif = clipped(qratio, pdif_cap);
    pdif += 1.0 * crit_rate;
    pdif *= 1.025;
    Ok(pdif)
}

/// get_pdif.py:154–195. Ranged crit does not change qRatio and has no third
/// multiplier draw. Non-Marksmanship, including unknown strings, caps at 3.25.
#[inline]
pub fn get_pdif_ranged(player_ranged_attack: f64, wpn_type_skill: PdifSkill, pdl_trait: f64, pdl_gear: f64, enemy_defense: f64, crit_rate: f64, random: &mut impl RandomSource) -> Result<(f64, bool), CalcError> {
    let pdif_base_cap = wpn_type_skill.ranged_cap();
    let crit = random.uniform(RandomStream::NumbaPython, 0.0, 1.0) < crit_rate;
    let ratio = divide(player_ranged_attack, enemy_defense, "get_pdif_ranged", "enemy_defense")?;
    let (lower, upper) = ranged_limits(ratio);
    let qratio = random.uniform(RandomStream::NumbaPython, lower, upper);
    let pdif_cap = (pdif_base_cap + pdl_trait) * (1.0 + pdl_gear);
    let mut pdif = clipped(qratio, pdif_cap);
    if crit { pdif *= 1.25; }
    Ok((pdif, crit))
}

/// get_pdif.py:198–237. The analytic crit-rate cap occurs after clipping.
#[inline]
pub fn get_avg_pdif_ranged(player_ranged_attack: f64, wpn_type_skill: PdifSkill, pdl_trait: f64, pdl_gear: f64, enemy_defense: f64, crit_rate: f64) -> Result<f64, CalcError> {
    let pdif_base_cap = wpn_type_skill.ranged_cap();
    let ratio = divide(player_ranged_attack, enemy_defense, "get_avg_pdif_ranged", "enemy_defense")?;
    let (lower, upper) = ranged_limits(ratio);
    let qratio = 0.5 * (lower + upper);
    let pdif_cap = (pdif_base_cap + pdl_trait) * (1.0 + pdl_gear);
    let mut pdif = clipped(qratio, pdif_cap);
    let crit_rate = if crit_rate > 1.0 { 1.0 } else { crit_rate };
    pdif *= 1.0 + 0.25 * crit_rate;
    Ok(pdif)
}
