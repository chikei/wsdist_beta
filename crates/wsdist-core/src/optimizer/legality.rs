use std::sync::{Arc, LazyLock};

use crate::catalog::Catalog;
use crate::model::{Gearset, Job, ModelState, PieceKind, SkillType, Slot, WsType};

pub const WS_MENUS: &[(SkillType, &[&str])] = &[
    (SkillType::Katana, &["Blade: Retsu", "Blade: Teki", "Blade: To", "Blade: Chi", "Blade: Ei", "Blade: Jin", "Blade: Ten", "Blade: Ku", "Blade: Yu", "Blade: Metsu", "Blade: Kamu", "Blade: Hi", "Blade: Shun", "Zesho Meppo"]),
    (SkillType::GreatKatana, &["Tachi: Enpi", "Tachi: Goten", "Tachi: Kagero", "Tachi: Jinpu", "Tachi: Koki", "Tachi: Yukikaze", "Tachi: Gekko", "Tachi: Kasha", "Tachi: Ageha", "Tachi: Kaiten", "Tachi: Rana", "Tachi: Fudo", "Tachi: Shoha", "Tachi: Mumei"]),
    (SkillType::Dagger, &["Viper Bite", "Dancing Edge", "Shark Bite", "Evisceration", "Aeolian Edge", "Mercy Stroke", "Mandalic Stab", "Mordant Rime", "Pyrrhic Kleos", "Rudra's Storm", "Exenterator", "Ruthless Stroke"]),
    (SkillType::Sword, &["Fast Blade", "Fast Blade II", "Burning Blade", "Red Lotus Blade", "Seraph Blade", "Circle Blade", "Swift Blade", "Savage Blade", "Sanguine Blade", "Knights of Round", "Death Blossom", "Expiacion", "Chant du Cygne", "Requiescat", "Imperator"]),
    (SkillType::Scythe, &["Slice", "Dark Harvest", "Shadow of Death", "Nightmare Scythe", "Spinning Scythe", "Guillotine", "Cross Reaper", "Spiral Hell", "Infernal Scythe", "Catastrophe", "Quietus", "Insurgency", "Entropy", "Origin"]),
    (SkillType::GreatSword, &["Hard Slash", "Freezebite", "Shockwave", "Sickle Moon", "Spinning Slash", "Ground Strike", "Herculean Slash", "Resolution", "Scourge", "Dimidiation", "Torcleaver", "Fimbulvetr"]),
    (SkillType::Club, &["Shining Strike", "Seraph Strike", "Skullbreaker", "True Strike", "Judgment", "Hexa Strike", "Black Halo", "Randgrith", "Exudation", "Mystic Boon", "Realmrazer", "Dagda"]),
    (SkillType::Polearm, &["Double Thrust", "Thunder Thrust", "Raiden Thrust", "Penta Thrust", "Wheeling Thrust", "Impulse Drive", "Sonic Thrust", "Geirskogul", "Drakesbane", "Camlann's Torment", "Stardiver", "Diarmuid"]),
    (SkillType::Staff, &["Heavy Swing", "Rock Crusher", "Earth Crusher", "Starburst", "Sunburst", "Shell Crusher", "Full Swing", "Cataclysm", "Retribution", "Gate of Tartarus", "Omniscience", "Vidohunir", "Garland of Bliss", "Shattersoul", "Oshala"]),
    (SkillType::GreatAxe, &["Iron Tempest", "Shield Break", "Armor Break", "Weapon Break", "Raging Rush", "Full Break", "Steel Cyclone", "Fell Cleave", "Metatron Torment", "King's Justice", "Ukko's Fury", "Upheaval", "Disaster"]),
    (SkillType::Axe, &["Raging Axe", "Spinning Axe", "Rampage", "Calamity", "Mistral Axe", "Decimation", "Bora Axe", "Onslaught", "Primal Rend", "Cloudsplitter", "Ruinator", "Blitz"]),
    (SkillType::Archery, &["Flaming Arrow", "Piercing Arrow", "Dulling Arrow", "Sidewinder", "Blast Arrow", "Empyreal Arrow", "Refulgent Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Sarv"]),
    (SkillType::Marksmanship, &["Hot Shot", "Split Shot", "Sniper Shot", "Slug Shot", "Blast Shot", "Detonator", "Coronach", "Leaden Salute", "Trueflight", "Wildfire", "Last Stand", "Terminus"]),
    (SkillType::HandToHand, &["Combo", "One Inch Punch", "Raging Fists", "Spinning Attack", "Howling Fist", "Dragon Kick", "Asuran Fists", "Tornado Kick", "Ascetic's Fury", "Stringing Pummel", "Final Heaven", "Victory Smite", "Shijin Spiral", "Maru Kala", "Dragon Blow"]),
];

