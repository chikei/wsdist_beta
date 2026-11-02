/// Exact source-name keys resolved at the catalog/action boundary, never
/// inside a hit loop. Missing weapons/skills intentionally resolve to zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponBonusWeapon(u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponBonusSkill(u8);

impl WeaponBonusWeapon {
    pub fn from_name(name: &str) -> Self {
        Self(WEAPON_BONUSES.iter().position(|entry| entry.weapon == name).map_or(u8::MAX, |index| index as u8))
    }
}

impl WeaponBonusSkill {
    pub fn from_name(name: &str) -> Self {
        Self(WEAPON_BONUSES.iter().find(|entry| entry.skill == name).map_or(u8::MAX, |entry| entry.skill_index))
    }
}

/// weapon_bonus.py:6–140. Both main and ranged contributions are added in
/// source order, including a duplicate weapon contributing twice.
#[inline]
pub fn get_weapon_bonus(main_wpn: WeaponBonusWeapon, rng_wpn: WeaponBonusWeapon, ws: WeaponBonusSkill) -> f64 {
    let mut bonus = 0.0;
    if let Some(entry) = WEAPON_BONUSES.get(main_wpn.0 as usize) {
        if entry.skill_index == ws.0 { bonus += entry.amount; }
    }
    if let Some(entry) = WEAPON_BONUSES.get(rng_wpn.0 as usize) {
        if entry.skill_index == ws.0 { bonus += entry.amount; }
    }
    bonus
}

struct WeaponBonusEntry {
    weapon: &'static str,
    skill: &'static str,
    skill_index: u8,
    amount: f64,
}

