//! Literal numerical kernels from the frozen legacy implementation.
//!
//! Percentages remain in each source function's units. Analytical functions are
//! intentionally not expectations of the sampled functions.

mod magic;
mod multiattack;
mod pdif;
mod rounding;
mod weapon_bonus;

pub use magic::*;
pub use multiattack::*;
pub use pdif::*;
pub use rounding::{numpy_round1, python_round1};
pub use weapon_bonus::*;

use crate::model::CalcError;

/// Truncate towards zero without Rust's saturating float-to-integer cast.
///
/// Python integers are unbounded: every finite input can be truncated directly
/// in binary64. There is no bounded Rust integer conversion, allocation, or
/// invented signed-64-bit clamp; a zero integer stage loses the input's sign.
#[inline]
pub fn python_int(value: f64, operation: &'static str, field: &'static str) -> Result<f64, CalcError> {
    if value.is_nan() {
        return Err(CalcError::legacy(operation, field, "ValueError", "cannot convert float NaN to integer"));
    }
    if value.is_infinite() {
        return Err(CalcError::legacy(operation, field, "OverflowError", "cannot convert float infinity to integer"));
    }
    let integer = value.trunc();
    Ok(if integer == 0.0 { 0.0 } else { integer })
}

// Python's two-argument min/max retain the first argument on unordered/equal
// comparisons. f64::min/max have different NaN and signed-zero semantics.
#[inline]
pub(crate) fn python_min(first: f64, second: f64) -> f64 {
    if second < first { second } else { first }
}

#[inline]
pub(crate) fn python_max(first: f64, second: f64) -> f64 {
    if second > first { second } else { first }
}

#[inline]
pub(crate) fn divide(numerator: f64, denominator: f64, operation: &'static str, field: &'static str) -> Result<f64, CalcError> {
    if denominator == 0.0 {
        Err(CalcError::legacy(operation, field, "ZeroDivisionError", "division by zero"))
    } else {
        Ok(numerator / denominator)
    }
}

/// get_tp.py:6–28. Both int stages occur before multiplication by swings.
#[inline]
pub fn get_tp(swings: f64, mdelay: f64, stp: f64, zanshin: bool) -> Result<f64, CalcError> {
    let base = if mdelay <= 180.0 {
        61.0 + (mdelay - 180.0) * 63.0 / 360.0
    } else if mdelay <= 540.0 {
        61.0 + (mdelay - 180.0) * 88.0 / 360.0
    } else if mdelay <= 630.0 {
        149.0 + (mdelay - 540.0) * 20.0 / 360.0
    } else if mdelay <= 720.0 {
        154.0 + (mdelay - 630.0) * 28.0 / 360.0
    } else if mdelay <= 900.0 {
        161.0 + (mdelay - 720.0) * 24.0 / 360.0
    } else {
        173.0 + (mdelay - 900.0) * 28.0 / 360.0
    };
    let base_tp = python_int(base, "get_tp", "base_tp")? + if zanshin { 150.0 } else { 0.0 };
    Ok(swings * python_int(base_tp * (1.0 + stp), "get_tp", "store_tp")?)
}

/// get_hit_rate.py:9–23. Preserve the nested cap-before-floor comparison.
#[inline]
pub fn get_hit_rate(player_accuracy: f64, enemy_evasion: f64, hit_rate_cap: f64) -> f64 {
    let hit_rate = (75.0 + 0.5 * (player_accuracy - enemy_evasion)) / 100.0;
    if hit_rate > hit_rate_cap { hit_rate_cap } else if hit_rate < 0.2 { 0.2 } else { hit_rate }
}

#[inline]
fn fstr_numerator(dstr: f64) -> f64 {
    if dstr <= -22.0 { dstr + 13.0 }
    else if dstr <= -16.0 { dstr + 12.0 }
    else if dstr <= -8.0 { dstr + 10.0 }
    else if dstr <= -3.0 { dstr + 9.0 }
    else if dstr <= 0.0 { dstr + 8.0 }
    else if dstr <= 5.0 { dstr + 7.0 }
    else if dstr < 12.0 { dstr + 6.0 }
    else { dstr + 4.0 }
}

/// get_fstr.py:9–39. Rank and fSTR are fractional, not floored.
#[inline]
pub fn get_fstr(dmg: f64, player_str: f64, enemy_vit: f64) -> f64 {
    let fstr = fstr_numerator(player_str - enemy_vit) / 4.0;
    if fstr < -1.0 * dmg / 9.0 { -1.0 * dmg / 9.0 }
    else if fstr > 8.0 + dmg / 9.0 { 8.0 + dmg / 9.0 }
    else { fstr }
}

