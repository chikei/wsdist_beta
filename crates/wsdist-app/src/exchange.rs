//! Native gear exchange and transactional catalog replacement.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use slint::Image;
use wsdist_core::model::{CandidatePools, ModelState};
use wsdist_core::{Catalog, GearId, Gearset, Slot};

use crate::resources::{AppPaths, ResourceError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GearSource {
    Quicklook = 0,
    Tp = 1,
    Ws = 2,
}

impl GearSource {
    pub const ALL: [Self; 3] = [Self::Quicklook, Self::Tp, Self::Ws];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Quicklook => "quicklook",
            Self::Tp => "tp",
            Self::Ws => "ws",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Quicklook => "Quicklook",
            Self::Tp => "TP",
            Self::Ws => "WS",
        }
    }
}

impl fmt::Display for GearSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.display_name())
    }
}

/// Exports base names, never name2 augment descriptions. CSV lookup retains its
/// first-row semantics, and WS uses its own set rather than Quicklook's set.
pub fn export_lua(catalog: &Catalog, sets: &[Gearset; 3], source: GearSource) -> String {
    let mut output = String::with_capacity(1024);
    output.push_str("new_set = {\n");
    for &slot in Slot::ALL {
        let piece = catalog.piece(sets[source.index()][slot.index()]);
        let name = catalog.alias_by_full_name(&piece.name)
            .map_or(piece.name.as_ref(), |alias| alias.name2.as_ref());
        output.push_str("    ");
        output.push_str(slot.name());
        output.push_str("=\"");
        append_lua_capitalized(&mut output, name);
        output.push_str("\",\n");
    }
    output.push('}');
    output
}

fn append_lua_capitalized(output: &mut String, name: &str) {
    // The source capitalizes whitespace-separated words, not every component
    // after punctuation (e.g. "Death's" and "Hachirin-no-obi").
    for (index, word) in name.split_whitespace().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            for character in first.to_uppercase() {
                append_lua_character(output, character);
            }
        }
        for character in characters.flat_map(char::to_lowercase) {
            append_lua_character(output, character);
        }
    }
}

fn append_lua_character(output: &mut String, character: char) {
    match character {
        '\\' => output.push_str("\\\\"),
        '"' => output.push_str("\\\""),
        '\n' => output.push_str("\\n"),
        '\r' => output.push_str("\\r"),
        '\t' => output.push_str("\\t"),
        _ => output.push(character),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("Cannot open the platform clipboard: {source}")]
    Open {
        #[source]
        source: arboard::Error,
    },
    #[error("Cannot copy the gear set to the platform clipboard: {source}")]
    Write {
        #[source]
        source: arboard::Error,
    },
}

/// Keep this owner in the application's event-loop state until shutdown. Linux
/// clipboard contents are hosted by the retained arboard instance.
#[derive(Default)]
pub struct ClipboardOwner {
    clipboard: Option<arboard::Clipboard>,
}

impl ClipboardOwner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn copy_export(
        &mut self,
        catalog: &Catalog,
        sets: &[Gearset; 3],
        source: GearSource,
    ) -> Result<(), ClipboardError> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new()
                .map_err(|source| ClipboardError::Open { source })?);
        }
        self.clipboard.as_mut().expect("clipboard was opened")
            .set_text(export_lua(catalog, sets, source))
            .map_err(|source| ClipboardError::Write { source })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IconWarning {
    #[error("No CSV icon alias for gear {name:?}")]
    MissingAlias { name: String },
    #[error("Cannot load icon {path:?}: {message}")]
    Asset { path: PathBuf, message: String },
}

/// Event-loop-thread image cache. Missing aliases and unavailable assets display
/// nothing and are reported once rather than once per model refresh.
pub struct IconProvider {
    icons: PathBuf,
    by_name: HashMap<Arc<str>, Image>,
    images: HashMap<PathBuf, Option<Image>>,
    missing_aliases: HashSet<Arc<str>>,
    warnings: Vec<IconWarning>,
}

