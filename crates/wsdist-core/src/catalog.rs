use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deser_core::de::{Deserialize as _, DeserializeDriver, Sink, SinkHandle};
use deser_core::{Atom, Error as DecodeError, ErrorKind as DecodeErrorKind, State};
use serde_json::{Map, Value};

use crate::model::{
    CalcError, EnemyInput, GearId, GearPiece, Job, PieceKind, SkillType, Slot,
    StatContribution, WscContribution,
};
use crate::stats::{Stat, Stats};

#[derive(Debug)]
pub struct Catalog {
    pub(crate) pieces: Box<[GearPiece]>,
    slots: [Box<[GearId]>; Slot::COUNT],
    foods: Box<[GearId]>,
    name2_ids: HashMap<Arc<str>, GearId>,
    food_names: HashMap<Arc<str>, GearId>,
    enemies: Box<[EnemyRecord]>,
    enemy_menu: Box<[EnemyId]>,
    enemy_names: HashMap<Arc<str>, usize>,
    buffs: BuffCatalog,
    attachments: Box<[Value]>,
    attachment_names: HashMap<Arc<str>, usize>,
    aliases: Box<[ItemAlias]>,
    full_name_aliases: HashMap<String, usize>,
    abbreviated_aliases: HashMap<String, usize>,
    pub empty: GearId,
}

impl Catalog {
    pub fn load(root: &Path) -> Result<Self, CalcError> {
        let gear = load_gear(&root.join("data/gear"))?;
        let enemies = load_enemies(&root.join("data/enemies.json5"))?;
        let buffs = load_buffs(&root.join("data/buffs.json5"))?;
        let attachments = load_attachments(&root.join("data/pup-attachments.json5"))?;
        let aliases = load_aliases(&root.join("item_list.csv"))?;
        Ok(Self {
            pieces: gear.pieces,
            slots: gear.slots,
            foods: gear.foods,
            name2_ids: gear.name2_ids,
            food_names: gear.food_names,
            enemies: enemies.records,
            enemy_menu: enemies.menu,
            enemy_names: enemies.names,
            buffs,
            attachments: attachments.records,
            attachment_names: attachments.names,
            aliases: aliases.records,
            full_name_aliases: aliases.full_names,
            abbreviated_aliases: aliases.abbreviations,
            empty: gear.empty,
        })
    }

