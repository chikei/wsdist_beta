use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    symbols: Box<[GearSymbol]>,
    original_ids: HashMap<u32, GearId>,
    symbol_ids: HashMap<Arc<str>, GearId>,
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
        let gear = load_gear(&root.join("data/gear.json"))?;
        let enemies = load_enemies(&root.join("data/enemies.json"))?;
        let buffs = load_buffs(&root.join("data/buffs.json"))?;
        let attachments = load_attachments(&root.join("data/pup-attachments.json"))?;
        let aliases = load_aliases(&root.join("item_list.csv"))?;
        Ok(Self {
            pieces: gear.pieces,
            slots: gear.slots,
            foods: gear.foods,
            symbols: gear.symbols,
            original_ids: gear.original_ids,
            symbol_ids: gear.symbol_ids,
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
    pub fn symbols(&self) -> &[GearSymbol] { &self.symbols }
    pub fn by_name2(&self, name: &str) -> Option<GearId> { self.name2_ids.get(name).copied() }
    pub fn by_symbol(&self, name: &str) -> Option<GearId> { self.symbol_ids.get(name).copied() }
    pub fn by_original_id(&self, id: u32) -> Option<GearId> { self.original_ids.get(&id).copied() }
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

#[derive(Debug)]
pub struct GearSymbol {
    pub name: Arc<str>,
    pub id: GearId,
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

struct DataFile {
    path: PathBuf,
    value: Value,
}

impl DataFile {
    fn open(path: &Path) -> Result<Self, CalcError> {
        let file = File::open(path).map_err(|error| CalcError::data(path, "$", error.to_string()))?;
        let value = serde_json::from_reader(BufReader::new(file)).map_err(|error| {
            CalcError::data(path, format!("$ (line {}, column {})", error.line(), error.column()), error.to_string())
        })?;
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

    fn root(&self, allowed: &[&str]) -> Result<&Map<String, Value>, CalcError> {
        let root = self.object(&self.value, "$")?;
        self.fields(root, allowed, "$")?;
        let version = self.required(root, "schema_version", "$")?;
        if version.as_u64() != Some(1) {
            return Err(self.error("$.schema_version", "expected schema version 1"));
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
    symbols: Box<[GearSymbol]>,
    original_ids: HashMap<u32, GearId>,
    symbol_ids: HashMap<Arc<str>, GearId>,
    name2_ids: HashMap<Arc<str>, GearId>,
    food_names: HashMap<Arc<str>, GearId>,
    empty: GearId,
}

fn load_gear(path: &Path) -> Result<GearResources, CalcError> {
    let file = DataFile::open(path)?;
    let root = file.root(&["schema_version", "pieces", "symbols", "slots", "foods"])?;
    let source_pieces = file.array(file.required(root, "pieces", "$")?, "$.pieces")?;
    let mut pieces = Vec::with_capacity(source_pieces.len());
    let mut original_ids = HashMap::with_capacity(source_pieces.len());
    for (index, value) in source_pieces.iter().enumerate() {
        let at = format!("$.pieces[{index}]");
        let object = file.object(value, &at)?;
        file.fields(object, &["id", "name", "name2", "jobs", "type", "skill_type", "dmg", "delay", "rank", "wsc", "stats"], &at)?;
        let original_id = file.u32(file.required(object, "id", &at)?, &format!("{at}.id"))?;
        let id = GearId(u32::try_from(index).map_err(|_| file.error(&at, "too many gear pieces"))?);
        if original_ids.insert(original_id, id).is_some() {
            return Err(file.error(&format!("{at}.id"), format!("duplicate original gear ID {original_id}")));
        }
        let name = Arc::from(file.string(file.required(object, "name", &at)?, &format!("{at}.name"))?);
        let name2 = Arc::from(file.string(file.required(object, "name2", &at)?, &format!("{at}.name2"))?);
        let jobs = file.jobs(file.required(object, "jobs", &at)?, &format!("{at}.jobs"))?;
        let job_mask = jobs.iter().fold(0, |mask, job| mask | job.mask());
        let kind_name = file.string(file.required(object, "type", &at)?, &format!("{at}.type"))?;
        let kind = PieceKind::from_name(kind_name).ok_or_else(|| file.error(&format!("{at}.type"), format!("unknown piece type {kind_name:?}")))?;
        let skill_name = file.string(file.required(object, "skill_type", &at)?, &format!("{at}.skill_type"))?;
        let skill_type = SkillType::from_name(skill_name).ok_or_else(|| file.error(&format!("{at}.skill_type"), format!("unknown skill type {skill_name:?}")))?;
        let dmg = file.number(file.required(object, "dmg", &at)?, &format!("{at}.dmg"))?;
        let delay = file.number(file.required(object, "delay", &at)?, &format!("{at}.delay"))?;
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
                .ok_or_else(|| file.error(&stat_at, format!("unsupported gear stat {name:?}")))?;
            stats.push(file.contribution(stat, value, &stat_at)?);
        }
        pieces.push(GearPiece { id, original_id, name, name2, jobs, job_mask, kind, skill_type, dmg, delay, rank, wsc, stats: stats.into_boxed_slice() });
    }

    let source_symbols = file.object(file.required(root, "symbols", "$")?, "$.symbols")?;
    let mut symbols = Vec::with_capacity(source_symbols.len());
    let mut symbol_ids = HashMap::with_capacity(source_symbols.len());
    for (name, value) in source_symbols {
        let id = gear_reference(&file, value, &field_path("$.symbols", name), &original_ids)?;
        let name: Arc<str> = Arc::from(name.as_str());
        symbol_ids.insert(Arc::clone(&name), id);
        symbols.push(GearSymbol { name, id });
    }
    let source_slots = file.object(file.required(root, "slots", "$")?, "$.slots")?;
    let allowed_slots: [&str; Slot::COUNT] = std::array::from_fn(|index| Slot::ALL[index].name());
    file.fields(source_slots, &allowed_slots, "$.slots")?;
    let mut slots: [Box<[GearId]>; Slot::COUNT] = std::array::from_fn(|_| Box::default());
    let mut name2_ids = HashMap::with_capacity(pieces.len());
    for &slot in Slot::ALL {
        let at = field_path("$.slots", slot.name());
        slots[slot.index()] = gear_references(&file, file.required(source_slots, slot.name(), "$.slots")?, &at, &original_ids)?;
        for &id in &slots[slot.index()] {
            name2_ids.insert(Arc::clone(&pieces[id.index()].name2), id);
        }
    }
    let foods = gear_references(&file, file.required(root, "foods", "$")?, "$.foods", &original_ids)?;
    // The original gate checks final slot/food lists, not declared-only bindings.
    // Retain those records without normalizing their known internal stat names.
    for id in slots.iter().flat_map(|slot| slot.iter()).chain(foods.iter()) {
        let piece = &pieces[id.index()];
        for contribution in &piece.stats {
            if !contribution.stat.gear_allowed() {
                let at = field_path(&format!("$.pieces[{}].stats", id.index()), contribution.stat.name());
                return Err(file.error(&at, format!("unsupported gear stat {:?}", contribution.stat.name())));
            }
        }
    }
    let mut food_names = HashMap::with_capacity(foods.len());
    for &id in &foods { food_names.insert(Arc::clone(&pieces[id.index()].name), id); }
    let empty = if let Some(&id) = symbol_ids.get("Empty") {
        if pieces[id.index()].name2.as_ref() != "Empty" {
            return Err(file.error("$.symbols.Empty", "Empty must reference a piece with name2 \"Empty\""));
        }
        id
    } else {
        pieces.iter().find(|piece| piece.name2.as_ref() == "Empty").map(|piece| piece.id)
            .ok_or_else(|| file.error("$.pieces", "missing Empty gear piece"))?
    };
    Ok(GearResources { pieces: pieces.into_boxed_slice(), slots, foods, symbols: symbols.into_boxed_slice(), original_ids, symbol_ids, name2_ids, food_names, empty })
}

fn gear_reference(file: &DataFile, value: &Value, at: &str, ids: &HashMap<u32, GearId>) -> Result<GearId, CalcError> {
    let original_id = file.u32(value, at)?;
    ids.get(&original_id).copied().ok_or_else(|| file.error(at, format!("unknown original gear ID {original_id}")))
}

fn gear_references(file: &DataFile, value: &Value, at: &str, ids: &HashMap<u32, GearId>) -> Result<Box<[GearId]>, CalcError> {
    let source = file.array(value, at)?;
    let mut references = Vec::with_capacity(source.len());
    for (index, value) in source.iter().enumerate() {
        references.push(gear_reference(file, value, &format!("{at}[{index}]"), ids)?);
    }
    Ok(references.into_boxed_slice())
}

struct EnemyResources {
    records: Box<[EnemyRecord]>,
    menu: Box<[EnemyId]>,
    names: HashMap<Arc<str>, usize>,
}

fn load_enemies(path: &Path) -> Result<EnemyResources, CalcError> {
    let file = DataFile::open(path)?;
    let root = file.root(&["schema_version", "enemies"])?;
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
    let root = file.root(&["schema_version", "cor", "brd", "geo", "whm", "brd_song_limits", "cor_debuffs", "geo_debuffs", "whm_debuffs", "misc_debuffs"])?;
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
    let root = file.root(&["schema_version", "attachments"])?;
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