impl IconProvider {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            icons: paths.icons.clone(),
            by_name: HashMap::new(),
            images: HashMap::new(),
            missing_aliases: HashSet::new(),
            warnings: Vec::new(),
        }
    }

    pub fn icon(&mut self, catalog: &Catalog, id: GearId) -> Image {
        let name = &catalog.piece(id).name;
        if let Some(image) = self.by_name.get(name.as_ref()) {
            return image.clone();
        }
        let image = self.load_named(catalog, name).unwrap_or_default();
        self.by_name.insert(Arc::clone(name), image.clone());
        image
    }

    /// Call after a committed data reload to allow edited/new images and aliases
    /// to be read again. Already queued warnings remain available to the UI.
    pub fn clear_cache(&mut self) {
        self.by_name.clear();
        self.images.clear();
        self.missing_aliases.clear();
    }

    pub fn take_warnings(&mut self) -> Vec<IconWarning> {
        std::mem::take(&mut self.warnings)
    }

    fn load_named(&mut self, catalog: &Catalog, name: &Arc<str>) -> Option<Image> {
        let Some(alias) = catalog.alias_by_full_name(name) else {
            if self.missing_aliases.insert(Arc::clone(name)) {
                self.report(IconWarning::MissingAlias { name: name.to_string() });
            }
            return None;
        };
        let path = self.icons.join(format!("{}.png", alias.id));
        if let Some(image) = self.images.get(&path) {
            return image.clone();
        }
        let image = match Image::load_from_path(&path) {
            Ok(image) => Some(image),
            Err(error) => {
                self.report(IconWarning::Asset { path: path.clone(), message: error.to_string() });
                None
            }
        };
        self.images.insert(path, image.clone());
        image
    }

    fn report(&mut self, warning: IconWarning) {
        eprintln!("{warning}");
        self.warnings.push(warning);
    }
}

/// These are the actual combo strings, not interpreted numeric ranks. Unknown
/// saved combo text must retain the same literal comparison behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkFilters {
    pub odyssey_rank: String,
    pub nyame_r25_b: bool,
    pub soa_ring: String,
    pub tvr_ring: String,
    pub mastery_rank: String,
}

