use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::stats::{Stat, Stats};

#[derive(Debug, thiserror::Error)]
pub enum CalcError {
    #[error("{path}: {json_path}: {message}")]
    Data { path: PathBuf, json_path: String, message: String },
    #[error("{operation}: {field}: {category}: {message}")]
    Legacy { operation: &'static str, field: &'static str, category: &'static str, message: String },
    #[error("{operation}: {field}: {message}")]
    Input { operation: &'static str, field: &'static str, message: String },
    #[error("{operation} interrupted")]
    Interrupted { operation: &'static str },
    #[error("{operation}: TP {starting_tp} cannot reach {threshold}; per-round upper bound {upper_bound}")]
    NonProgress { operation: &'static str, starting_tp: f64, threshold: f64, upper_bound: f64 },
}

impl CalcError {
    pub fn data(path: impl Into<PathBuf>, json_path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Data { path: path.into(), json_path: json_path.into(), message: message.into() }
    }

    pub fn legacy(operation: &'static str, field: &'static str, category: &'static str, message: impl Into<String>) -> Self {
        Self::Legacy { operation, field, category, message: message.into() }
    }
}

macro_rules! named_enum {
    ($visibility:vis enum $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        $visibility enum $name {
            $(#[serde(rename = $text)] $variant),+
        }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($text => Some(Self::$variant)),+, _ => None }
            }
        }
    };
}

mod inputs;
mod numbers;
pub use inputs::*;
pub use numbers::*;

named_enum! {
    pub enum Job {
        War => "war", Mnk => "mnk", Whm => "whm", Blm => "blm", Rdm => "rdm", Thf => "thf",
        Pld => "pld", Drk => "drk", Bst => "bst", Brd => "brd", Rng => "rng", Smn => "smn",
        Sam => "sam", Nin => "nin", Drg => "drg", Blu => "blu", Cor => "cor", Pup => "pup",
        Dnc => "dnc", Sch => "sch", Geo => "geo", Run => "run"
    }
}

impl Job {
    pub fn parse_player_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|job| job.name().eq_ignore_ascii_case(name))
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::War => "Warrior", Self::Mnk => "Monk", Self::Whm => "White Mage",
            Self::Blm => "Black Mage", Self::Rdm => "Red Mage", Self::Thf => "Thief",
            Self::Pld => "Paladin", Self::Drk => "Dark Knight", Self::Bst => "Beastmaster",
            Self::Brd => "Bard", Self::Rng => "Ranger", Self::Smn => "Summoner",
            Self::Sam => "Samurai", Self::Nin => "Ninja", Self::Drg => "Dragoon",
            Self::Blu => "Blue Mage", Self::Cor => "Corsair", Self::Pup => "Puppetmaster",
            Self::Dnc => "Dancer", Self::Sch => "Scholar", Self::Geo => "Geomancer",
            Self::Run => "Rune Fencer",
        }
    }

    pub const fn mask(self) -> u32 { 1 << self as u32 }
}

named_enum! {
    pub enum Slot {
        Main => "main", Sub => "sub", Ranged => "ranged", Ammo => "ammo", Head => "head", Neck => "neck",
        Ear1 => "ear1", Ear2 => "ear2", Body => "body", Hands => "hands", Ring1 => "ring1",
        Ring2 => "ring2", Back => "back", Waist => "waist", Legs => "legs", Feet => "feet"
    }
}

impl Slot {
    pub const COUNT: usize = 16;
    pub const fn index(self) -> usize { self as usize }
}

named_enum! {
    pub enum SkillType {
        None => "None", GreatKatana => "Great Katana", Scythe => "Scythe", GreatSword => "Great Sword",
        Dagger => "Dagger", Katana => "Katana", Club => "Club", Sword => "Sword", Polearm => "Polearm",
        Staff => "Staff", GreatAxe => "Great Axe", Axe => "Axe", HandToHand => "Hand-to-Hand",
        Shield => "Shield", Throwing => "Throwing", Marksmanship => "Marksmanship", Archery => "Archery",
        Instrument => "Instrument"
    }
}

impl SkillType {
    pub const fn skill_stat(self) -> Stat {
        match self {
            Self::None => Stat::NoneSkill, Self::GreatKatana => Stat::GreatKatanaSkill,
            Self::Scythe => Stat::ScytheSkill, Self::GreatSword => Stat::GreatSwordSkill,
            Self::Dagger => Stat::DaggerSkill, Self::Katana => Stat::KatanaSkill, Self::Club => Stat::ClubSkill,
            Self::Sword => Stat::SwordSkill, Self::Polearm => Stat::PolearmSkill, Self::Staff => Stat::StaffSkill,
            Self::GreatAxe => Stat::GreatAxeSkill, Self::Axe => Stat::AxeSkill, Self::HandToHand => Stat::HandToHandSkill,
            Self::Shield => Stat::ShieldSkill, Self::Throwing => Stat::ThrowingSkill,
            Self::Marksmanship => Stat::MarksmanshipSkill, Self::Archery => Stat::ArcherySkill,
            Self::Instrument => Stat::InstrumentSkill,
        }
    }
}