// Exact literal names include Gokotai and Dojikiri Yasutsuna; do not normalize.
const WEAPON_BONUSES: &[WeaponBonusEntry] = &[
    WeaponBonusEntry { weapon: "Dolichenus", skill: "Decimation", skill_index: 0, amount: 1.2 },
    WeaponBonusEntry { weapon: "Drepanum", skill: "Spiral Hell", skill_index: 1, amount: 1.0 },
    WeaponBonusEntry { weapon: "Gokotai", skill: "Blade: Ku", skill_index: 2, amount: 0.6 },
    WeaponBonusEntry { weapon: "Hachimonji", skill: "Tachi: Kasha", skill_index: 3, amount: 0.25 },
    WeaponBonusEntry { weapon: "Karambit", skill: "Asuran Fists", skill_index: 4, amount: 0.5 },
    WeaponBonusEntry { weapon: "Lycurgos", skill: "Steel Cyclone", skill_index: 5, amount: 0.3 },
    WeaponBonusEntry { weapon: "Maxentius", skill: "Black Halo", skill_index: 6, amount: 0.5 },
    WeaponBonusEntry { weapon: "Naegling", skill: "Savage Blade", skill_index: 7, amount: 0.15 },
    WeaponBonusEntry { weapon: "Nandaka", skill: "Ground Strike", skill_index: 8, amount: 0.15 },
    WeaponBonusEntry { weapon: "Shining One", skill: "Impulse Drive", skill_index: 9, amount: 0.4 },
    WeaponBonusEntry { weapon: "Tauret", skill: "Evisceration", skill_index: 10, amount: 0.5 },
    WeaponBonusEntry { weapon: "Ullr", skill: "Empyreal Arrow", skill_index: 11, amount: 0.5 },
    WeaponBonusEntry { weapon: "Xoanon", skill: "Retribution", skill_index: 12, amount: 0.2 },
    WeaponBonusEntry { weapon: "Amanomurakumo", skill: "Tachi: Kaiten", skill_index: 13, amount: 0.4 },
    WeaponBonusEntry { weapon: "Annihilator", skill: "Coronach", skill_index: 14, amount: 0.4 },
    WeaponBonusEntry { weapon: "Apocalypse", skill: "Catastrophe", skill_index: 15, amount: 0.4 },
    WeaponBonusEntry { weapon: "Bravura", skill: "Metatron Torment", skill_index: 16, amount: 0.4 },
    WeaponBonusEntry { weapon: "Excalibur", skill: "Knights of Round", skill_index: 17, amount: 0.4 },
    WeaponBonusEntry { weapon: "Gungnir", skill: "Geirskogul", skill_index: 18, amount: 0.4 },
    WeaponBonusEntry { weapon: "Guttler", skill: "Onslaught", skill_index: 19, amount: 0.4 },
    WeaponBonusEntry { weapon: "Kikoku", skill: "Blade: Metsu", skill_index: 20, amount: 0.4 },
    WeaponBonusEntry { weapon: "Mandau", skill: "Mercy Stroke", skill_index: 21, amount: 0.4 },
    WeaponBonusEntry { weapon: "Mjollnir", skill: "Randgrith", skill_index: 22, amount: 0.4 },
    WeaponBonusEntry { weapon: "Ragnarok", skill: "Scourge", skill_index: 23, amount: 0.4 },
    WeaponBonusEntry { weapon: "Spharai", skill: "Final Heaven", skill_index: 24, amount: 0.4 },
    WeaponBonusEntry { weapon: "Yoichinoyumi", skill: "Namas Arrow", skill_index: 25, amount: 0.4 },
    WeaponBonusEntry { weapon: "Amanomurakumo R15", skill: "Tachi: Kaiten", skill_index: 13, amount: 0.68 },
    WeaponBonusEntry { weapon: "Annihilator R15", skill: "Coronach", skill_index: 14, amount: 0.68 },
    WeaponBonusEntry { weapon: "Apocalypse R15", skill: "Catastrophe", skill_index: 15, amount: 0.68 },
    WeaponBonusEntry { weapon: "Bravura R15", skill: "Metatron Torment", skill_index: 16, amount: 0.68 },
    WeaponBonusEntry { weapon: "Excalibur R15", skill: "Knights of Round", skill_index: 17, amount: 0.68 },
    WeaponBonusEntry { weapon: "Gungnir R15", skill: "Geirskogul", skill_index: 18, amount: 0.68 },
    WeaponBonusEntry { weapon: "Guttler R15", skill: "Onslaught", skill_index: 19, amount: 0.68 },
    WeaponBonusEntry { weapon: "Kikoku R15", skill: "Blade: Metsu", skill_index: 20, amount: 0.68 },
    WeaponBonusEntry { weapon: "Mandau R15", skill: "Mercy Stroke", skill_index: 21, amount: 0.68 },
    WeaponBonusEntry { weapon: "Mjollnir R15", skill: "Randgrith", skill_index: 22, amount: 0.68 },
    WeaponBonusEntry { weapon: "Ragnarok R15", skill: "Scourge", skill_index: 23, amount: 0.68 },
    WeaponBonusEntry { weapon: "Spharai R15", skill: "Final Heaven", skill_index: 24, amount: 0.68 },
    WeaponBonusEntry { weapon: "Yoichinoyumi R15", skill: "Namas Arrow", skill_index: 25, amount: 0.68 },
    WeaponBonusEntry { weapon: "Almace R15", skill: "Chant du Cygne", skill_index: 26, amount: 0.1 },
    WeaponBonusEntry { weapon: "Armageddon R15", skill: "Wildfire", skill_index: 27, amount: 0.1 },
    WeaponBonusEntry { weapon: "Caladbolg R15", skill: "Torcleaver", skill_index: 28, amount: 0.1 },
    WeaponBonusEntry { weapon: "Farsha R15", skill: "Cloudsplitter", skill_index: 29, amount: 0.1 },
    WeaponBonusEntry { weapon: "Gandiva R15", skill: "Jishnu's Radiance", skill_index: 30, amount: 0.1 },
    WeaponBonusEntry { weapon: "Kannagi R15", skill: "Blade: Hi", skill_index: 31, amount: 0.1 },
    WeaponBonusEntry { weapon: "Masamune R15", skill: "Tachi: Fudo", skill_index: 32, amount: 0.1 },
    WeaponBonusEntry { weapon: "Redemption R15", skill: "Quietus", skill_index: 33, amount: 0.1 },
    WeaponBonusEntry { weapon: "Rhongomiant R15", skill: "Camlann's Torment", skill_index: 34, amount: 0.1 },
    WeaponBonusEntry { weapon: "Twashtar R15", skill: "Rudra's Storm", skill_index: 35, amount: 0.1 },
    WeaponBonusEntry { weapon: "Ukonvasara R15", skill: "Ukko's Fury", skill_index: 36, amount: 0.1 },
    WeaponBonusEntry { weapon: "Verethragna R15", skill: "Victory Smite", skill_index: 37, amount: 0.1 },
    WeaponBonusEntry { weapon: "Aymur R15", skill: "Primal Rend", skill_index: 38, amount: 0.495 },
    WeaponBonusEntry { weapon: "Burtgang R15", skill: "Atonement", skill_index: 39, amount: 0.3 },
    WeaponBonusEntry { weapon: "Carnwenhan R15", skill: "Mordant Rime", skill_index: 40, amount: 0.495 },
    WeaponBonusEntry { weapon: "Conqueror R15", skill: "King's Justice", skill_index: 41, amount: 0.495 },
    WeaponBonusEntry { weapon: "Death Penalty R15", skill: "Leaden Salute", skill_index: 42, amount: 0.495 },
    WeaponBonusEntry { weapon: "Gastraphetes R15", skill: "Trueflight", skill_index: 43, amount: 0.495 },
    WeaponBonusEntry { weapon: "Glanzfaust R15", skill: "Ascetic's Fury", skill_index: 44, amount: 0.495 },
    WeaponBonusEntry { weapon: "Kenkonken R15", skill: "Stringing Pummel", skill_index: 45, amount: 0.495 },
    WeaponBonusEntry { weapon: "Kogarasumaru R15", skill: "Tachi: Rana", skill_index: 46, amount: 0.495 },
    WeaponBonusEntry { weapon: "Laevateinn R15", skill: "Vidohunir", skill_index: 47, amount: 0.495 },
    WeaponBonusEntry { weapon: "Liberator R15", skill: "Insurgency", skill_index: 48, amount: 0.495 },
    WeaponBonusEntry { weapon: "Murgleis R15", skill: "Death Blossom", skill_index: 49, amount: 0.495 },
    WeaponBonusEntry { weapon: "Nagi R15", skill: "Blade: Kamu", skill_index: 50, amount: 0.495 },
    WeaponBonusEntry { weapon: "Ryunohige R15", skill: "Drakesbane", skill_index: 51, amount: 0.495 },
    WeaponBonusEntry { weapon: "Terpsichore R15", skill: "Pyrrhic Kleos", skill_index: 52, amount: 0.495 },
    WeaponBonusEntry { weapon: "Tizona R15", skill: "Expiacion", skill_index: 53, amount: 0.495 },
    WeaponBonusEntry { weapon: "Tupsimati R15", skill: "Omniscience", skill_index: 54, amount: 0.495 },
    WeaponBonusEntry { weapon: "Nirvana R15", skill: "Garland of Bliss", skill_index: 55, amount: 0.495 },
    WeaponBonusEntry { weapon: "Vajra R15", skill: "Mandalic Stab", skill_index: 56, amount: 0.495 },
    WeaponBonusEntry { weapon: "Yagrush R15", skill: "Mystic Boon", skill_index: 57, amount: 0.495 },
    WeaponBonusEntry { weapon: "Aymur", skill: "Primal Rend", skill_index: 38, amount: 0.3 },
    WeaponBonusEntry { weapon: "Burtgang", skill: "Atonement", skill_index: 39, amount: 0.3 },
    WeaponBonusEntry { weapon: "Carnwenhan", skill: "Mordant Rime", skill_index: 40, amount: 0.3 },
    WeaponBonusEntry { weapon: "Conqueror", skill: "King's Justice", skill_index: 41, amount: 0.3 },
    WeaponBonusEntry { weapon: "Death Penalty", skill: "Leaden Salute", skill_index: 42, amount: 0.3 },
    WeaponBonusEntry { weapon: "Gastraphetes", skill: "Trueflight", skill_index: 43, amount: 0.3 },
    WeaponBonusEntry { weapon: "Glanzfaust", skill: "Ascetic's Fury", skill_index: 44, amount: 0.3 },
    WeaponBonusEntry { weapon: "Kenkonken", skill: "Stringing Pummel", skill_index: 45, amount: 0.3 },
    WeaponBonusEntry { weapon: "Kogarasumaru", skill: "Tachi: Rana", skill_index: 46, amount: 0.3 },
    WeaponBonusEntry { weapon: "Laevateinn", skill: "Vidohunir", skill_index: 47, amount: 0.3 },
    WeaponBonusEntry { weapon: "Liberator", skill: "Insurgency", skill_index: 48, amount: 0.3 },
    WeaponBonusEntry { weapon: "Murgleis", skill: "Death Blossom", skill_index: 49, amount: 0.3 },
    WeaponBonusEntry { weapon: "Nagi", skill: "Blade: Kamu", skill_index: 50, amount: 0.3 },
    WeaponBonusEntry { weapon: "Ryunohige", skill: "Drakesbane", skill_index: 51, amount: 0.3 },
    WeaponBonusEntry { weapon: "Terpsichore", skill: "Pyrrhic Kleos", skill_index: 52, amount: 0.3 },
    WeaponBonusEntry { weapon: "Tizona", skill: "Expiacion", skill_index: 53, amount: 0.3 },
    WeaponBonusEntry { weapon: "Tupsimati", skill: "Omniscience", skill_index: 54, amount: 0.3 },
    WeaponBonusEntry { weapon: "Nirvana", skill: "Garland of Bliss", skill_index: 55, amount: 0.3 },
    WeaponBonusEntry { weapon: "Vajra", skill: "Mandalic Stab", skill_index: 56, amount: 0.3 },
    WeaponBonusEntry { weapon: "Yagrush", skill: "Mystic Boon", skill_index: 57, amount: 0.3 },
    WeaponBonusEntry { weapon: "Epeolatry R15", skill: "Dimidiation", skill_index: 58, amount: 0.15 },
    WeaponBonusEntry { weapon: "Idris R15", skill: "Exudation", skill_index: 59, amount: 0.15 },
    WeaponBonusEntry { weapon: "Aeneas R15", skill: "Exenterator", skill_index: 60, amount: 0.1 },
    WeaponBonusEntry { weapon: "Anguta R15", skill: "Entropy", skill_index: 61, amount: 0.1 },
    WeaponBonusEntry { weapon: "Chango R15", skill: "Upheaval", skill_index: 62, amount: 0.1 },
    WeaponBonusEntry { weapon: "Dojikiri Yasutsuna R15", skill: "Tachi: Shoha", skill_index: 63, amount: 0.1 },
    WeaponBonusEntry { weapon: "Fail-not R15", skill: "Apex Arrow", skill_index: 64, amount: 0.1 },
    WeaponBonusEntry { weapon: "Fomalhaut R15", skill: "Last Stand", skill_index: 65, amount: 0.1 },
    WeaponBonusEntry { weapon: "Godhands R15", skill: "Shijin Spiral", skill_index: 66, amount: 0.1 },
    WeaponBonusEntry { weapon: "Heishi Shorinken R15", skill: "Blade: Shun", skill_index: 67, amount: 0.1 },
    WeaponBonusEntry { weapon: "Khatvanga R15", skill: "Shattersoul", skill_index: 68, amount: 0.1 },
    WeaponBonusEntry { weapon: "Lionheart R15", skill: "Resolution", skill_index: 69, amount: 0.1 },
    WeaponBonusEntry { weapon: "Sequence R15", skill: "Requiescat", skill_index: 70, amount: 0.1 },
    WeaponBonusEntry { weapon: "Tishtrya R15", skill: "Realmrazer", skill_index: 71, amount: 0.1 },
    WeaponBonusEntry { weapon: "Tri-edge R15", skill: "Ruinator", skill_index: 72, amount: 0.1 },
    WeaponBonusEntry { weapon: "Trishula R15", skill: "Stardiver", skill_index: 73, amount: 0.1 },
];