impl Default for BulkFilters {
    fn default() -> Self {
        Self {
            odyssey_rank: "30".to_owned(),
            nyame_r25_b: true,
            soa_ring: "Weatherspoon".to_owned(),
            tvr_ring: "Lehko Habhoka's".to_owned(),
            mastery_rank: "MR07".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkAction {
    SelectAll,
    SelectSlot(Slot),
    UnselectSlot(Slot),
}

/// Visible lists are supplied in the checkbox models' full master order. A bulk
/// action clears hidden checks in affected slots; other slots retain every
/// selected occurrence in its existing order. Manual checks do not use filters.
pub fn select_bulk(
    catalog: &Catalog,
    visible: &CandidatePools,
    selected: &CandidatePools,
    filters: &BulkFilters,
    action: BulkAction,
) -> CandidatePools {
    match action {
        BulkAction::UnselectSlot(target) => std::array::from_fn(|index| {
            if Slot::ALL[index] == target { Vec::new() } else { selected[index].clone() }
        }),
        BulkAction::SelectAll | BulkAction::SelectSlot(_) => {
            let filters = CompiledBulkFilters::new(filters);
            std::array::from_fn(|index| {
                let slot = Slot::ALL[index];
                if let BulkAction::SelectSlot(target) = action
                    && slot != target
                {
                    selected[index].clone()
                } else {
                    visible[index].iter().copied()
                        .filter(|id| bulk_includes(catalog, *id, slot, &filters, true, false))
                        .collect()
                }
            })
        }
    }
}

/// Reads only the first double-quoted substring on each line. This deliberately
/// does not interpret Lua, quantities, escape sequences or augment tables.
/// Matches use the first case-insensitive abbreviated CSV row; returned full
/// names retain the CSV spelling and duplicate input lines.
pub fn parse_inventory(catalog: &Catalog, contents: &str) -> Vec<String> {
    inventory_names(catalog, contents).map(str::to_owned).collect()
}

fn inventory_names<'a>(catalog: &'a Catalog, contents: &'a str) -> impl Iterator<Item = &'a str> {
    contents.lines().filter_map(move |line| {
        let (_, quoted) = line.split_once('"')?;
        let (abbreviation, _) = quoted.split_once('"')?;
        catalog.alias_by_abbreviation(abbreviation).map(|alias| alias.name.as_ref())
    })
}

pub fn select_inventory(
    catalog: &Catalog,
    visible: &CandidatePools,
    filters: &BulkFilters,
    contents: &str,
) -> CandidatePools {
    let filters = CompiledBulkFilters::new(filters);
    let names: HashSet<String> = inventory_names(catalog, contents).map(str::to_lowercase).collect();
    std::array::from_fn(|index| {
        let slot = Slot::ALL[index];
        visible[index].iter().copied().filter(|id| {
            let name = catalog.piece(*id).name.to_lowercase();
            let base = name.split(" +").next().unwrap_or(&name);
            (names.contains(name.as_str()) || names.contains(base))
                && bulk_includes(catalog, *id, slot, &filters, false, true)
        }).collect()
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportOutcome {
    Cancelled,
    Completed,
}

#[derive(Debug, thiserror::Error)]
#[error("Cannot read inventory file {path:?}: {source}")]
pub struct InventoryError {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

/// Commits all candidate slots only after a completed read. Cancellation and
/// unreadable/invalid-UTF-8 files leave hidden and visible selections intact.
pub fn import_inventory_file(
    catalog: &Catalog,
    visible: &CandidatePools,
    filters: &BulkFilters,
    selected: &mut CandidatePools,
    path: Option<&Path>,
) -> Result<ImportOutcome, InventoryError> {
    let Some(path) = path else {
        return Ok(ImportOutcome::Cancelled);
    };
    let contents = fs::read_to_string(path).map_err(|source| InventoryError {
        path: path.to_path_buf(),
        source,
    })?;
    let imported = select_inventory(catalog, visible, filters, &contents);
    *selected = imported;
    Ok(ImportOutcome::Completed)
}

struct CompiledBulkFilters {
    rank: Option<i64>,
    nyame_r25_b: bool,
    mastery_rank: String,
    soa_ring: String,
    tvr_ring: String,
}

impl CompiledBulkFilters {
    fn new(filters: &BulkFilters) -> Self {
        Self {
            rank: filters.odyssey_rank.parse::<i64>().ok()
                .filter(|rank| rank.to_string() == filters.odyssey_rank),
            nyame_r25_b: filters.odyssey_rank == "30" && filters.nyame_r25_b,
            mastery_rank: filters.mastery_rank.to_lowercase(),
            soa_ring: format!("{} ring +1", filters.soa_ring.to_lowercase()),
            tvr_ring: format!("{} ring", filters.tvr_ring.to_lowercase()),
        }
    }
}

fn bulk_includes(
    catalog: &Catalog,
    id: GearId,
    slot: Slot,
    filters: &CompiledBulkFilters,
    selecting_all: bool,
    inventory: bool,
) -> bool {
    let piece = catalog.piece(id);
    let name = piece.name2.as_ref();
    let lower = name.to_lowercase();
    let mut selected = piece.rank.is_none_or(|rank| Some(rank) == filters.rank);

    // Nyame R25B is explicitly reselected *after* the Odyssey-rank exclusion,
    // but only by Select all, not inventory import.
    if lower.contains("nyame") {
        if !name.ends_with('B') {
            selected = false;
        } else if filters.nyame_r25_b {
            if name.contains("30B") {
                selected = false;
            } else if name.contains("25B") && selecting_all {
                selected = true;
            }
        }
    }

    let first = name.split_whitespace().next().unwrap_or("");
    match slot {
        Slot::Main | Slot::Sub | Slot::Ranged => {
            // This is intentionally a first-token check against the literal
            // source list, including its multiword entries.
            if REMA_WEAPONS.contains(&first) && !name.contains("R15")
                || name.split_whitespace().next_back() == Some("V")
                || lower.contains("kraken")
            {
                selected = false;
            }
        }
        Slot::Ammo => {
            if name.contains("Hoxne") || name.contains("Antitail") {
                selected = false;
            }
        }
        Slot::Head | Slot::Body | Slot::Hands | Slot::Legs | Slot::Feet => {
            if !inventory
                && ((RELIC_NAMES.contains(&first) || AF_NAMES.contains(&first)) && !name.contains("+4")
                    || EMPYREAN_NAMES.contains(&first) && !name.contains("+3"))
            {
                selected = false;
            }
            if name.contains("R30") && LIMBUS_NAMES.iter().any(|set| lower.contains(*set)) {
                selected = false;
            }
        }
        Slot::Neck => {
            if !inventory && name.contains("R20") && name.contains("+1") {
                selected = false;
            }
        }
        Slot::Ear1 | Slot::Ear2 => {
            if EMPYREAN_NAMES.contains(&first) && name.contains("+2")
                || name.contains("Hoxne") && !lower.contains(&filters.mastery_rank)
                || name.contains("Balder")
                || lower.contains("(night)")
            {
                selected = false;
            }
        }
        Slot::Ring1 | Slot::Ring2 => {
            if TVR_RINGS.contains(&lower.as_str())
                && lower != filters.tvr_ring
                || SOA_RINGS.contains(&lower.as_str())
                    && lower != filters.soa_ring
            {
                selected = false;
            }
        }
        Slot::Back | Slot::Waist => {}
    }
    selected
}

const EMPYREAN_NAMES: [&str; 22] = [
    "Hattori", "Heathen's", "Wicce", "Lethargy", "Peltast's", "Ebers", "Kasuga", "Arbatel",
    "Boii", "Chasseur's", "Fili", "Skulker's", "Bhikku", "Maculele", "Nukumi", "Azimuth",
    "Chevalier's", "Amini", "Hashishin", "Erilaz", "Karagoz", "Beckoner's",
];
const RELIC_NAMES: [&str; 22] = [
    "Pedagogy", "Hesychast", "Vitiation", "Mochizuki", "Fallen", "Horos", "Pitre", "Luhlaza",
    "Plunderer", "Bagua", "Archmage", "Piety", "Agoge", "Caballarius", "Wakido", "Ankusa",
    "Bihu", "Glyphic", "Lanun", "Arcadian", "Pteroslaver", "Futhark",
];
const AF_NAMES: [&str; 22] = [
    "Academic", "Anchorite", "Atrophy", "Hachiya", "Ignominy", "Maxixi", "Foire", "Assimilator",
    "Pillager", "Geomancy", "Spaekona", "Theophany", "Pummeler", "Reverence", "Sakonji", "Totemic",
    "Brioso", "Convoker", "Laksamana", "Orion", "Vishap", "Runeist",
];
const LIMBUS_NAMES: [&str; 15] = [
    "hope", "perfection", "revelation", "trust", "prestige", "sworn", "bravery", "intrepid",
    "indomitable", "justice", "magnificent", "duty", "mercy", "grace", "clemency",
];
const REMA_WEAPONS: &[&str] = &[
    "Amanomurakumo", "Annihilator", "Apocalypse", "Bravura", "Excalibur", "Gungnir", "Guttler",
    "Kikoku", "Mandau", "Mjollnir", "Ragnarok", "Spharai", "Yoichinoyumi", "Almace", "Armageddon",
    "Caladbolg", "Farsha", "Gandiva", "Kannagi", "Masamune", "Redemption", "Rhongomiant", "Twashtar",
    "Ukonvasara", "Verethragna", "Hvergelmir", "Aymur", "Burtgang", "Carnwenhan", "Conqueror",
    "Death Penalty", "Gastraphetes", "Glanzfaust", "Kenkonken", "Kogarasumaru", "Laevateinn",
    "Liberator", "Murgleis", "Nagi", "Ryunohige", "Terpsichore", "Tizona", "Tupsimati", "Nirvana",
    "Vajra", "Yagrush", "Epeolatry", "Idris", "Aeneas", "Anguta", "Chango", "Dojikiri Yasutsuna",
    "Fail-not", "Fomalhaut", "Godhands", "Heishi Shorinken", "Khatvanga", "Lionheart", "Sequence",
    "Tishtrya", "Tri-edge", "Trishula",
];
const TVR_RINGS: [&str; 8] = [
    "cornelia's ring", "ephramad's ring", "fickblix's ring", "gurebu-ogurebu's ring",
    "lehko habhoka's ring", "medada's ring", "ragelise's ring", "none ring",
];
const SOA_RINGS: [&str; 4] = [
    "weatherspoon ring +1", "karieyh ring +1", "vocane ring +1", "none ring +1",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedEquipped {
    pub source: GearSource,
    pub slot: Slot,
    pub name2: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedCandidate {
    pub slot: Slot,
    pub name2: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReloadReport {
    pub removed_equipped: Vec<RemovedEquipped>,
    pub removed_candidates: Vec<RemovedCandidate>,
}

impl fmt::Display for ReloadReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "Data reloaded.")?;
        for removed in &self.removed_equipped {
            writeln!(formatter, "{} {}: {:?} removed; equipped Empty.",
                removed.source, removed.slot.name(), removed.name2)?;
        }
        for removed in &self.removed_candidates {
            writeln!(formatter, "{} candidate {:?} removed.", removed.slot.name(), removed.name2)?;
        }
        Ok(())
    }
}

/// Parse the complete resource set before touching session state. All name2
/// resolutions use the new catalog's index, including its last-value-wins rule.
/// Candidate occurrences keep their order and duplicates. The controller may
/// rebuild models after this commit, but must not load a profile as part of it.
pub fn reload_data(
    paths: &AppPaths,
    catalog: &mut Arc<Catalog>,
    sets: &mut [Gearset; 3],
    candidates: &mut CandidatePools,
    model: &mut ModelState,
) -> Result<ReloadReport, ResourceError> {
    let replacement = Arc::new(Catalog::load(&paths.root)?);
    let mut report = ReloadReport::default();
    let mut resolved_sets = [[replacement.empty; Slot::COUNT]; 3];
    for source in GearSource::ALL {
        for &slot in Slot::ALL {
            let old = catalog.piece(sets[source.index()][slot.index()]);
            if let Some(id) = replacement.by_name2(&old.name2) {
                resolved_sets[source.index()][slot.index()] = id;
            } else {
                report.removed_equipped.push(RemovedEquipped {
                    source, slot, name2: old.name2.to_string(),
                });
            }
        }
    }
    let resolved_candidates = std::array::from_fn(|index| {
        candidates[index].iter().filter_map(|old_id| {
            let old = catalog.piece(*old_id);
            match replacement.by_name2(&old.name2) {
                Some(id) => Some(id),
                None => {
                    report.removed_candidates.push(RemovedCandidate {
                        slot: Slot::ALL[index], name2: old.name2.to_string(),
                    });
                    None
                }
            }
        }).collect()
    });
    model.reset(replacement.pieces().len());
    *sets = resolved_sets;
    *candidates = resolved_candidates;
    *catalog = replacement;
    Ok(report)
}
