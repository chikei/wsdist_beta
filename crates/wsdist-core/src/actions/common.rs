use crate::model::{AttackProc, CalcError, EvaluationContext, GearPiece, RandomSource, SkillType};
use crate::stats::{Stat, Stats};

pub(crate) const ONE_HANDED_SKILLS: &[SkillType] = &[SkillType::Axe, SkillType::Club, SkillType::Dagger, SkillType::Sword, SkillType::Katana];
pub(crate) const TWO_HANDED_SKILLS: &[SkillType] = &[SkillType::GreatSword, SkillType::GreatKatana, SkillType::GreatAxe, SkillType::Polearm, SkillType::Scythe, SkillType::Staff];
pub(crate) const EMPYREAN_AM: [f64; 3] = [0.3, 0.4, 0.5];
pub(crate) const EMPYREAN_WEAPONS: &[&str] = &["Verethragna", "Twashtar", "Almace", "Caladbolg", "Farsha", "Ukonvasara", "Redemption", "Kannagi", "Rhongomiant", "Gambanteinn", "Masamune", "Hvergelmir"];
pub(crate) const RELIC_WEAPONS_30: &[&str] = &["Spharai", "Mandau", "Excalibur", "Kikoku", "Mjollnir"];
pub(crate) const RELIC_WEAPONS_25: &[&str] = &["Ragnarok", "Guttler", "Gungnir", "Amanomurakumo", "Claustrum"];
pub(crate) const RELIC_WEAPONS_20: &[&str] = &["Bravura", "Apocalypse"];
pub(crate) const PRIME_WEAPONS_3: &[&str] = &["Varga Purnikawa V", "Mpu Gandring V", "Caliburnus V", "Helheim V", "Spalirisos V", "Laphria V", "Foenaria V", "Gae Buide V", "Dokoku V", "Kusanagi no Tsurugi V", "Lorg Mor V", "Opashoro V"];
pub(crate) const PRIME_WEAPONS_2: &[&str] = &["Varga Purnikawa IV", "Mpu Gandring IV", "Caliburnus IV", "Helheim IV", "Spalirisos IV", "Laphria IV", "Foenaria IV", "Gae Buide IV", "Dokoku IV", "Kusanagi no Tsurugi IV", "Lorg Mor IV", "Opashoro IV"];

pub(crate) fn oa_list(stats: &Stats) -> [f64; 9] {
    [Stat::Oa3Main, Stat::Oa2Main, Stat::Oa8Sub, Stat::Oa7Sub, Stat::Oa6Sub, Stat::Oa5Sub, Stat::Oa4Sub, Stat::Oa3Sub, Stat::Oa2Sub].map(|stat| stats.get(stat) / 100.0)
}

pub(crate) fn oa_tiers(stats: &Stats, offhand: bool) -> [(AttackProc, f64, u8); 7] {
    let stat = if offhand {
        [Stat::Oa8Sub, Stat::Oa7Sub, Stat::Oa6Sub, Stat::Oa5Sub, Stat::Oa4Sub, Stat::Oa3Sub, Stat::Oa2Sub]
    } else {
        [Stat::Oa8Main, Stat::Oa7Main, Stat::Oa6Main, Stat::Oa5Main, Stat::Oa4Main, Stat::Oa3Main, Stat::Oa2Main]
    };
    std::array::from_fn(|index| {
        let attacks = 8 - index as u8;
        (AttackProc::Occasionally(attacks), stats.get(stat[index]) / 100.0, attacks - 1)
    })
}

// actions.py:81–116. Caller retains the source's preceding int(Aftermath).
pub(crate) fn weapon_damage_bonuses(main: &GearPiece, aftermath: f64) -> Result<[f64; 4], CalcError> {
    let mut empyrean = 1.0;
    if EMPYREAN_WEAPONS.contains(&main.name.as_ref()) && aftermath > 0.0 {
        let rate = if aftermath == 1.0 { EMPYREAN_AM[0] }
            else if aftermath == 2.0 { EMPYREAN_AM[1] }
            else if aftermath == 3.0 { EMPYREAN_AM[2] }
            else {
                let message = if aftermath > 9223372036854775808.0 {
                    "cannot fit 'int' into an index-sized integer"
                } else { "list index out of range" };
                return Err(CalcError::legacy("get_weapon_damage_bonuses", "Aftermath", "IndexError", message));
            };
        empyrean += 2.0 * rate;
    }
    let mut relic = 1.0;
    if RELIC_WEAPONS_30.contains(&main.name.as_ref()) { relic += 2.0 * 0.13; }
    else if RELIC_WEAPONS_25.contains(&main.name.as_ref()) { relic += 1.5 * 0.16; }
    else if RELIC_WEAPONS_20.contains(&main.name.as_ref()) { relic += 1.0 * 0.2; }
    let mut prime = 1.0;
    if PRIME_WEAPONS_3.contains(&main.name2.as_ref()) { prime += 2.0 * 0.3; }
    else if PRIME_WEAPONS_2.contains(&main.name2.as_ref()) { prime += 1.0 * 0.3; }
    let mut kick = 1.0;
    if main.name2.as_ref() == "Dragon Fangs" { kick += 1.0 * 0.2; }
    Ok([empyrean, relic, prime, kick])
}

