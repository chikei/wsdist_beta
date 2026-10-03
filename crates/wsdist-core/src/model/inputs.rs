use super::*;
use serde::ser::SerializeMap;
use crate::weaponskills::WeaponSkillId;

named_enum! {
    pub enum Ability {
        DamageLimitDps => "99999",
        Aggressor => "Aggressor",
        Angon => "Angon",
        ArmorBreak => "Armor Break",
        Barrage => "Barrage",
        Berserk => "Berserk",
        BloodRage => "Blood Rage",
        BoxStep => "Box Step",
        BoxStepSub => "Box Step (sub)",
        BuildingFlourish => "Building Flourish",
        Chainspell => "Chainspell",
        ClimacticFlourish => "Climactic Flourish",
        ClosedPosition => "Closed Position",
        Composure => "Composure",
        Conspirator => "Conspirator",
        CorrosiveOoze => "Corrosive Ooze",
        CrimsonHowl => "Crimson Howl",
        CrystalBlessing => "Crystal Blessing",
        DistractIii => "Distract III",
        DivineEmblem => "Divine Emblem",
        DoubleShot => "Double Shot",
        Ebullience => "Ebullience",
        EnSpell => "EnSpell",
        EndarkIi => "Endark II",
        EnlightIi => "Enlight II",
        Enlightenment => "Enlightenment",
        Focus => "Focus",
        Footwork => "Footwork",
        FrenziedRage => "Frenzied Rage",
        Futae => "Futae",
        Hasso => "Hasso",
        HasteSamba => "Haste Samba",
        HasteSambaSub => "Haste Samba (sub)",
        HoverShot => "Hover Shot",
        IfritsFavor => "Ifrit's Favor",
        Impetus => "Impetus",
        Innin => "Innin",
        Klimaform => "Klimaform",
        LastResort => "Last Resort",
        MagicBurst => "Magic Burst",
        Manafont => "Manafont",
        Manawell => "Manawell",
        MightyGuard => "Mighty Guard",
        MightyStrikes => "Mighty Strikes",
        NaturesMeditation => "Nature's Meditation",
        Overwhelm => "Overwhelm",
        Rage => "Rage",
        RamuhsFavor => "Ramuh's Favor",
        SaberDance => "Saber Dance",
        Sange => "Sange",
        Sharpshot => "Sharpshot",
        ShivasFavor => "Shiva's Favor",
        SneakAttack => "Sneak Attack",
        StrikingFlourish => "Striking Flourish",
        SwoopingFrenzy => "Swooping Frenzy",
        Swordplay => "Swordplay",
        Temper => "Temper",
        TemperIi => "Temper II",
        TernaryFlourish => "Ternary Flourish",
        TheurgicFocus => "Theurgic Focus",
        TrickAttack => "Trick Attack",
        TripleShot => "Triple Shot",
        TrueShot => "True Shot",
        VelocityShot => "Velocity Shot",
        VerboseDps => "Verbose DPS",
        VerboseSwaps => "Verbose Swaps",
        VeryVerboseDps => "Very Verbose DPS",
        Warcry => "Warcry",
        WarcrySub => "Warcry (sub)",
    }
}

named_enum! {
    pub enum Storm {
        None => "None", Aurora => "Aurorastorm", AuroraII => "Aurorastorm II",
        Void => "Voidstorm", VoidII => "Voidstorm II", Fire => "Firestorm", FireII => "Firestorm II",
        Sand => "Sandstorm", SandII => "Sandstorm II", Rain => "Rainstorm", RainII => "Rainstorm II",
        Wind => "Windstorm", WindII => "Windstorm II", Hail => "Hailstorm", HailII => "Hailstorm II",
        Thunder => "Thunderstorm", ThunderII => "Thunderstorm II"
    }
}

pub const RESIST_RANKS: [&str; 15] = ["150%", "130%", "115%", "100%", "85%", "70%", "60%", "50%", "40%", "30%", "25%", "20%", "15%", "10%", "5%"];

#[derive(Clone, Debug)]
enum AftermathSource {
    Number(serde_json::Number),
    Bool(bool),
    Nonfinite(f64),
}

impl Serialize for AftermathSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Number(value) => value.serialize(serializer),
            Self::Bool(value) => value.serialize(serializer),
            Self::Nonfinite(value) => LegacyF64(*value).serialize(serializer),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Abilities {
    flags: [u64; 2],
    presence: [u64; 2],
    aftermath: f64,
    aftermath_source: AftermathSource,
    pub enhancing_skill: f64,
    pub storm_name: Option<Arc<str>>,
    pub storm: Option<Storm>,
    pub storm_contains_ii: bool,
    pub resist_rank_name: Arc<str>,
    pub resist_rank_index: Option<u8>,
}