pub const RESTRICTED_WS: &[(&str, &str)] = &[
    ("Blade: Metsu", "Kikoku"), ("Final Heaven", "Spharai"), ("Mercy Stroke", "Mandau"),
    ("Knights of Round", "Excalibur"), ("Scourge", "Ragnarok"), ("Onslaught", "Guttler"),
    ("Metatron Torment", "Bravura"), ("Catastrophe", "Apocalypse"), ("Geirskogul", "Gungnir"),
    ("Tachi: Kaiten", "Amanomurakumo"), ("Randgrith", "Mjollnir"), ("Gate of Tartarus", "Claustrum"),
    ("Namas Arrow", "Yoichinoyumi"), ("Coronach", "Annihilator"), ("Fast Blade II", "Onion Sword III"),
    ("Dragon Blow", "Dragon Fangs"), ("Imperator", "Caliburnus"), ("Zesho Meppo", "Dokoku"),
    ("Terminus", "Earp"), ("Origin", "Foenaria"), ("Diarmuid", "Gae Buide"), ("Fimbulvetr", "Helheim"),
    ("Tachi: Mumei", "Kusanagi-no-Tsurugi"), ("Disaster", "Laphria"), ("Dagda", "Lorg Mor"),
    ("Ruthless Stroke", "Mpu Gandring"), ("Oshala", "Opashoro"), ("Sarv", "Pinaka"),
    ("Blitz", "Spalirisos"), ("Maru Kala", "Varga Purnikawa"),
];

const JSE_PREFIXES: &[&str] = &["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"];

static JSE_EARS: LazyLock<Vec<Arc<str>>> = LazyLock::new(|| {
    [1, 2].into_iter().flat_map(|rank| JSE_PREFIXES.iter().map(move |prefix| Arc::from(format!("{prefix} Earring +{rank}")))).collect()
});
static PRODUCTION_MENUS: LazyLock<Vec<(SkillType, Vec<Arc<str>>)>> = LazyLock::new(|| {
    WS_MENUS.iter().map(|(skill, names)| (*skill, names.iter().map(|name| Arc::from(*name)).collect())).collect()
});
static PRODUCTION_RESTRICTIONS: LazyLock<Vec<(Arc<str>, Arc<str>)>> = LazyLock::new(|| {
    RESTRICTED_WS.iter().map(|(name, weapon)| (Arc::from(*name), Arc::from(*weapon))).collect()
});

pub fn jse_ear_names() -> Vec<Arc<str>> {
    JSE_EARS.clone()
}

