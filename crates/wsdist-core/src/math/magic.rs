use crate::model::{CalcError, RandomSource, RandomStream};

use super::python_int;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NinjutsuTier { Ichi, Ni, San }

impl NinjutsuTier {
    /// The source uses San's windows for every name other than Ichi and Ni.
    pub fn from_name(name: &str) -> Self {
        match name { "Ichi" => Self::Ichi, "Ni" => Self::Ni, _ => Self::San }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlmElement { Earth, Water, Wind, Fire, Ice, Thunder, Light, Dark }

impl BlmElement {
    pub fn from_name(name: &str) -> Result<Self, CalcError> {
        match name {
            "earth" => Ok(Self::Earth), "water" => Ok(Self::Water), "wind" => Ok(Self::Wind),
            "fire" => Ok(Self::Fire), "ice" => Ok(Self::Ice), "thunder" => Ok(Self::Thunder),
            "light" => Ok(Self::Light), "dark" => Ok(Self::Dark),
            _ => Err(source_key_error("element", name)),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Earth => "earth", Self::Water => "water", Self::Wind => "wind",
            Self::Fire => "fire", Self::Ice => "ice", Self::Thunder => "thunder",
            Self::Light => "light", Self::Dark => "dark",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlmTier { I, II, III, IV, V, VI, Ja, Helix }

impl BlmTier {
    pub fn from_name(name: &str) -> Result<Self, CalcError> {
        match name {
            "I" => Ok(Self::I), "II" => Ok(Self::II), "III" => Ok(Self::III),
            "IV" => Ok(Self::IV), "V" => Ok(Self::V), "VI" => Ok(Self::VI),
            "ja" => Ok(Self::Ja), "helix" => Ok(Self::Helix),
            _ => Err(source_key_error("tier", name)),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::I => "I", Self::II => "II", Self::III => "III", Self::IV => "IV",
            Self::V => "V", Self::VI => "VI", Self::Ja => "ja", Self::Helix => "helix",
        }
    }
}

fn source_key_error(field: &'static str, key: &str) -> CalcError {
    CalcError::legacy("get_mv_blm", field, "KeyError", format!("'{key}'"))
}

/// get_dint_m_v.py:13–55. Negative dINT is not clamped.
#[inline]
pub fn get_mv_ninjutsu(tier: NinjutsuTier, dint: f64) -> (f64, f64) {
    match tier {
        NinjutsuTier::Ichi => {
            if dint <= -9.0 { (0.0, 11.0) }
            else if dint <= -1.0 { (0.5, 16.0) }
            else if dint <= 24.0 { (1.0, 16.0) }
            else if dint <= 74.0 { (0.5, 28.5) }
            else { (0.0, 66.0) }
        }
        NinjutsuTier::Ni => {
            if dint <= -43.0 { (0.0, 47.0) }
            else if dint <= -1.0 { (0.5, 69.0) }
            else if dint <= 112.0 { (1.0, 69.0) }
            else if dint <= 338.0 { (0.5, 125.5) }
            else { (0.0, 295.0) }
        }
        NinjutsuTier::San => {
            if dint <= -53.0 { (0.0, 81.0) }
            else if dint <= 1.0 { (1.0, 134.0) }
            else if dint <= 353.0 { (1.5, 134.0) }
            else { (0.0, 655.0) }
        }
    }
}

/// get_dint_m_v.py:57–143. Returns M, V, and the window's original integer
/// value as binary64. Resolve tier before element to preserve source KeyError
/// precedence at string boundaries. Light/dark are only defined for helix.
#[inline]
pub fn get_mv_blm(element: BlmElement, tier: BlmTier, dint: f64) -> Result<(f64, f64, f64), CalcError> {
    let window = if dint < 50.0 { 0 }
        else if dint < 100.0 { 1 }
        else if dint < 200.0 { 2 }
        else if dint < 300.0 { 3 }
        else if dint < 400.0 { 4 }
        else if dint < 500.0 { 5 }
        else if dint < 600.0 { 6 }
        else { 7 };
    let element_index = if matches!(element, BlmElement::Light | BlmElement::Dark) {
        if tier != BlmTier::Helix { return Err(source_key_error("element", element.name())); }
        0 // All eight literal helix rows are identical.
    } else { element as usize };
    let (m, v) = BLM_MV[tier as usize][element_index][window];
    const WINDOWS: [f64; 8] = [0.0, 50.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0];
    Ok((m, v, WINDOWS[window]))
}

/// nuking.py:11–24. Truncate before adding flat damage and percent scaling.
#[inline]
pub fn get_enspell_damage(enhancing_magic_skill: f64, enspell_damage_percent: f64, enspell_damage: f64) -> Result<f64, CalcError> {
    let scaled_skill = if enhancing_magic_skill < 600.0 {
        (enhancing_magic_skill - 223.0) / 7.70
    } else {
        (enhancing_magic_skill - 202.5) / 8.05
    };
    let mut damage = python_int(scaled_skill, "get_enspell_damage", "enhancing_magic_skill")? + 29.0;
    damage += enspell_damage;
    damage *= 1.0 + enspell_damage_percent / 100.0;
    Ok(damage)
}

/// nuking.py:207–254, including literal fractional slopes.
#[inline]
pub fn get_dstat_macc(player_stat: f64, enemy_stat: f64) -> f64 {
    let dstat = player_stat - enemy_stat;
    if dstat <= -70.0 { -30.0 }
    else if dstat <= -30.0 { 0.25 * dstat - 12.5 }
    else if dstat <= -10.0 { 0.5 * dstat - 5.0 }
    else if dstat <= 10.0 { 1.0 * dstat }
    else if dstat <= 30.0 { 0.5 * dstat + 5.0 }
    else if dstat <= 70.0 { 0.25 * dstat + 12.5 }
    else { 30.0 }
}

/// nuking.py:257–268. Negative deltas truncate after division by two.
#[inline]
pub fn get_magic_hit_rate(player_macc: f64, enemy_meva: f64) -> Result<f64, CalcError> {
    let dmacc = player_macc - enemy_meva;
    let mut magic_hit_rate = if dmacc < 0.0 {
        0.50 + python_int(dmacc / 2.0, "get_magic_hit_rate", "dMAcc")? / 100.0
    } else {
        0.50 + python_int(dmacc, "get_magic_hit_rate", "dMAcc")? / 100.0
    };
    if magic_hit_rate > 0.95 { magic_hit_rate = 0.95; }
    if magic_hit_rate < 0.0 { magic_hit_rate = 0.0; }
    Ok(magic_hit_rate)
}

/// nuking.py:270–291. Success is >=, not >. Draw only until the first success;
/// zero/one hit rate still consumes the source's first NumPy-host uniform.
#[inline]
pub fn get_resist_state(magic_hit_rate: f64, random: &mut impl RandomSource) -> f64 {
    let mut resist_state = 1.0;
    for _ in 0..3 {
        if magic_hit_rate >= random.uniform(RandomStream::NumpyHost, 0.0, 1.0) { break; }
        resist_state *= 0.5;
    }
    resist_state
}

/// nuking.py:294–308. Preserve the four-state ordered polynomial, even for
/// probabilities outside [0,1]; this helper does not normalize its inputs.
#[inline]
pub fn get_resist_state_average(magic_hit_rate: f64) -> f64 {
    let missed = 1.0 - magic_hit_rate;
    magic_hit_rate + 0.500 * magic_hit_rate * missed
        + 0.250 * magic_hit_rate * (missed * missed)
        + 0.125 * ((missed * missed) * missed)
}

// get_dint_m_v.py:88–139, transcribed from the inert literal dictionary.
// Each entry is (M,V), in source window order. Apparent discontinuities and
// values such as IV ice M=4.8 and V wind V=1110 are deliberately preserved.
const BLM_MV: [[[(f64, f64); 8]; 6]; 8] = [
    [ // I
        [(2.0, 10.0), (1.0, 110.0), (0.0, 160.0), (0.0, 160.0), (0.0, 160.0), (0.0, 160.0), (0.0, 160.0), (0.0, 160.0)], // earth
        [(1.8, 25.0), (1.0, 115.0), (0.0, 165.0), (0.0, 165.0), (0.0, 165.0), (0.0, 165.0), (0.0, 165.0), (0.0, 165.0)], // water
        [(1.6, 40.0), (1.0, 120.0), (0.0, 170.0), (0.0, 170.0), (0.0, 170.0), (0.0, 170.0), (0.0, 170.0), (0.0, 170.0)], // wind
        [(1.4, 55.0), (1.0, 125.0), (0.0, 175.0), (0.0, 175.0), (0.0, 175.0), (0.0, 175.0), (0.0, 175.0), (0.0, 175.0)], // fire
        [(1.2, 70.0), (1.0, 130.0), (0.0, 180.0), (0.0, 180.0), (0.0, 180.0), (0.0, 180.0), (0.0, 180.0), (0.0, 180.0)], // ice
        [(1.0, 85.0), (1.0, 135.0), (0.0, 185.0), (0.0, 185.0), (0.0, 185.0), (0.0, 185.0), (0.0, 185.0), (0.0, 185.0)], // thunder
    ],
    [ // II
        [(3.0, 100.0), (2.0, 250.0), (1.0, 350.0), (0.0, 450.0), (0.0, 450.0), (0.0, 450.0), (0.0, 450.0), (0.0, 450.0)], // earth
        [(2.8, 120.0), (1.9, 260.0), (1.0, 355.0), (0.0, 455.0), (0.0, 455.0), (0.0, 455.0), (0.0, 455.0), (0.0, 455.0)], // water
        [(2.6, 140.0), (1.8, 270.0), (1.0, 360.0), (0.0, 460.0), (0.0, 460.0), (0.0, 460.0), (0.0, 460.0), (0.0, 460.0)], // wind
        [(2.4, 160.0), (1.7, 280.0), (1.0, 365.0), (0.0, 465.0), (0.0, 465.0), (0.0, 465.0), (0.0, 465.0), (0.0, 465.0)], // fire
        [(2.2, 180.0), (1.6, 290.0), (1.0, 370.0), (0.0, 470.0), (0.0, 470.0), (0.0, 470.0), (0.0, 470.0), (0.0, 470.0)], // ice
        [(2.0, 200.0), (1.5, 300.0), (1.0, 375.0), (0.0, 475.0), (0.0, 475.0), (0.0, 475.0), (0.0, 475.0), (0.0, 475.0)], // thunder
    ],
    [ // III
        [(4.0, 200.0), (3.0, 400.0), (2.0, 550.0), (1.0, 750.0), (0.0, 850.0), (0.0, 850.0), (0.0, 850.0), (0.0, 850.0)], // earth
        [(3.7, 230.0), (2.9, 415.0), (1.95, 560.0), (1.0, 755.0), (0.0, 855.0), (0.0, 855.0), (0.0, 855.0), (0.0, 855.0)], // water
        [(3.4, 260.0), (2.8, 430.0), (1.9, 570.0), (1.0, 760.0), (0.0, 860.0), (0.0, 860.0), (0.0, 860.0), (0.0, 860.0)], // wind
        [(3.1, 290.0), (2.7, 445.0), (1.85, 580.0), (1.0, 765.0), (0.0, 865.0), (0.0, 865.0), (0.0, 865.0), (0.0, 865.0)], // fire
        [(2.8, 320.0), (2.6, 460.0), (1.8, 590.0), (1.0, 770.0), (0.0, 870.0), (0.0, 870.0), (0.0, 870.0), (0.0, 870.0)], // ice
        [(2.5, 350.0), (2.5, 475.0), (1.75, 600.0), (1.0, 775.0), (0.0, 875.0), (0.0, 875.0), (0.0, 875.0), (0.0, 875.0)], // thunder
    ],
    [ // IV
        [(5.0, 400.0), (4.0, 650.0), (3.0, 850.0), (2.0, 1150.0), (1.0, 1350.0), (0.0, 1450.0), (0.0, 1450.0), (0.0, 1450.0)], // earth
        [(4.7, 440.0), (3.9, 675.0), (2.95, 870.0), (1.99, 1165.0), (1.0, 1364.0), (0.0, 1464.0), (0.0, 1464.0), (0.0, 1464.0)], // water
        [(4.4, 480.0), (3.8, 700.0), (2.9, 890.0), (1.98, 1180.0), (1.0, 1378.0), (0.0, 1478.0), (0.0, 1478.0), (0.0, 1478.0)], // wind
        [(4.1, 520.0), (3.7, 730.0), (2.85, 915.0), (1.97, 1195.0), (1.0, 1397.0), (0.0, 1497.0), (0.0, 1497.0), (0.0, 1497.0)], // fire
        [(4.8, 560.0), (3.6, 755.0), (2.8, 935.0), (1.96, 1210.0), (1.0, 1411.0), (0.0, 1511.0), (0.0, 1511.0), (0.0, 1511.0)], // ice
        [(4.5, 600.0), (3.5, 780.0), (2.75, 955.0), (1.95, 1225.0), (1.0, 1325.0), (0.0, 1525.0), (0.0, 1525.0), (0.0, 1525.0)], // thunder
    ],
    [ // V
        [(6.0, 650.0), (5.0, 950.0), (4.0, 1200.0), (3.0, 1600.0), (2.0, 1900.0), (1.0, 2100.0), (0.0, 2200.0), (0.0, 2200.0)], // earth
        [(5.6, 700.0), (4.74, 980.0), (3.95, 1217.0), (2.99, 1612.0), (1.99, 1911.0), (1.0, 2110.0), (0.0, 2210.0), (0.0, 2210.0)], // water
        [(5.2, 750.0), (4.5, 1110.0), (3.9, 1235.0), (2.98, 1625.0), (1.98, 1923.0), (1.0, 2121.0), (0.0, 2221.0), (0.0, 2221.0)], // wind
        [(4.8, 800.0), (4.24, 1140.0), (3.85, 1252.0), (2.97, 1637.0), (1.97, 1934.0), (1.0, 2131.0), (0.0, 2231.0), (0.0, 2231.0)], // fire
        [(4.4, 850.0), (4.0, 1170.0), (3.8, 1270.0), (2.96, 1650.0), (1.96, 1946.0), (1.0, 2142.0), (0.0, 2242.0), (0.0, 2242.0)], // ice
        [(4.0, 900.0), (3.74, 1200.0), (3.75, 1287.0), (2.95, 1662.0), (1.95, 1957.0), (1.0, 2152.0), (0.0, 2252.0), (0.0, 2252.0)], // thunder
    ],
    [ // VI
        [(7.0, 950.0), (6.0, 1300.0), (5.0, 1600.0), (4.0, 2100.0), (3.0, 2500.0), (2.0, 2800.0), (1.0, 3000.0), (0.0, 3100.0)], // earth
        [(6.5, 1010.0), (5.9, 1335.0), (4.9, 1630.0), (3.9, 2100.0), (2.95, 2510.0), (1.99, 2805.0), (1.0, 3004.0), (0.0, 3104.0)], // water
        [(6.0, 1070.0), (5.8, 1370.0), (4.8, 1660.0), (3.8, 2100.0), (2.9, 2520.0), (1.98, 2810.0), (1.0, 3008.0), (0.0, 3108.0)], // wind
        [(5.5, 1130.0), (5.7, 1405.0), (4.7, 1690.0), (3.7, 2100.0), (2.85, 2530.0), (1.97, 2815.0), (1.0, 3012.0), (0.0, 3112.0)], // fire
        [(5.0, 1190.0), (5.6, 1440.0), (4.6, 1720.0), (3.6, 2100.0), (2.8, 2540.0), (1.96, 2820.0), (1.0, 3016.0), (0.0, 3116.0)], // ice
        [(4.5, 1250.0), (5.5, 1475.0), (4.5, 1750.0), (3.5, 2100.0), (2.75, 2550.0), (1.95, 2825.0), (1.0, 3020.0), (0.0, 3120.0)], // thunder
    ],
    [ // ja
        [(6.0, 750.0), (5.0, 1050.0), (3.0, 1700.0), (2.0, 2000.0), (1.0, 2200.0), (0.0, 2300.0), (0.0, 2300.0), (0.0, 2300.0)], // earth
        [(5.6, 800.0), (4.75, 1080.0), (2.98, 1712.0), (2.0, 2010.0), (1.0, 2210.0), (0.0, 2310.0), (0.0, 2310.0), (0.0, 2310.0)], // water
        [(5.2, 850.0), (4.5, 1110.0), (2.96, 1725.0), (2.0, 2021.0), (1.0, 2221.0), (0.0, 2321.0), (0.0, 2321.0), (0.0, 2321.0)], // wind
        [(4.8, 900.0), (4.25, 1140.0), (2.94, 1737.0), (2.0, 2031.0), (1.0, 2231.0), (0.0, 2331.0), (0.0, 2331.0), (0.0, 2331.0)], // fire
        [(4.4, 950.0), (4.0, 1170.0), (2.92, 1750.0), (2.0, 2042.0), (1.0, 2242.0), (0.0, 2342.0), (0.0, 2342.0), (0.0, 2342.0)], // ice
        [(4.0, 1000.0), (3.75, 1200.0), (2.9, 1762.0), (2.0, 2052.0), (1.0, 2252.0), (0.0, 2352.0), (0.0, 2352.0), (0.0, 2352.0)], // thunder
    ],
    [ // helix
        [(2.0, 75.0), (1.0, 175.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0)], // earth
        [(2.0, 75.0), (1.0, 175.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0)], // water
        [(2.0, 75.0), (1.0, 175.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0)], // wind
        [(2.0, 75.0), (1.0, 175.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0)], // fire
        [(2.0, 75.0), (1.0, 175.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0)], // ice
        [(2.0, 75.0), (1.0, 175.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0), (0.0, 225.0)], // thunder
    ],
];