    pub fn pieces(&self) -> &[GearPiece] { &self.pieces }
    pub fn piece(&self, id: GearId) -> &GearPiece { &self.pieces[id.index()] }
    pub fn slot(&self, slot: Slot) -> &[GearId] { &self.slots[slot.index()] }
    pub fn foods(&self) -> &[GearId] { &self.foods }
    pub fn by_name2(&self, name: &str) -> Option<GearId> { self.name2_ids.get(name).copied() }
    pub fn food_by_name(&self, name: &str) -> Option<GearId> { self.food_names.get(name).copied() }
    pub fn enemies(&self) -> &[EnemyRecord] { &self.enemies }
    pub fn enemy(&self, id: EnemyId) -> &EnemyRecord { &self.enemies[id.index()] }
    pub fn enemy_menu(&self) -> &[EnemyId] { &self.enemy_menu }
    pub fn enemy_by_name(&self, name: &str) -> Option<EnemyId> {
        self.enemy_names.get(name).map(|index| self.enemy_menu[*index])
    }
    pub fn buffs(&self) -> &BuffCatalog { &self.buffs }
    pub fn attachments(&self) -> &[Value] { &self.attachments }
    pub fn attachment_by_name(&self, name: &str) -> Option<&Value> {
        self.attachment_names.get(name).map(|index| &self.attachments[*index])
    }
    pub fn aliases(&self) -> &[ItemAlias] { &self.aliases }
    pub fn alias_by_full_name(&self, name: &str) -> Option<&ItemAlias> {
        self.full_name_aliases.get(&name.to_lowercase()).map(|index| &self.aliases[*index])
    }
    pub fn alias_by_abbreviation(&self, name: &str) -> Option<&ItemAlias> {
        self.abbreviated_aliases.get(&name.to_lowercase()).map(|index| &self.aliases[*index])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EnemyId(usize);

impl EnemyId {
    pub const fn index(self) -> usize { self.0 }
}

#[derive(Debug)]
pub struct EnemyRecord {
    pub name: Arc<str>,
    pub location: Arc<str>,
    pub level: f64,
    pub stats: Box<[StatContribution]>,
}

impl EnemyRecord {
    pub fn stat(&self, stat: Stat) -> Option<f64> {
        self.stats.iter().find(|entry| entry.stat == stat).map(|entry| entry.value)
    }

    pub fn to_input(&self) -> EnemyInput {
        let mut stats = Stats::default();
        for entry in &self.stats { stats.set(entry.stat, entry.value); }
        EnemyInput { name: Arc::clone(&self.name), location: Arc::clone(&self.location), stats }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BuffTableKind {
    Cor, Brd, Geo, Whm, CorDebuffs, GeoDebuffs, WhmDebuffs, MiscDebuffs,
}

impl BuffTableKind {
    pub const ALL: [Self; 8] = [
        Self::Cor, Self::Brd, Self::Geo, Self::Whm, Self::CorDebuffs,
        Self::GeoDebuffs, Self::WhmDebuffs, Self::MiscDebuffs,
    ];
    pub const fn index(self) -> usize { self as usize }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cor => "cor", Self::Brd => "brd", Self::Geo => "geo", Self::Whm => "whm",
            Self::CorDebuffs => "cor_debuffs", Self::GeoDebuffs => "geo_debuffs",
            Self::WhmDebuffs => "whm_debuffs", Self::MiscDebuffs => "misc_debuffs",
        }
    }
}

pub const ROLL_NUMERALS: [&str; 11] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scaling {
    pub base: f64,
    pub per_point: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorRoll {
    pub potency: [f64; 11],
    pub per_roll_bonus: f64,
    pub job_bonus: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BuffValue {
    Constant(f64),
    Scaling(Scaling),
    CorRoll(CorRoll),
}

#[derive(Debug)]
pub struct BuffDefinition {
    pub name: Arc<str>,
    pub stats: Box<[BuffStat]>,
    pub level_requirement: Option<f64>,
    pub job_requirement: Option<Box<[Job]>>,
    pub song_bonus_cap: Option<f64>,
}

impl BuffDefinition {
    pub fn stat(&self, stat: Stat) -> Option<BuffValue> {
        self.stats.iter().find(|entry| entry.stat == stat).map(|entry| entry.value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuffStat {
    pub stat: Stat,
    pub value: BuffValue,
}

#[derive(Debug)]
pub struct BuffTable {
    entries: Box<[BuffDefinition]>,
    names: HashMap<Arc<str>, usize>,
}

impl BuffTable {
    pub fn entries(&self) -> &[BuffDefinition] { &self.entries }
    pub fn index_of(&self, name: &str) -> Option<usize> { self.names.get(name).copied() }
    pub fn get(&self, name: &str) -> Option<&BuffDefinition> {
        self.index_of(name).map(|index| &self.entries[index])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLimit {
    pub song_index: usize,
    pub cap: f64,
}

#[derive(Debug)]
pub struct BuffCatalog {
    tables: [BuffTable; 8],
    song_limits: Box<[SongLimit]>,
}

impl BuffCatalog {
    pub fn table(&self, kind: BuffTableKind) -> &BuffTable { &self.tables[kind.index()] }
    pub fn song_limits(&self) -> &[SongLimit] { &self.song_limits }
}

#[derive(Debug)]
pub struct ItemAlias {
    // Keep the CSV spelling (including leading zeroes) for icon filenames.
    pub id: Arc<str>,
    pub name: Arc<str>,
    pub name2: Arc<str>,
}

// Build the final ordered Serde value directly. Original numeric spans retain
// JSON's -0 and wide-integer semantics; nonfinite JSON5 values are never null.
struct CatalogSink<'out, 'source> {
    out: &'out mut Value,
    source: &'source str,
    key: Option<String>,
}

impl CatalogSink<'_, '_> {
    fn native_number(atom: Atom<'_>, negative: bool) -> Result<serde_json::Number, DecodeError> {
        match atom {
            Atom::U64(0) | Atom::I64(0) if negative => {
                Ok(serde_json::Number::from_f64(-0.0).expect("negative zero is finite"))
            }
            Atom::U64(value) => Ok(value.into()),
            Atom::I64(value) => Ok(value.into()),
            Atom::F64(value) => serde_json::Number::from_f64(value)
                .ok_or_else(|| DecodeError::new(DecodeErrorKind::OutOfRange, "expected a finite number")),
            Atom::Ext(value) => {
                let value = if let Some(value) = value.downcast_ref::<u128>() {
                    *value as f64
                } else if let Some(value) = value.downcast_ref::<i128>() {
                    *value as f64
                } else {
                    return Err(DecodeError::new(DecodeErrorKind::InvalidType, "expected a number"));
                };
                Ok(serde_json::Number::from_f64(value).expect("128-bit integer fits in finite f64"))
            }
            _ => Err(DecodeError::new(DecodeErrorKind::InvalidType, "expected a number")),
        }
    }
}

impl<'de> Sink<'de> for CatalogSink<'_, '_> {
    fn atom(&mut self, atom: Atom<'_>, state: &mut State) -> Result<(), DecodeError> {
        *self.out = match atom {
            Atom::Null => Value::Null,
            Atom::Bool(value) => Value::Bool(value),
            Atom::Str(value) | Atom::Lexical(value) => Value::String(value.into_owned()),
            number => {
                let literal = state.input_range().and_then(|range| self.source.get(range))
                    .ok_or_else(|| DecodeError::new(DecodeErrorKind::Custom, "missing numeric input range"))?;
                Value::Number(Self::native_number(number, literal.starts_with('-'))?)
            }
        };
        Ok(())
    }

    fn map(&mut self, _state: &mut State) -> Result<(), DecodeError> {
        *self.out = Value::Object(Map::new());
        Ok(())
    }

    fn seq(&mut self, _state: &mut State) -> Result<(), DecodeError> {
        *self.out = Value::Array(Vec::new());
        Ok(())
    }

    fn next_key(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, DecodeError> {
        Ok(String::deserialize_into(&mut self.key, state))
    }

    fn next_value(&mut self, state: &mut State) -> Result<SinkHandle<'_, 'de>, DecodeError> {
        let out = match self.out {
            Value::Array(values) => {
                values.push(Value::Null);
                values.last_mut().expect("just appended array value")
            }
            Value::Object(values) => {
                let key = self.key.take().expect("map value follows its key");
                values.entry(key).or_insert(Value::Null)
            }
            _ => unreachable!("only containers request a child value"),
        };
        Ok(SinkHandle::arena(CatalogSink { out, source: self.source, key: None }, state))
    }
}

struct DataFile {
    path: PathBuf,
    value: Value,
}

impl DataFile {
    fn open(path: &Path) -> Result<Self, CalcError> {
        let source = fs::read_to_string(path).map_err(|error| CalcError::data(path, "$", error.to_string()))?;
        let mut value = Value::Null;
        let mut sink = CatalogSink { out: &mut value, source: &source, key: None };
        {
            let mut driver = DeserializeDriver::from_fn(|_| SinkHandle::to(&mut sink));
            let config = deser_json5::DeserializerConfig::builder().exact_numbers(false).build();
            let mut deserializer = deser_json5::Deserializer::from_str_with_config(&source, config);
            deserializer.drive(&mut driver).map_err(|error| {
                let at = error.line().zip(error.column()).map_or_else(|| "$".to_owned(), |(line, column)| {
                    format!("$ (line {line}, column {column})")
                });
                CalcError::data(path, at, error.to_string())
            })?;
        }
        Ok(Self { path: path.to_owned(), value })
    }

    fn error(&self, at: &str, message: impl Into<String>) -> CalcError {
        CalcError::data(&self.path, at, message)
    }

    fn object<'a>(&self, value: &'a Value, at: &str) -> Result<&'a Map<String, Value>, CalcError> {
        value.as_object().ok_or_else(|| self.error(at, "expected an object"))
    }

    fn array<'a>(&self, value: &'a Value, at: &str) -> Result<&'a [Value], CalcError> {
        value.as_array().map(Vec::as_slice).ok_or_else(|| self.error(at, "expected an array"))
    }

    fn string<'a>(&self, value: &'a Value, at: &str) -> Result<&'a str, CalcError> {
        value.as_str().ok_or_else(|| self.error(at, "expected a string"))
    }

    fn number(&self, value: &Value, at: &str) -> Result<f64, CalcError> {
        value.as_f64().filter(|number| number.is_finite())
            .ok_or_else(|| self.error(at, "expected a finite number"))
    }

    fn u32(&self, value: &Value, at: &str) -> Result<u32, CalcError> {
        value.as_u64().and_then(|number| u32::try_from(number).ok())
            .ok_or_else(|| self.error(at, "expected an integer in 0..=4294967295"))
    }

    fn required<'a>(&self, object: &'a Map<String, Value>, key: &str, at: &str) -> Result<&'a Value, CalcError> {
        object.get(key).ok_or_else(|| self.error(&field_path(at, key), "missing required field"))
    }

    fn fields(&self, object: &Map<String, Value>, allowed: &[&str], at: &str) -> Result<(), CalcError> {
        for name in object.keys() {
            if !allowed.contains(&name.as_str()) {
                return Err(self.error(&field_path(at, name), format!("unknown field {name:?}")));
            }
        }
        Ok(())
    }

    fn root(&self, allowed: &[&str], expected_version: u64) -> Result<&Map<String, Value>, CalcError> {
        let root = self.object(&self.value, "$")?;
        self.fields(root, allowed, "$")?;
        let version = self.required(root, "schema_version", "$")?;
        if version.as_u64() != Some(expected_version) {
            return Err(self.error("$.schema_version", format!("expected schema version {expected_version}")));
        }
        Ok(root)
    }

    fn jobs(&self, value: &Value, at: &str) -> Result<Box<[Job]>, CalcError> {
        let source = self.array(value, at)?;
        let mut jobs = Vec::with_capacity(source.len());
        for (index, value) in source.iter().enumerate() {
            let at = format!("{at}[{index}]");
            let name = self.string(value, &at)?;
            jobs.push(Job::from_name(name).ok_or_else(|| self.error(&at, format!("unknown job {name:?}")))?);
        }
        Ok(jobs.into_boxed_slice())
    }

    fn stat(&self, name: &str, at: &str) -> Result<Stat, CalcError> {
        Stat::from_name(name).ok_or_else(|| self.error(at, format!("unknown stat {name:?}")))
    }

    fn contribution(&self, stat: Stat, value: &Value, at: &str) -> Result<StatContribution, CalcError> {
        let numeric = self.number(value, at)?;
        let literal = value.as_number().expect("validated numeric value").clone();
        Ok(StatContribution { stat, value: numeric, literal })
    }
}

fn field_path(parent: &str, name: &str) -> String {
    let mut bytes = name.bytes();
    let identifier = bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if identifier { format!("{parent}.{name}") }
    else { format!("{parent}[{}]", Value::String(name.to_owned())) }
}

struct GearResources {
    pieces: Box<[GearPiece]>,
    slots: [Box<[GearId]>; Slot::COUNT],
    foods: Box<[GearId]>,
    name2_ids: HashMap<Arc<str>, GearId>,
    food_names: HashMap<Arc<str>, GearId>,
    empty: GearId,
}

#[derive(Clone)]
struct GearOrigin {
    path: Arc<Path>,
    index: usize,
    label_field: &'static str,
}

impl GearOrigin {
    fn label_location(&self) -> String {
        format!("{}: $.pieces[{}].{}", self.path.display(), self.index, self.label_field)
    }
}

fn gear_fragments(directory: &Path) -> Result<Vec<PathBuf>, CalcError> {
    fn collect(directory: &Path, root: &Path, fragments: &mut Vec<(String, PathBuf)>) -> Result<(), CalcError> {
        let entries = fs::read_dir(directory).map_err(|error| CalcError::data(directory, "$", error.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|error| CalcError::data(directory, "$", error.to_string()))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| CalcError::data(&path, "$", error.to_string()))?;
            if kind.is_dir() {
                collect(&path, root, fragments)?;
            } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "json5") {
                let relative = path.strip_prefix(root).expect("fragment is below gear directory");
                let relative = relative.to_str().ok_or_else(|| CalcError::data(&path, "$", "fragment path must be valid Unicode"))?;
                fragments.push((relative.replace(std::path::MAIN_SEPARATOR, "/"), path));
            }
        }
        Ok(())
    }