pub fn optimizer_ws_type(name: &str) -> WsType {
    WS_MENUS.iter().find_map(|(skill, menu)| menu.contains(&name).then_some(
        if matches!(skill, SkillType::Archery | SkillType::Marksmanship) { WsType::Ranged } else { WsType::Melee },
    )).unwrap_or(WsType::None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind { WeaponSkill, SpellCast, AttackRound, Other }

impl SearchKind {
    pub fn from_name(name: &str) -> Self {
        match name { "weapon skill" => Self::WeaponSkill, "spell cast" => Self::SpellCast, "attack round" => Self::AttackRound, _ => Self::Other }
    }
}

#[derive(Clone, Debug)]
pub struct LegalityRules {
    pub main_job: Job,
    pub sub_job: Option<Job>,
    pub kind: SearchKind,
    pub spell_name: Arc<str>,
    ws_skills: u32,
    required_weapon: Option<Arc<str>>,
    archery_ws: bool,
    marksmanship_ws: bool,
    pub jse_ears: Vec<Arc<str>>,
}

impl LegalityRules {
    pub fn new(main_job: Job, sub_job: Option<Job>, action_type: &str, ws_name: &str, spell_name: impl Into<Arc<str>>,
        ws_dict: &[(SkillType, Vec<Arc<str>>)], restricted_ws: &[(Arc<str>, Arc<str>)]) -> Self {
        let ws_skills = ws_dict.iter().filter(|(_, names)| names.iter().any(|name| name.as_ref() == ws_name))
            .fold(0, |mask, (skill, _)| mask | (1 << *skill as u32));
        Self { main_job, sub_job, kind: SearchKind::from_name(action_type), spell_name: spell_name.into(), ws_skills,
            required_weapon: restricted_ws.iter().find(|(name, _)| name.as_ref() == ws_name).map(|(_, weapon)| weapon.clone()),
            archery_ws: ["Empyreal Arrow", "Flaming Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Refulgent Arrow", "Sidewinder", "Blast Arrow", "Piercing Arrow"].contains(&ws_name),
            marksmanship_ws: ["Last Stand", "Hot Shot", "Leaden Salute", "Wildfire", "Coronach", "Trueflight", "Detonator", "Blast Shot", "Slug Shot", "Split Shot"].contains(&ws_name),
            jse_ears: jse_ear_names() }
    }

    pub fn production(main_job: Job, sub_job: Option<Job>, action_type: &str, ws_name: &str, spell_name: &str) -> Self {
        Self::new(main_job, sub_job, action_type, ws_name, spell_name, &PRODUCTION_MENUS, &PRODUCTION_RESTRICTIONS)
    }

    pub fn is_valid(&self, gearset: &Gearset, catalog: &Catalog, state: &ModelState) -> bool {
        let piece = |slot: Slot| catalog.piece(gearset[slot.index()]);
        let main = piece(Slot::Main); let sub = piece(Slot::Sub);
        let ranged = piece(Slot::Ranged); let ammo = piece(Slot::Ammo);
        let ear1 = piece(Slot::Ear1); let ear2 = piece(Slot::Ear2);
        for (first, second) in [(Slot::Ring1, Slot::Ring2), (Slot::Ear1, Slot::Ear2), (Slot::Main, Slot::Sub)] {
            if piece(first).value_equal(piece(second), state) && piece(first).name.as_ref() != "Empty" { return false; }
        }
        let main_skill = state.skill(main);
        if matches!(main_skill, SkillType::Axe | SkillType::Club | SkillType::Dagger | SkillType::Sword | SkillType::Katana) && sub.kind == PieceKind::Grip { return false; }
        if matches!(main_skill, SkillType::GreatSword | SkillType::GreatKatana | SkillType::GreatAxe | SkillType::Polearm | SkillType::Scythe | SkillType::Staff)
            && matches!(sub.kind, PieceKind::Weapon | PieceKind::Shield) { return false; }
        if main_skill == SkillType::HandToHand && sub.name.as_ref() != "Empty" { return false; }
        if self.kind == SearchKind::WeaponSkill && (self.archery_ws || self.marksmanship_ws) {
            if self.archery_ws && (state.skill(ranged) != SkillType::Archery || ammo.kind != PieceKind::Arrow) { return false; }
            if self.marksmanship_ws && (state.skill(ranged) != SkillType::Marksmanship || !matches!(ammo.kind, PieceKind::Bolt | PieceKind::Bullet)) { return false; }
            if ranged.kind == PieceKind::Crossbow && ammo.kind != PieceKind::Bolt { return false; }
            if ranged.kind == PieceKind::Gun && ammo.kind != PieceKind::Bullet { return false; }
        }
        if self.kind == SearchKind::SpellCast && self.spell_name.as_ref() == "Ranged Attack"
            && (!matches!(ranged.kind, PieceKind::Gun | PieceKind::Bow | PieceKind::Crossbow) || !matches!(ammo.kind, PieceKind::Bullet | PieceKind::Arrow | PieceKind::Bolt)) { return false; }
        if ranged.kind == PieceKind::Gun && !matches!(ammo.kind, PieceKind::Bullet | PieceKind::None) { return false; }
        if ranged.kind == PieceKind::Bow && !matches!(ammo.kind, PieceKind::Arrow | PieceKind::None) { return false; }
        if ranged.kind == PieceKind::Crossbow && !matches!(ammo.kind, PieceKind::Bolt | PieceKind::None) { return false; }
        if ammo.kind == PieceKind::Bullet && ranged.kind != PieceKind::Gun { return false; }
        if ammo.kind == PieceKind::Arrow && ranged.kind != PieceKind::Bow { return false; }
        if ammo.kind == PieceKind::Bolt && ranged.kind != PieceKind::Crossbow { return false; }
        if ranged.kind == PieceKind::Instrument && ammo.kind != PieceKind::None { return false; }
        let main_dual = matches!(self.main_job, Job::Nin | Job::Dnc | Job::Thf | Job::Blu);
        let sub_dual = matches!(self.sub_job, Some(Job::Nin | Job::Dnc));
        if !main_dual && !sub_dual && sub.kind == PieceKind::Weapon { return false; }
        if self.jse_ears.contains(&ear1.name) && ear2.name.as_ref() == "Balder Earring +1" { return false; }
        if self.jse_ears.contains(&ear2.name) && ear1.name.as_ref() == "Balder Earring +1" { return false; }
        if ["Cohort Cloak", "Cohort Cloak +1", "Crepuscular Cloak", "Twilight Cloak"].contains(&piece(Slot::Body).name.as_ref()) && piece(Slot::Head).name.as_ref() != "Empty" { return false; }
        if self.kind == SearchKind::SpellCast && self.spell_name.as_ref() == "Impact" && !["Crepuscular Cloak", "Twilight Cloak"].contains(&piece(Slot::Body).name.as_ref()) { return false; }
        if self.kind == SearchKind::WeaponSkill {
            if let Some(weapon) = &self.required_weapon {
                if weapon != &main.name && weapon != &ranged.name { return false; }
            }
            if self.ws_skills & ((1 << main_skill as u32) | (1 << state.skill(ranged) as u32)) == 0 { return false; }
        }
        true
    }
}