named_enum! {
    pub enum PieceKind {
        None => "None", Weapon => "Weapon", Equipment => "Equipment", Instrument => "Instrument",
        Gun => "Gun", Bow => "Bow", Food => "Food", Shield => "Shield", Bullet => "Bullet",
        Crossbow => "Crossbow", Grip => "Grip", Arrow => "Arrow", Shuriken => "Shuriken", Bolt => "Bolt",
        Armor => "Armor"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GearId(pub(crate) u32);

impl GearId {
    pub const fn index(self) -> usize { self.0 as usize }
}

pub type Gearset = [GearId; Slot::COUNT];

#[derive(Clone, Debug)]
pub struct StatContribution {
    pub stat: Stat,
    pub value: f64,
    pub literal: serde_json::Number,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WscContribution {
    pub stat: Stat,
    pub percent: f64,
}

#[derive(Debug)]
pub struct GearPiece {
    pub id: GearId,
    pub name: Arc<str>,
    pub name2: Arc<str>,
    pub jobs: Box<[Job]>,
    pub job_mask: u32,
    pub kind: PieceKind,
    pub skill_type: SkillType,
    pub dmg: f64,
    pub delay: f64,
    pub rank: Option<i64>,
    pub wsc: Option<WscContribution>,
    pub stats: Box<[StatContribution]>,
}

impl GearPiece {
    pub fn eligible_for(&self, job: Job) -> bool { self.job_mask & job.mask() != 0 }
    pub fn stat(&self, stat: Stat) -> f64 {
        self.stats.iter().find(|entry| entry.stat == stat).map_or(0.0, |entry| entry.value)
    }

    pub fn value_equal(&self, other: &Self, state: &ModelState) -> bool {
        self.name == other.name && self.name2 == other.name2 && self.jobs == other.jobs
            && self.kind == other.kind && state.skill(self) == state.skill(other)
            && self.dmg == other.dmg && self.delay == other.delay && self.rank == other.rank
            && self.wsc == other.wsc && self.stats.len() == other.stats.len()
            && self.stats.iter().all(|entry| other.stats.iter().any(|candidate|
                candidate.stat == entry.stat && numeric_equal(&entry.literal, &candidate.literal)))
    }
}

fn numeric_equal(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    let integer = |number: &serde_json::Number| number.as_i64().map(i128::from).or_else(|| number.as_u64().map(i128::from));
    match (integer(a), integer(b)) {
        (Some(a), Some(b)) => a == b,
        (None, None) => a.as_f64() == b.as_f64(),
        (Some(integer), None) => b.as_f64().is_some_and(|value| value.fract() == 0.0 && value as i128 == integer),
        (None, Some(integer)) => a.as_f64().is_some_and(|value| value.fract() == 0.0 && value as i128 == integer),
    }
}

#[derive(Clone, Debug)]
pub struct ModelState {
    skill_overrides: Vec<Option<SkillType>>,
    generation: u64,
}

impl ModelState {
    pub fn new(piece_count: usize) -> Self { Self { skill_overrides: vec![None; piece_count], generation: 0 } }
    pub fn skill(&self, piece: &GearPiece) -> SkillType { self.skill_overrides[piece.id.index()].unwrap_or(piece.skill_type) }
    pub fn set_skill(&mut self, id: GearId, skill: SkillType) {
        let current = &mut self.skill_overrides[id.index()];
        if *current != Some(skill) { *current = Some(skill); self.generation = self.generation.wrapping_add(1); }
    }
    pub fn override_for(&self, id: GearId) -> Option<SkillType> { self.skill_overrides[id.index()] }
    pub fn generation(&self) -> u64 { self.generation }
    pub fn reset(&mut self, piece_count: usize) {
        if self.skill_overrides.len() != piece_count || self.skill_overrides.iter().any(Option::is_some) {
            self.generation = self.generation.wrapping_add(1);
        }
        self.skill_overrides.clear();
        self.skill_overrides.resize(piece_count, None);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActiveEffects {
    pub buckets: Vec<EffectBucket>,
    pub enemy_debuffs: Stats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectBucket {
    pub name: Arc<str>,
    pub stats: Stats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
    pub name: Arc<str>,
    pub location: Arc<str>,
    pub stats: Stats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnemyInput {
    pub name: Arc<str>,
    pub location: Arc<str>,
    pub stats: Stats,
}

mod player;
pub use player::{build_player, get_skill_accuracy};

/// Primary: action rolls, resist, optimizer choice/shuffle. Pdif: pDIF kernel rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomStream { Primary, Pdif }

pub trait RandomSource: Clone + Send + 'static {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64;
    fn bounded(&mut self, upper_exclusive: usize) -> usize;
    fn reseed(&mut self, seed: u64);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AttackKind { Main, Offhand, Ranged, Kick, Daken, Zanshin, Magical, TpRetention }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AttackProc { Normal, QuadAttack, TripleAttack, DoubleAttack, Occasionally(u8), FollowUp, Fotia, ConserveTp }

#[derive(Clone, Copy, Debug, Serialize)]
pub struct HitEvent {
    pub kind: AttackKind,
    pub proc: AttackProc,
    pub attempt: u8,
    pub landed: bool,
    pub critical: bool,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub physical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub magical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_return: f64,
}

/// A display-only equation captured from an existing calculation.
/// `equation` holds an optional named formula; `{operands}` in `formula`
/// are substituted by the report renderer. Random terms remain placeholders.
#[derive(Clone, Debug, PartialEq)]
pub struct CalculationStep {
    pub label: &'static str,
    pub equation: Option<&'static str>,
    pub formula: &'static str,
    pub values: Vec<(&'static str, f64)>,
}

pub struct EvaluationContext<R: RandomSource = crate::rng::RandomSources> {
    pub catalog: Arc<crate::catalog::Catalog>,
    pub state: ModelState,
    pub random: R,
    pub scratch: EvaluationScratch,
    pub player_stages: Option<Vec<PlayerStageSnapshot>>,
    pub action_trace: Option<Vec<HitEvent>>,
    pub calculation_trace: Option<Vec<CalculationStep>>,
}

impl<R: RandomSource> EvaluationContext<R> {
    pub fn new(catalog: Arc<crate::catalog::Catalog>, random: R) -> Self {
        Self { state: ModelState::new(catalog.pieces.len()), catalog, random,
            scratch: EvaluationScratch::default(), player_stages: None, action_trace: None,
            calculation_trace: None }
    }
    pub fn record_hit(&mut self, event: HitEvent) {
        if let Some(trace) = &mut self.action_trace { trace.push(event); }
    }
    pub fn record_calculation(&mut self, label: &'static str, formula: &'static str,
        values: &[(&'static str, f64)]) {
        if let Some(trace) = &mut self.calculation_trace {
            trace.push(CalculationStep { label, equation: None, formula, values: values.to_vec() });
        }
    }
    pub fn record_equation(&mut self, label: &'static str, equation: &'static str,
        formula: &'static str, values: &[(&'static str, f64)]) {
        if let Some(trace) = &mut self.calculation_trace {
            trace.push(CalculationStep { label, equation: Some(equation), formula, values: values.to_vec() });
        }
    }
}

#[derive(Default)]
pub struct EvaluationScratch {
    pub oa_main: [f64; 9],
    pub oa_sub: [f64; 9],
    pub hit_rates: [[f64; 2]; 2],
}

named_enum! {
    pub enum PlayerStage {
        Base => "after_base", Gear => "after_gear", Buffs => "after_buffs",
        FinalizeBeforeSort => "after_finalize_before_sort", Final => "final"
    }
}

#[derive(Clone, Debug)]
pub struct PlayerStageSnapshot {
    pub stage: PlayerStage,
    pub stats: Stats,
    pub wsc: Vec<WscContribution>,
    pub wyvern_bonus_attack: bool,
    pub skills: [SkillType; Slot::COUNT],
}

#[derive(Clone, Debug)]
pub struct Player {
    pub main_job: Job,
    pub sub_job: Option<Job>,
    pub master_level: i32,
    pub main_job_level: i32,
    pub sub_job_level: i32,
    pub gearset: Gearset,
    pub stats: Stats,
    pub wsc: Vec<WscContribution>,
    pub wyvern_bonus_attack: bool,
    pub abilities: Abilities,
}

impl Player {
    pub fn new(gearset: Gearset) -> Self {
        Self { main_job: Job::War, sub_job: None, master_level: 0, main_job_level: 99, sub_job_level: 49,
            gearset, stats: Stats::default(), wsc: Vec::with_capacity(Slot::COUNT), wyvern_bonus_attack: false,
            abilities: Abilities::default() }
    }
}