    let metadata = fs::symlink_metadata(directory).map_err(|error| CalcError::data(directory, "$", error.to_string()))?;
    if !metadata.is_dir() {
        return Err(CalcError::data(directory, "$", "expected a gear fragment directory; symlinks are not followed"));
    }
    let mut fragments = Vec::new();
    collect(directory, directory, &mut fragments)?;
    fragments.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    if fragments.is_empty() {
        return Err(CalcError::data(directory, "$", "no .json5 gear fragments found"));
    }
    let mut case_paths = HashMap::with_capacity(fragments.len());
    for (relative, path) in &fragments {
        if let Some(first) = case_paths.insert(relative.to_lowercase(), path) {
            return Err(CalcError::data(path, "$", format!("case-only fragment path collision with {}", first.display())));
        }
    }
    Ok(fragments.into_iter().map(|(_, path)| path).collect())
}

fn load_gear(directory: &Path) -> Result<GearResources, CalcError> {
    const FOOD_SLOT: u32 = 1 << Slot::COUNT;
    let fragments = gear_fragments(directory)?;
    let mut pieces: Vec<GearPiece> = Vec::new();
    let mut slots: [Vec<GearId>; Slot::COUNT] = std::array::from_fn(|_| Vec::new());
    let mut foods = Vec::new();
    let mut name2_ids = HashMap::new();
    let mut food_names = HashMap::new();
    let mut selectable_labels: HashMap<Arc<str>, GearOrigin> = HashMap::new();
    let mut food_origins: HashMap<Arc<str>, GearOrigin> = HashMap::new();
    let mut empty: Option<(GearId, GearOrigin)> = None;
    for path in &fragments {
        let file = DataFile::open(path)?;
        let root = file.root(&["schema_version", "pieces"], 2)?;
        let source_pieces = file.array(file.required(root, "pieces", "$")?, "$.pieces")?;
        let origin_path: Arc<Path> = Arc::from(path.as_path());
        pieces.reserve(source_pieces.len());
        for (index, value) in source_pieces.iter().enumerate() {
            let at = format!("$.pieces[{index}]");
            let object = file.object(value, &at)?;
            file.fields(object, &["name", "name2", "jobs", "slots", "type", "skill_type", "dmg", "delay", "rank", "wsc", "stats"], &at)?;
            let id = GearId(u32::try_from(pieces.len()).map_err(|_| file.error(&at, "too many gear pieces"))?);
            let name: Arc<str> = Arc::from(file.string(file.required(object, "name", &at)?, &format!("{at}.name"))?);
            let name2_field = if object.contains_key("name2") { "name2" } else { "name" };
            let name2: Arc<str> = match object.get("name2") {
                Some(value) => Arc::from(file.string(value, &format!("{at}.name2"))?),
                None => Arc::clone(&name),
            };
            let jobs = file.jobs(file.required(object, "jobs", &at)?, &format!("{at}.jobs"))?;
            let job_mask = jobs.iter().fold(0, |mask, job| mask | job.mask());
            let slots_at = format!("{at}.slots");
            let mut membership = 0;
            for (index, value) in file.array(file.required(object, "slots", &at)?, &slots_at)?.iter().enumerate() {
                let member_at = format!("{slots_at}[{index}]");
                let name = file.string(value, &member_at)?;
                let bit = if name == "food" {
                    FOOD_SLOT
                } else {
                    let slot = Slot::from_name(name).ok_or_else(|| file.error(&member_at, format!("unknown gear slot {name:?}")))?;
                    1 << slot.index()
                };
                if membership & bit != 0 {
                    return Err(file.error(&member_at, format!("duplicate gear slot {name:?}")));
                }
                membership |= bit;
            }
            let kind = match object.get("type") {
                Some(value) => {
                    let name = file.string(value, &format!("{at}.type"))?;
                    PieceKind::from_name(name).ok_or_else(|| file.error(&format!("{at}.type"), format!("unknown piece type {name:?}")))?
                }
                None => PieceKind::None,
            };
            let skill_type = match object.get("skill_type") {
                Some(value) => {
                    let name = file.string(value, &format!("{at}.skill_type"))?;
                    SkillType::from_name(name).ok_or_else(|| file.error(&format!("{at}.skill_type"), format!("unknown skill type {name:?}")))?
                }
                None => SkillType::None,
            };
            let dmg = object.get("dmg").map(|value| file.number(value, &format!("{at}.dmg"))).transpose()?.unwrap_or(0.0);
            let delay = object.get("delay").map(|value| file.number(value, &format!("{at}.delay"))).transpose()?.unwrap_or(0.0);
            let rank = match object.get("rank") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_i64().ok_or_else(|| file.error(&format!("{at}.rank"), "expected a signed 64-bit integer or null"))?),
            };
            let wsc = match object.get("wsc") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let values = file.array(value, &format!("{at}.wsc"))?;
                    if values.len() != 2 { return Err(file.error(&format!("{at}.wsc"), "expected [stat name, percentage]")); }
                    let stat_at = format!("{at}.wsc[0]");
                    let stat = file.stat(file.string(&values[0], &stat_at)?, &stat_at)?;
                    Some(WscContribution { stat, percent: file.number(&values[1], &format!("{at}.wsc[1]"))? })
                }
            };
            let stats_at = format!("{at}.stats");
            let source_stats = file.object(file.required(object, "stats", &at)?, &stats_at)?;
            let mut stats = Vec::with_capacity(source_stats.len());
            for (name, value) in source_stats {
                let stat_at = field_path(&stats_at, name);
                let stat = Stat::from_name(name)
                    .filter(|stat| membership == 0 || stat.gear_allowed())
                    .ok_or_else(|| file.error(&stat_at, format!("unsupported gear stat {name:?}")))?;
                stats.push(file.contribution(stat, value, &stat_at)?);
            }
            let origin = GearOrigin { path: Arc::clone(&origin_path), index, label_field: name2_field };
            if name2.as_ref() == "Empty" {
                if let Some((_, first)) = &empty {
                    return Err(file.error(&format!("{at}.{name2_field}"), format!("duplicate Empty gear piece; first declared at {}", first.label_location())));
                }
                empty = Some((id, origin.clone()));
            }
            if membership != 0 {
                match selectable_labels.entry(Arc::clone(&name2)) {
                    Entry::Vacant(entry) => { entry.insert(origin); }
                    Entry::Occupied(entry) => {
                        return Err(file.error(&format!("{at}.{name2_field}"), format!("duplicate selectable name2 {name2:?}; first declared at {}", entry.get().label_location())));
                    }
                }
            }
            if membership & (FOOD_SLOT - 1) != 0 {
                name2_ids.insert(Arc::clone(&name2), id);
                for &slot in Slot::ALL {
                    if membership & (1 << slot.index()) != 0 { slots[slot.index()].push(id); }
                }
            }
            if membership & FOOD_SLOT != 0 {
                match food_origins.entry(Arc::clone(&name)) {
                    Entry::Vacant(entry) => {
                        entry.insert(GearOrigin { path: Arc::clone(&origin_path), index, label_field: "name" });
                    }
                    Entry::Occupied(entry) => {
                        return Err(file.error(&format!("{at}.name"), format!("ambiguous food name {name:?}; first declared at {}", entry.get().label_location())));
                    }
                }
                food_names.insert(Arc::clone(&name), id);
                foods.push(id);
            }
            pieces.push(GearPiece { id, name, name2, jobs, job_mask, kind, skill_type, dmg, delay, rank, wsc, stats: stats.into_boxed_slice() });
        }
    }
    let empty = empty.map(|(id, _)| id)
        .ok_or_else(|| CalcError::data(&fragments[0], "$.pieces", "missing Empty gear piece"))?;
    for candidates in &mut slots {
        candidates.sort_unstable_by(|left, right| pieces[left.index()].name2.cmp(&pieces[right.index()].name2));
    }
    foods.sort_unstable_by(|left, right| pieces[left.index()].name2.cmp(&pieces[right.index()].name2));
    Ok(GearResources {
        pieces: pieces.into_boxed_slice(),
        slots: slots.map(Vec::into_boxed_slice),
        foods: foods.into_boxed_slice(),
        name2_ids,
        food_names,
        empty,
    })
}