impl Default for Abilities {
    fn default() -> Self {
        Self { flags: [0; 2], presence: [0; 2], aftermath: 0.0, aftermath_source: AftermathSource::Number(0.into()), enhancing_skill: 0.0,
            storm_name: None, storm: None, storm_contains_ii: false,
            resist_rank_name: Arc::from("100%"), resist_rank_index: Some(3) }
    }
}

impl Abilities {
    pub fn aftermath(&self) -> f64 { self.aftermath }
    pub fn aftermath_is_integer(&self) -> bool {
        match &self.aftermath_source {
            AftermathSource::Number(value) => value.is_i64() || value.is_u64(),
            AftermathSource::Bool(_) => true,
            AftermathSource::Nonfinite(_) => false,
        }
    }
    pub fn set_aftermath_integer(&mut self, level: i64) {
        self.aftermath = level as f64;
        self.aftermath_source = AftermathSource::Number(level.into());
    }
    pub fn set_aftermath_float(&mut self, level: f64) {
        self.aftermath = level;
        self.aftermath_source = serde_json::Number::from_f64(level)
            .map(AftermathSource::Number).unwrap_or(AftermathSource::Nonfinite(level));
    }
    #[inline]
    pub fn enabled(&self, ability: Ability) -> bool { self.flags[ability as usize / 64] & (1 << (ability as usize % 64)) != 0 }
    pub fn present(&self, ability: Ability) -> bool { self.presence[ability as usize / 64] & (1 << (ability as usize % 64)) != 0 }
    pub fn set(&mut self, ability: Ability, enabled: bool) {
        let word = ability as usize / 64;
        let mask = 1 << (ability as usize % 64);
        self.presence[word] |= mask;
        if enabled { self.flags[word] |= mask; } else { self.flags[word] &= !mask; }
    }
    pub fn set_storm(&mut self, name: Option<Arc<str>>) {
        self.storm = name.as_deref().and_then(Storm::from_name);
        self.storm_contains_ii = name.as_deref().is_some_and(|name| name.contains("II"));
        self.storm_name = name;
    }
    pub fn set_resist_rank(&mut self, name: Arc<str>) {
        self.resist_rank_index = RESIST_RANKS.iter().position(|rank| *rank == name.as_ref()).map(|index| index as u8);
        self.resist_rank_name = name;
    }
    pub fn from_legacy(values: &serde_json::Map<String, serde_json::Value>) -> Result<Self, CalcError> {
        let mut result = Self::default();
        for (name, value) in values {
            if let Some(ability) = Ability::from_name(name) { result.set(ability, python_truth(value)); }
            else {
                match name.as_str() {
                    "Aftermath" => {
                        result.aftermath = legacy_number(value, "Aftermath")?;
                        result.aftermath_source = match value {
                            serde_json::Value::Number(value) => AftermathSource::Number(value.clone()),
                            serde_json::Value::Bool(value) => AftermathSource::Bool(*value),
                            _ => AftermathSource::Nonfinite(result.aftermath),
                        };
                    }
                    "Enhancing Skill" => result.enhancing_skill = legacy_number(value, "Enhancing Skill")?,
                    "Storm spell" => {
                        if value == &serde_json::Value::Bool(false) || value.is_null() { result.set_storm(None); }
                        else { result.set_storm(Some(Arc::from(value.as_str().ok_or_else(|| CalcError::legacy("abilities", "Storm spell", "TypeError", "Storm spell must be text or false"))?))); }
                    }
                    "Enemy Resist Rank" => result.set_resist_rank(Arc::from(value.as_str().ok_or_else(|| CalcError::legacy("abilities", "Enemy Resist Rank", "TypeError", "Enemy Resist Rank must be text"))?)),
                    _ => {}
                }
            }
        }
        Ok(result)
    }
}

fn legacy_number(value: &serde_json::Value, field: &'static str) -> Result<f64, CalcError> {
    if let serde_json::Value::Bool(value) = value { return Ok(if *value { 1.0 } else { 0.0 }); }
    LegacyF64::deserialize(value).map(|number| number.0)
        .map_err(|_| CalcError::legacy("abilities", field, "TypeError", format!("{field} must be numeric")))
}

pub fn python_truth(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(value) => !value.is_empty(),
        serde_json::Value::Object(value) => !value.is_empty(),
    }
}