/// get_fstr.py:42–78. Apply dSTR limits before the distinct ranged limits.
#[inline]
pub fn get_fstr2(rng_dmg: f64, player_str: f64, enemy_vit: f64) -> f64 {
    let wpn_rank = rng_dmg / 9.0;
    let mut dstr = player_str - enemy_vit;
    let dstr_lower = -(7.0 + 2.0 * wpn_rank) * 2.0;
    let dstr_upper = (14.0 + 2.0 * wpn_rank) * 2.0;
    if dstr < dstr_lower { dstr = dstr_lower; }
    if dstr > dstr_upper { dstr = dstr_upper; }
    let mut fstr = fstr_numerator(dstr) / 2.0;
    if fstr < -2.0 * wpn_rank { fstr = -2.0 * wpn_rank; }
    if fstr > 2.0 * (wpn_rank + 8.0) { fstr = 2.0 * (wpn_rank + 8.0); }
    fstr
}

/// get_dex_crit.py:9–30, including the discontinuity above dDEX 39.
#[inline]
pub fn get_dex_crit(player_dex: f64, enemy_agi: f64) -> f64 {
    let ddex = player_dex - enemy_agi;
    if ddex <= 6.0 { 0.0 }
    else if ddex <= 13.0 { 0.01 }
    else if ddex <= 19.0 { 0.02 }
    else if ddex <= 29.0 { 0.03 }
    else if ddex <= 39.0 { 0.04 }
    else {
        let check = (ddex - 35.0) / 100.0;
        if check <= 0.15 { check } else { 0.15 }
    }
}

/// get_delay_timing.py:9–30. Inputs are fractions, not sheet percentages.
#[inline]
pub fn get_delay_timing(delay1: f64, delay2: f64, dw: f64, marts: f64, magic_haste: f64, ja_haste: f64, gear_haste: f64) -> f64 {
    let gear_haste = if gear_haste > 256.0 / 1024.0 { 256.0 / 1024.0 } else { gear_haste };
    let magic_haste = if magic_haste > 448.0 / 1024.0 { 448.0 / 1024.0 } else { magic_haste };
    let ja_haste = if ja_haste > 256.0 / 1024.0 { 256.0 / 1024.0 } else { ja_haste };
    let total_haste = gear_haste + magic_haste + ja_haste;
    let delay = delay1 + delay2;
    let rdelay = (delay - marts) * (1.0 - dw) * (1.0 - total_haste);
    let rdelay = if rdelay < 0.2 * delay { 0.2 * delay } else { rdelay };
    rdelay / 60.0
}

/// get_phys_damage.py:9–20. Only the first main-hand hit receives WSD and
/// flourish/sneak/trick additions. Truncate once, before pDIF and crit damage.
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn get_phys_damage(wpn_dmg: f64, fstr_wpn: f64, wsc: f64, pdif: f64, ftp: f64, crit: bool, crit_dmg: f64, wsd: f64, ws_bonus: f64, ws_trait: f64, n: f64, sneak_attack_bonus: f64, trick_attack_bonus: f64, climactic_flourish_bonus: f64, striking_flourish_bonus: f64, ternary_flourish_bonus: f64) -> Result<f64, CalcError> {
    let first = if n == 0.0 { 1.0 } else { 0.0 };
    let base = (wpn_dmg + fstr_wpn + wsc) * ftp * (1.0 + wsd * first) * (1.0 + ws_bonus) * (1.0 + ws_trait)
        + sneak_attack_bonus * first + trick_attack_bonus * first + climactic_flourish_bonus * first
        + striking_flourish_bonus * first + ternary_flourish_bonus * first;
    Ok(python_int(base, "get_phys_damage", "base_damage")? * pdif
        * (1.0 + (if crit { 1.0 } else { 0.0 }) * python_min(crit_dmg, 1.0)))
}

/// get_phys_damage.py:23–29. The analytical path keeps its separate caps.
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn get_avg_phys_damage(wpn_dmg: f64, fstr_wpn: f64, wsc: f64, pdif: f64, ftp: f64, crit_rate: f64, crit_dmg: f64, wsd: f64, ws_bonus: f64, ws_trait: f64, sneak_attack_bonus: f64, trick_attack_bonus: f64, climactic_flourish_bonus: f64, striking_flourish_bonus: f64, ternary_flourish_bonus: f64) -> Result<f64, CalcError> {
    let base = (wpn_dmg + fstr_wpn + wsc) * ftp * (1.0 + wsd) * (1.0 + ws_bonus) * (1.0 + ws_trait)
        + sneak_attack_bonus + trick_attack_bonus + climactic_flourish_bonus
        + striking_flourish_bonus + ternary_flourish_bonus;
    Ok(python_int(base, "get_avg_phys_damage", "base_damage")? * pdif
        * (1.0 + python_min(crit_rate, 1.0) * python_min(crit_dmg, 1.0)))
}