struct EnemyResources {
    records: Box<[EnemyRecord]>,
    menu: Box<[EnemyId]>,
    names: HashMap<Arc<str>, usize>,
}

fn load_enemies(path: &Path) -> Result<EnemyResources, CalcError> {
    let file = DataFile::open(path)?;
    let root = file.root(&["schema_version", "enemies"], 1)?;
    let source = file.array(file.required(root, "enemies", "$")?, "$.enemies")?;
    let mut records = Vec::with_capacity(source.len());
    for (index, value) in source.iter().enumerate() {
        let at = format!("$.enemies[{index}]");
        let object = file.object(value, &at)?;
        let name = Arc::from(file.string(file.required(object, "Name", &at)?, &format!("{at}.Name"))?);
        let location = Arc::from(file.string(file.required(object, "Location", &at)?, &format!("{at}.Location"))?);
        for stat in [Stat::Level, Stat::Defense, Stat::Evasion, Stat::Vit, Stat::Agi, Stat::Mnd, Stat::Int, Stat::Chr, Stat::MagicEvasion, Stat::MagicDefense, Stat::MagicDtPct] {
            file.required(object, stat.name(), &at)?;
        }
        let level = file.number(file.required(object, "Level", &at)?, &format!("{at}.Level"))?;
        let mut stats = Vec::with_capacity(object.len().saturating_sub(2));
        for (name, value) in object {
            if name == "Name" || name == "Location" { continue; }
            let stat_at = field_path(&at, name);
            stats.push(file.contribution(file.stat(name, &stat_at)?, value, &stat_at)?);
        }
        records.push(EnemyRecord { name, location, level, stats: stats.into_boxed_slice() });
    }
    let mut sorted: Vec<_> = (0..records.len()).map(EnemyId).collect();
    sorted.sort_by(|left, right| records[right.index()].level.partial_cmp(&records[left.index()].level).expect("finite enemy levels"));
    let mut menu = Vec::with_capacity(sorted.len());
    let mut names = HashMap::with_capacity(sorted.len());
    // Python's ordered name dictionary keeps the first key position, but the last value.
    for id in sorted {
        match names.entry(Arc::clone(&records[id.index()].name)) {
            Entry::Vacant(entry) => { entry.insert(menu.len()); menu.push(id); }
            Entry::Occupied(entry) => { menu[*entry.get()] = id; }
        }
    }
    Ok(EnemyResources { records: records.into_boxed_slice(), menu: menu.into_boxed_slice(), names })
}