/// TP notation shared by all action reports; it does not evaluate TP again.
pub(crate) fn record_tp_reference<R: RandomSource>(ctx: &mut EvaluationContext<R>) {
    if ctx.calculation_trace.is_none() { return; }
    ctx.record_equation("Tactical Points reference",
        "BG Wiki Tactical Points:\nBase TP = floor(Delay TP(Modified Delay))\nTP per hit = floor(Base TP) + floor(Base TP × Store TP / 100)\nNormal multiattacks grant full TP per landed hit; weapon skills grant full TP when the first main-hand or first offhand attempt lands, then 10 base TP per additional landed hit.",
        "Base TP denotes the initially rounded delay-derived value. The delay-based TP kernel truncates toward zero rather than flooring and applies Store TP before weighting expected landed hits. The wiki describes TP from hits dealing more than zero damage; this evaluator uses landed-hit counts without a separate damage-positive gate. Haste changes attack timing, not TP-generating delay. Action-specific delay, Store TP units, extra TP sources and exceptions are shown below.",
        &[]);
    ctx.record_equation("Delay-to-TP table",
        "Implemented continuous-delay form of the BG Wiki table, with d = Modified Delay:\nDelay TP(d) = 61 + (d − 180) × 63 / 360, d ≤ 180\nDelay TP(d) = 61 + (d − 180) × 88 / 360, 180 < d ≤ 540\nDelay TP(d) = 149 + (d − 540) × 20 / 360, 540 < d ≤ 630\nDelay TP(d) = 154 + (d − 630) × 28 / 360, 630 < d ≤ 720\nDelay TP(d) = 161 + (d − 720) × 24 / 360, 720 < d ≤ 900\nDelay TP(d) = 173 + (d − 900) × 28 / 360, d > 900",
        "The wiki labels its integer ranges 181–540, 541–630, 631–720, 721–900 and >901. The evaluator retains fractional delay and uses the continuous boundaries above, including its final branch for every delay >900. No table boundary or arithmetic is changed for this display.",
        &[]);
}

/// Record the inputs and result of an already-completed `math::get_tp` call.
/// `store_tp` is the exact kernel input: usually a fraction, but raw in Quick Draw.
pub(crate) fn record_tp_gain<R: RandomSource>(ctx: &mut EvaluationContext<R>,
    label: &'static str, hits: f64, delay: f64, store_tp: f64, zanshin: bool, tp: f64) {
    if ctx.calculation_trace.is_none() { return; }
    let formula = if delay <= 180.0 {
        "TP = {hits} × trunc((trunc(61 + ({delay} − 180) × 63 / 360) + {bonus}) × (1 + {store_tp})) = {tp}"
    } else if delay <= 540.0 {
        "TP = {hits} × trunc((trunc(61 + ({delay} − 180) × 88 / 360) + {bonus}) × (1 + {store_tp})) = {tp}"
    } else if delay <= 630.0 {
        "TP = {hits} × trunc((trunc(149 + ({delay} − 540) × 20 / 360) + {bonus}) × (1 + {store_tp})) = {tp}"
    } else if delay <= 720.0 {
        "TP = {hits} × trunc((trunc(154 + ({delay} − 630) × 28 / 360) + {bonus}) × (1 + {store_tp})) = {tp}"
    } else if delay <= 900.0 {
        "TP = {hits} × trunc((trunc(161 + ({delay} − 720) × 24 / 360) + {bonus}) × (1 + {store_tp})) = {tp}"
    } else {
        "TP = {hits} × trunc((trunc(173 + ({delay} − 900) × 28 / 360) + {bonus}) × (1 + {store_tp})) = {tp}"
    };
    ctx.record_equation(label,
        "Implemented TP Gain = Landed Hits × trunc((trunc(Delay TP(Modified Delay)) + Zanshin TP Bonus) × (1 + Store TP Input))\nStore TP Input normally equals Store TP / 100; any branch using raw Store TP is labeled separately. Zanshin TP Bonus is 150 for the modeled SAM Zanshin call, otherwise 0.",
        formula, &[("hits", hits), ("delay", delay), ("store_tp", store_tp),
            ("bonus", if zanshin { 150.0 } else { 0.0 }), ("tp", tp)]);
}
