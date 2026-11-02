use crate::model::{AttackProc, CalcError, CalculationSection, EvaluationContext, GearPiece, RandomSource, SkillType};
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

const TP_GAIN: &str = "TP = Hits × trunc((trunc(DelayTP(Delay)) + Zanshin bonus) × (1 + Store TP))";

/// Record the inputs and result of an already-completed `math::get_tp` call.
/// `store_tp` is the exact kernel input: usually a fraction, but raw in Quick Draw.
pub(crate) fn record_tp_gain<R: RandomSource>(ctx: &mut EvaluationContext<R>,
    label: &'static str, hits: f64, delay: f64, store_tp: f64, zanshin: bool, tp: f64) {
    if ctx.calculation_trace.is_none() { return; }
    let lines: &'static [(&'static str, &'static str)] = if delay <= 180.0 {
        &[(TP_GAIN, "{hits} × trunc((trunc(61 + ({delay} − 180) × 63 / 360) + {bonus}) × (1 + {store_tp})) = {tp}")]
    } else if delay <= 540.0 {
        &[(TP_GAIN, "{hits} × trunc((trunc(61 + ({delay} − 180) × 88 / 360) + {bonus}) × (1 + {store_tp})) = {tp}")]
    } else if delay <= 630.0 {
        &[(TP_GAIN, "{hits} × trunc((trunc(149 + ({delay} − 540) × 20 / 360) + {bonus}) × (1 + {store_tp})) = {tp}")]
    } else if delay <= 720.0 {
        &[(TP_GAIN, "{hits} × trunc((trunc(154 + ({delay} − 630) × 28 / 360) + {bonus}) × (1 + {store_tp})) = {tp}")]
    } else if delay <= 900.0 {
        &[(TP_GAIN, "{hits} × trunc((trunc(161 + ({delay} − 720) × 24 / 360) + {bonus}) × (1 + {store_tp})) = {tp}")]
    } else {
        &[(TP_GAIN, "{hits} × trunc((trunc(173 + ({delay} − 900) × 28 / 360) + {bonus}) × (1 + {store_tp})) = {tp}")]
    };
    ctx.record_step(CalculationSection::Tp, label, lines, &[("hits", hits), ("delay", delay),
        ("store_tp", store_tp), ("bonus", if zanshin { 150.0 } else { 0.0 }), ("tp", tp)]);
}