fn load_buffs(path: &Path) -> Result<BuffCatalog, CalcError> {
    let file = DataFile::open(path)?;
    let root = file.root(&["schema_version", "cor", "brd", "geo", "whm", "brd_song_limits", "cor_debuffs", "geo_debuffs", "whm_debuffs", "misc_debuffs"], 1)?;
    let mut tables = Vec::with_capacity(BuffTableKind::ALL.len());
    for kind in BuffTableKind::ALL {
        let at = field_path("$", kind.name());
        let source = file.object(file.required(root, kind.name(), "$")?, &at)?;
        let mut entries = Vec::with_capacity(source.len());
        let mut names = HashMap::with_capacity(source.len());
        for (name, value) in source {
            let buff_at = field_path(&at, name);
            let object = file.object(value, &buff_at)?;
            let level_requirement = object.get("level requirement")
                .map(|value| file.number(value, &field_path(&buff_at, "level requirement"))).transpose()?;
            let job_requirement = object.get("job requirement")
                .map(|value| file.jobs(value, &field_path(&buff_at, "job requirement"))).transpose()?;
            if kind == BuffTableKind::MiscDebuffs {
                file.required(object, "level requirement", &buff_at)?;
                file.required(object, "job requirement", &buff_at)?;
            }
            let metadata_count = usize::from(level_requirement.is_some()) + usize::from(job_requirement.is_some());
            let mut stats = Vec::with_capacity(object.len() - metadata_count);
            for (name, value) in object {
                if name == "level requirement" || name == "job requirement" { continue; }
                let stat_at = field_path(&buff_at, name);
                let stat = file.stat(name, &stat_at)?;
                stats.push(BuffStat { stat, value: buff_value(&file, kind, value, &stat_at)? });
            }
            let name: Arc<str> = Arc::from(name.as_str());
            names.insert(Arc::clone(&name), entries.len());
            entries.push(BuffDefinition { name, stats: stats.into_boxed_slice(), level_requirement, job_requirement, song_bonus_cap: None });
        }
        tables.push(BuffTable { entries: entries.into_boxed_slice(), names });
    }
    let mut tables: [BuffTable; 8] = tables.try_into().expect("eight buff table kinds");
    let source_limits = file.object(file.required(root, "brd_song_limits", "$")?, "$.brd_song_limits")?;
    let mut song_limits = Vec::with_capacity(source_limits.len());
    let brd = &mut tables[BuffTableKind::Brd.index()];
    for (name, value) in source_limits {
        let at = field_path("$.brd_song_limits", name);
        let song_index = brd.index_of(name).ok_or_else(|| file.error(&at, format!("unknown BRD song {name:?}")))?;
        let cap = file.number(value, &at)?;
        brd.entries[song_index].song_bonus_cap = Some(cap);
        song_limits.push(SongLimit { song_index, cap });
    }
    for song in &brd.entries {
        if song.song_bonus_cap.is_none() {
            return Err(file.error(&field_path("$.brd_song_limits", &song.name), "missing BRD song bonus cap"));
        }
    }
    Ok(BuffCatalog { tables, song_limits: song_limits.into_boxed_slice() })
}