impl Serialize for Abilities {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for ability in Ability::ALL { if self.present(*ability) { map.serialize_entry(ability.name(), &self.enabled(*ability))?; } }
        map.serialize_entry("Aftermath", &self.aftermath_source)?;
        map.serialize_entry("Enhancing Skill", &LegacyF64(self.enhancing_skill))?;
        if let Some(name) = &self.storm_name { map.serialize_entry("Storm spell", name)?; }
        map.serialize_entry("Enemy Resist Rank", &self.resist_rank_name)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Abilities {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        Self::from_legacy(&values).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInput {
    pub main_job: Job,
    pub sub_job: Option<Job>,
    pub master_level: i32,
    pub gearset: Gearset,
    pub effects: Arc<ActiveEffects>,
    pub abilities: Abilities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollInput { pub name: Arc<str>, pub potency: Arc<str> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuffInputs {
    pub brd_enabled: bool,
    pub songs: [Arc<str>; 5],
    pub song_bonus: Arc<str>,
    pub soul_voice: bool,
    pub marcato: bool,
    pub cor_enabled: bool,
    pub rolls: [RollInput; 4],
    pub roll_bonus: Arc<str>,
    pub crooked_cards: bool,
    pub cor_job_bonus: bool,
    pub light_shot: bool,
    pub geo_enabled: bool,
    pub bubbles: [Arc<str>; 3],
    pub bubble_bonus: Arc<str>,
    pub bubble_potency: Arc<str>,
    pub bolster: bool,
    pub blaze_of_glory: bool,
    pub whm_enabled: bool,
    pub whm_selections: [Arc<str>; 4],
    pub shell_v: bool,
    pub food_name: Arc<str>,
    pub miscellaneous: Abilities,
}

impl Default for BuffInputs {
    fn default() -> Self {
        let none: Arc<str> = Arc::from("None");
        Self { brd_enabled: false, songs: std::array::from_fn(|_| none.clone()), song_bonus: Arc::from("Songs +0"),
            soul_voice: false, marcato: false, cor_enabled: false,
            rolls: std::array::from_fn(|_| RollInput { name: none.clone(), potency: Arc::from("XI") }),
            roll_bonus: Arc::from("Rolls +0"), crooked_cards: false, cor_job_bonus: false, light_shot: false,
            geo_enabled: false, bubbles: std::array::from_fn(|_| none.clone()), bubble_bonus: Arc::from("Geomancy +0"),
            bubble_potency: Arc::from("100"), bolster: false, blaze_of_glory: false, whm_enabled: false,
            whm_selections: std::array::from_fn(|_| none.clone()), shell_v: false, food_name: none, miscellaneous: Abilities::default() }
    }
}

named_enum! { pub enum WsType { Melee => "melee", Ranged => "ranged", None => "None" } }
named_enum! { pub enum SpellType { DivineMagic => "Divine Magic", ElementalMagic => "Elemental Magic", Ninjutsu => "Ninjutsu", QuickDraw => "Quick Draw", RangedAttack => "Ranged Attack" } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellId(pub(crate) u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricKind { Damage, TpReturn, Dps, DamageThenTp, TpThenDamage, TimeToWs, MagicAccuracy, Fallback }

#[derive(Clone, Debug)]
pub struct Metric { pub name: Arc<str>, pub kind: MetricKind }

impl Metric {
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        let name = name.into();
        let kind = match name.as_ref() {
            "Damage dealt" => MetricKind::Damage, "TP return" => MetricKind::TpReturn, "DPS" => MetricKind::Dps,
            "Damage > TP" => MetricKind::DamageThenTp, "TP > Damage" => MetricKind::TpThenDamage,
            "Time to WS" => MetricKind::TimeToWs, "Magic accuracy" => MetricKind::MagicAccuracy,
            _ => MetricKind::Fallback,
        };
        Self { name, kind }
    }
}

#[derive(Clone, Debug)]
pub struct WsRequest { pub skill: WeaponSkillId, pub ws_type: WsType, pub tp: f64, pub metric: Metric }
#[derive(Clone, Debug)]
pub struct SpellRequest { pub spell: SpellId, pub spell_type: SpellType, pub metric: Metric }
#[derive(Clone, Debug)]
pub enum ActionRequest { Round { metric: Metric, starting_tp: f64, threshold: f64 }, Ws(WsRequest), Spell(SpellRequest) }

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ActionResult {
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_return: f64,
    #[serde(serialize_with = "serialize_optional_legacy_f64")]
    pub seconds_per_round: Option<f64>,
    #[serde(serialize_with = "serialize_optional_legacy_f64")]
    pub time_to_ws: Option<f64>,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub metric: f64,
    pub invert: i8,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub physical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub magical_damage: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RoundSample {
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_return: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub physical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub magical_damage: f64,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WsSample {
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_return: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub physical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub magical_damage: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizeOptions {
    pub iterations: i64, pub max_swap_slots: i64, pub restart_count: i64,
    pub seed: Option<u32>, pub dt_step: f64, pub workers: i64,
}

impl Default for OptimizeOptions {
    fn default() -> Self { Self { iterations: 10, max_swap_slots: 2, restart_count: 1, seed: None, dt_step: 1.0, workers: 0 } }
}

pub type CandidatePools = [Vec<GearId>; Slot::COUNT];

#[derive(Clone, Debug)]
pub struct OptimizeRequest {
    pub player: PlayerInput, pub enemy: Enemy, pub action: ActionRequest, pub candidates: CandidatePools,
    pub ws_name: Arc<str>, pub spell_name: Arc<str>,
    pub min_pdt: f64, pub min_mdt: f64, pub options: OptimizeOptions,
    pub similar_results: bool, pub similar_percent: f64, pub verbose_swaps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum OptimizeTermination { Converged, IterationLimit, UnchangedDtStages, NoWinner, UnsatisfiedDt }

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: usize, pub pass: usize, pub scored: usize,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub best_metric: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub pdt: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub mdt: f64,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AcceptedCandidate { pub gearset: Gearset, pub metric: f64, pub legacy_output: Vec<f64>, pub pdt: f64, pub mdt: f64 }
#[derive(Clone, Debug)]
pub struct SimilarSwap { pub slot: Slot, pub gear: GearId, pub metric: f64, pub difference_percent: f64 }
#[derive(Clone, Debug)]
pub struct OptimizeResult {
    pub gearset: Gearset, pub player: Option<Player>, pub metric: f64, pub legacy_output: Vec<f64>,
    pub pdt: f64, pub mdt: f64, pub feasible: bool, pub termination: OptimizeTermination,
    pub accepted: Vec<AcceptedCandidate>, pub similar: Vec<SimilarSwap>,
    pub stages: usize, pub passes: usize, pub scored: usize, pub restarts: usize, pub invert: i8,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DpsRequest { pub tp_player: PlayerInput, pub ws_player: PlayerInput, pub enemy: Enemy, pub ws: WsRequest, pub threshold: f64, pub trace: bool }
#[derive(Clone, Debug)]
pub struct DistributionRequest { pub player: PlayerInput, pub enemy: Enemy, pub ws: WsRequest }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SimulationEventKind { Round, WeaponSkill }
#[derive(Clone, Debug, Serialize)]
pub struct SimulationEvent {
    pub kind: SimulationEventKind,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub time: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_before: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_after: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub physical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub magical_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_return: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub regain_tp: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub cumulative_damage: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct DpsResult {
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub final_time: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub final_tp: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub tp_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub ws_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub seconds_per_round: f64,
    pub round_count: usize,
    pub ws_count: usize,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub average_tp_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub average_physical_round_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub average_magical_round_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub average_ws_damage: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub average_pre_bonus_tp: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub ws_tp_bonus: f64,
    #[serde(serialize_with = "serialize_legacy_series")]
    pub total_series: Vec<[f64; 2]>,
    #[serde(serialize_with = "serialize_legacy_series")]
    pub tp_series: Vec<[f64; 2]>,
    #[serde(serialize_with = "serialize_legacy_series")]
    pub ws_series: Vec<[f64; 2]>,
    pub events: Vec<SimulationEvent>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Histogram {
    pub counts: Vec<u64>,
    #[serde(serialize_with = "serialize_legacy_f64_slice")]
    pub density: Vec<f64>,
    #[serde(serialize_with = "serialize_legacy_f64_slice")]
    pub edges: Vec<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct DistributionResult {
    #[serde(serialize_with = "serialize_legacy_f64_slice")]
    pub damage: Vec<f64>,
    #[serde(serialize_with = "serialize_legacy_f64_slice")]
    pub tp: Vec<f64>,
    pub histogram: Histogram,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub minimum: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub mean: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub median: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub maximum: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub title_mean: f64,
    #[serde(serialize_with = "serialize_legacy_f64")]
    pub title_median: f64,
}