fn buff_value(file: &DataFile, kind: BuffTableKind, value: &Value, at: &str) -> Result<BuffValue, CalcError> {
    match kind {
        BuffTableKind::Cor => {
            let object = file.object(value, at)?;
            file.fields(object, &["potency", "per_roll_bonus", "job_bonus"], at)?;
            let potency_at = field_path(at, "potency");
            let source = file.object(file.required(object, "potency", at)?, &potency_at)?;
            file.fields(source, &ROLL_NUMERALS, &potency_at)?;
            let mut potency = [0.0; 11];
            for (index, numeral) in ROLL_NUMERALS.iter().enumerate() {
                potency[index] = file.number(file.required(source, numeral, &potency_at)?, &field_path(&potency_at, numeral))?;
            }
            Ok(BuffValue::CorRoll(CorRoll {
                potency,
                per_roll_bonus: file.number(file.required(object, "per_roll_bonus", at)?, &field_path(at, "per_roll_bonus"))?,
                job_bonus: file.number(file.required(object, "job_bonus", at)?, &field_path(at, "job_bonus"))?,
            }))
        }
        BuffTableKind::Brd | BuffTableKind::Geo | BuffTableKind::GeoDebuffs => {
            let object = file.object(value, at)?;
            file.fields(object, &["base", "per_point"], at)?;
            Ok(BuffValue::Scaling(Scaling {
                base: file.number(file.required(object, "base", at)?, &field_path(at, "base"))?,
                per_point: file.number(file.required(object, "per_point", at)?, &field_path(at, "per_point"))?,
            }))
        }
        BuffTableKind::Whm | BuffTableKind::CorDebuffs | BuffTableKind::WhmDebuffs | BuffTableKind::MiscDebuffs => {
            Ok(BuffValue::Constant(file.number(value, at)?))
        }
    }
}

struct AttachmentResources {
    records: Box<[Value]>,
    names: HashMap<Arc<str>, usize>,
}

fn load_attachments(path: &Path) -> Result<AttachmentResources, CalcError> {
    let mut file = DataFile::open(path)?;
    let root = file.root(&["schema_version", "attachments"], 1)?;
    let source = file.array(file.required(root, "attachments", "$")?, "$.attachments")?;
    let mut names = HashMap::with_capacity(source.len());
    for (index, value) in source.iter().enumerate() {
        let at = format!("$.attachments[{index}]");
        let object = file.object(value, &at)?;
        let name = file.string(file.required(object, "Name", &at)?, &format!("{at}.Name"))?;
        file.string(file.required(object, "Element", &at)?, &format!("{at}.Element"))?;
        file.u32(file.required(object, "Capacity", &at)?, &format!("{at}.Capacity"))?;
        names.insert(Arc::from(name), index);
    }
    let Value::Array(records) = file.value.get_mut("attachments").expect("validated attachments field").take() else {
        unreachable!("validated attachments array")
    };
    Ok(AttachmentResources { records: records.into_boxed_slice(), names })
}

struct AliasResources {
    records: Box<[ItemAlias]>,
    full_names: HashMap<String, usize>,
    abbreviations: HashMap<String, usize>,
}

fn load_aliases(path: &Path) -> Result<AliasResources, CalcError> {
    let file = File::open(path).map_err(|error| CalcError::data(path, "$", error.to_string()))?;
    let mut reader = csv::ReaderBuilder::new().delimiter(b';').flexible(false).from_reader(file);
    let headers = reader.headers().map_err(|error| csv_error(path, error))?;
    if headers.iter().ne(["id", "name", "name2"]) {
        return Err(CalcError::data(path, "line 1", "expected exactly the columns id;name;name2, in that order"));
    }
    let mut records = Vec::new();
    let mut full_names = HashMap::new();
    let mut abbreviations = HashMap::new();
    for record in reader.records() {
        let record = record.map_err(|error| csv_error(path, error))?;
        let line = record.position().map_or(0, csv::Position::line);
        let id = &record[0];
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) || id.parse::<u32>().is_err() {
            return Err(CalcError::data(path, format!("line {line}.id"), format!("expected a decimal item ID in 0..=4294967295, got {id:?}")));
        }
        let index = records.len();
        full_names.entry(record[1].to_lowercase()).or_insert(index);
        abbreviations.entry(record[2].to_lowercase()).or_insert(index);
        records.push(ItemAlias { id: Arc::from(id), name: Arc::from(&record[1]), name2: Arc::from(&record[2]) });
    }
    Ok(AliasResources { records: records.into_boxed_slice(), full_names, abbreviations })
}

fn csv_error(path: &Path, error: csv::Error) -> CalcError {
    let at = error.position().map_or_else(|| "$".to_owned(), |position| format!("line {} (record {})", position.line(), position.record()));
    CalcError::data(path, at, error.to_string())
}
