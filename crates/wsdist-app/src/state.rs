use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use wsdist_core::Catalog;
use wsdist_core::buffs::{aggregate_buffs, build_enemy, parse_integer_text};
use wsdist_core::catalog::BuffTableKind;
use wsdist_core::model::*;
use wsdist_core::rng::RandomSources;
use wsdist_core::stats::{Stat, Stats};

use crate::exchange::GearSource;
use crate::input_metadata::{ABILITY_SPECS, ENEMY_FIELDS, spell_menu, ws_menu};
use crate::profile_registry::{INPUT_REGISTRY, InputKind, input_spec};
use crate::profiles::{GEAR_PROFILE_KEYS, ProfileStore, ProfileValues, legacy_text};
use crate::selections::{GearModels, can_dual_wield};
use crate::resources::{AppPaths, ResourceError};

#[derive(Clone, Debug)]
pub struct InputControl {
    pub kind: InputKind,
    pub value: Value,
    pub choices: Vec<String>,
    pub enabled: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub struct CalculationInputs {
    pub player: PlayerInput,
    pub enemy: Enemy,
    pub ws_name: Arc<str>,
    pub ws_type: WsType,
    pub spell_name: Arc<str>,
    pub tp: f64,
}

pub struct UiState {
    pub engine: EvaluationContext<RandomSources>,
    pub controls: BTreeMap<&'static str, InputControl>,
    pub gear: GearModels,
    pub whm_selections: [String; 4],
    pub cached_food: Option<GearId>,
    pub sub_job_level: i32,
    pub main_weapon_skill: SkillType,
    pub ranged_weapon_skill: SkillType,
    pub enhancing_skill_label: String,
    pub active_tab: usize,
    pub damage_limit: bool,
    pub verbose_swaps: bool,
    pub plot_dps: bool,
    pub last_player: Option<Player>,
    pub messages: Vec<String>,
}

impl UiState {
    /// Construction and profile restoration are distinct: saved widgets do not
    /// emit the activation callbacks used by new direct user actions.
    pub fn new(catalog: Arc<Catalog>, random: RandomSources, profiles: Option<&ProfileStore>) -> Self {
        let controls = INPUT_REGISTRY.iter().map(|spec| (spec.key, InputControl {
            kind: spec.kind, value: spec.initial_value(), choices: spec.choices.iter().map(|text| (*text).to_owned()).collect(),
            enabled: true, hidden: false,
        })).collect();
        let cached_food = catalog.food_by_name("Grape Daifuku");
        let mut state = Self {
            gear: GearModels::new(&catalog), engine: EvaluationContext::new(catalog, random), controls,
            whm_selections: ["Dia II".into(), "Haste".into(), "Boost-STR".into(), "Aurorastorm".into()],
            cached_food, sub_job_level: 55, main_weapon_skill: SkillType::None, ranged_weapon_skill: SkillType::None,
            enhancing_skill_label: "Enhancing Skill:".into(), active_tab: 0,
            damage_limit: false, verbose_swaps: false, plot_dps: false, last_player: None, messages: Vec::new(),
        };
        if let Some(profiles) = profiles { state.load_profile(profiles, "default"); }
        else {
            if let Err(error) = state.update_job("main", None) { state.messages.push(error.to_string()); }
            if let Err(error) = state.update_job("sub", None) { state.messages.push(error.to_string()); }
            for slot in Slot::ALL {
                for source in GearSource::ALL {
                    if let Err(error) = state.select_gear(source, *slot, state.engine.catalog.empty) { state.messages.push(error.to_string()); }
                }
            }
        }
        state
    }

    pub fn text(&self, key: &str) -> &str {
        self.controls.get(key).and_then(|control| control.value.as_str()).unwrap_or("")
    }
    pub fn checked(&self, key: &str) -> bool {
        self.controls.get(key).is_some_and(|control| python_truth(&control.value))
    }

    pub fn set_raw(&mut self, key: &str, value: &Value) -> Result<(), CalcError> {
        let spec = input_spec(key).ok_or_else(|| CalcError::Input { operation: "input", field: "key", message: format!("Unknown input {key:?}") })?;
        let control = self.controls.get_mut(spec.key).expect("registered input initialized");
        match spec.kind {
            InputKind::Checkbox => control.value = Value::Bool(python_truth(value)),
            InputKind::Combo => {
                let text = legacy_text(value);
                if !text.is_empty() && !control.choices.contains(&text) { control.choices.push(text.clone()); }
                if control.choices.contains(&text) { control.value = Value::String(text); }
            }
            InputKind::Entry => control.value = Value::String(legacy_text(value)),
            InputKind::Label => {}
        }
        Ok(())
    }

    pub fn commit_tp_entry(&mut self) {
        let text = self.text("defaults_tp_entry");
        let value = parse_integer_text(text, "defaults_tp_entry");
        let replacement = match value {
            Ok(value) if value > 3000.0 => Some("3000"),
            Ok(value) if value >= 1000.0 => None,
            Err(CalcError::Legacy { category: "OverflowError", .. }) if !text.trim_start().starts_with('-') => Some("3000"),
            _ => Some("1000"),
        };
        if let Some(text) = replacement { self.set_text("defaults_tp_entry", text); }
    }

    pub fn wheel_tp(&mut self, direction: i32) {
        let text = self.text("defaults_tp_entry");
        let current = match parse_integer_text(text, "defaults_tp_entry") {
            Ok(value) => value,
            Err(CalcError::Legacy { category: "OverflowError", .. }) => {
                if text.trim_start().starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY }
            }
            Err(_) => 0.0,
        };
        let step = if direction > 0 { 100.0 } else { -100.0 };
        self.set_text("defaults_tp_entry", ((current + step).clamp(1000.0, 3000.0) as i64).to_string());
    }

    pub fn activate(&mut self, key: &str, value: &Value, profiles: Option<&ProfileStore>) -> Result<(), CalcError> {
        self.set_raw(key, value)?;
        match key {
            "defaults_mainjob_combobox" => self.update_job("main", profiles),
            "defaults_subjob_combobox" => self.update_job("sub", profiles),
            "defaults_ml_combobox" => self.update_job("master level", profiles),
            "defaults_enemy_combobox" => self.select_enemy(),
            "defaults_dia_combobox" => self.update_buffs("set dia"),
            "defaults_haste_combobox" => self.update_buffs("set haste"),
            "defaults_boost_combobox" => self.update_buffs("set boost"),
            "defaults_storm_combobox" => self.update_buffs("set storm"),
            "defaults_food_combobox" => self.update_buffs("set food"),
            "defaults_marcato_checkbox" => self.update_buffs("marcato"),
            "defaults_soul_voice_checkbox" => self.update_buffs("soul voice"),
            "defaults_bolster_checkbox" => self.update_buffs("bolster"),
            "defaults_bog_checkbox" => self.update_buffs("blaze of glory"),
            "defaults_Haste Samba_checkbox" => self.update_buffs("haste samba"),
            "defaults_Haste Samba (sub)_checkbox" => self.update_buffs("haste samba (sub)"),
            "defaults_Warcry_checkbox" => self.update_buffs("warcry"),
            "defaults_Warcry (sub)_checkbox" => self.update_buffs("warcry (sub)"),
            "defaults_Box Step_checkbox" => self.update_buffs("box step"),
            "defaults_Box Step (sub)_checkbox" => self.update_buffs("box step (sub)"),
            "defaults_Song1_combobox" => self.update_buffs("set Song1"),
            "defaults_Song2_combobox" => self.update_buffs("set Song2"),
            "defaults_Song3_combobox" => self.update_buffs("set Song3"),
            "defaults_Song4_combobox" => self.update_buffs("set Song4"),
            "defaults_Song5_combobox" => self.update_buffs("set Song5"),
            "defaults_Roll1_name_combobox" => self.update_buffs("set Roll1"),
            "defaults_Roll2_name_combobox" => self.update_buffs("set Roll2"),
            "defaults_Roll3_name_combobox" => self.update_buffs("set Roll3"),
            "defaults_Roll4_name_combobox" => self.update_buffs("set Roll4"),
            "defaults_Indi-_combobox" => self.update_buffs("set Indi-"),
            "defaults_Geo-_combobox" => self.update_buffs("set Geo-"),
            "defaults_Entrust-_combobox" => self.update_buffs("set Entrust-"),
            _ => Ok(()),
        }
    }

    fn set_text(&mut self, key: &'static str, text: impl Into<String>) {
        self.controls.get_mut(key).expect("registered input initialized").value = Value::String(text.into());
    }
    fn uncheck(&mut self, key: &'static str) {
        self.controls.get_mut(key).expect("registered input initialized").value = Value::Bool(false);
    }
    fn replace_choices(&mut self, key: &'static str, choices: Vec<String>) {
        let control = self.controls.get_mut(key).expect("registered input initialized");
        let current = control.value.as_str().unwrap_or("");
        if current.is_empty() || !choices.iter().any(|choice| choice == current) {
            control.value = Value::String(choices.first().cloned().unwrap_or_default());
        }
        control.choices = choices;
    }
    fn set_existing(&mut self, key: &'static str, text: &str) {
        let control = self.controls.get_mut(key).expect("registered input initialized");
        if control.choices.iter().any(|choice| choice == text) { control.value = Value::String(text.to_owned()); }
    }

    pub fn main_job(&self) -> Result<Job, CalcError> {
        let text = self.text("defaults_mainjob_combobox");
        Job::ALL.iter().copied().find(|job| job.display_name() == text).ok_or_else(|| CalcError::Input {
            operation: "input", field: "defaults_mainjob_combobox", message: format!("Unknown main job {text:?}"),
        })
    }
    pub fn sub_job(&self) -> Option<Job> {
        let text = self.text("defaults_subjob_combobox");
        Job::ALL.iter().copied().find(|job| job.display_name() == text)
    }
    pub fn integer(&self, key: &'static str) -> Result<f64, CalcError> {
        let text = self.text(key);
        parse_integer_text(if text.is_empty() { "0" } else { text }, key).map_err(|error| match error {
            CalcError::Legacy { category, message, .. } => CalcError::legacy("input", key, category, message),
            other => other,
        })
    }

    pub fn update_job(&mut self, trigger: &str, profiles: Option<&ProfileStore>) -> Result<(), CalcError> {
        let main = self.main_job()?;
        let sub = self.sub_job();
        if trigger == "master level" {
            let level = self.integer("defaults_ml_combobox")?;
            if level < i32::MIN as f64 || level > i32::MAX as f64 { return Err(CalcError::Input {
                operation: "input", field: "defaults_ml_combobox", message: "Master level is outside the supported integer range".into(),
            }); }
            self.sub_job_level = 49 + (level as i32).div_euclid(5);
        } else if trigger.contains("main") {
            self.replace_choices("defaults_spell_combobox", spell_menu(main).iter().map(|name| (*name).to_owned()).collect());
            if self.text("defaults_mainjob_combobox") == self.text("defaults_subjob_combobox") { self.set_existing("defaults_subjob_combobox", "None"); }
            let mut sub_choices = Job::ALL.iter().filter(|job| **job != main).map(|job| job.display_name().to_owned()).collect::<Vec<_>>();
            sub_choices.sort(); sub_choices.push("None".into());
            self.replace_choices("defaults_subjob_combobox", sub_choices);
            self.gear.refilter(&self.engine.catalog, main, sub, true);
            let old_boost = self.text("defaults_boost_combobox").rsplit('-').next().unwrap_or("None").to_owned();
            let prefix = if main == Job::Rdm { "Gain" } else { "Boost" };
            if old_boost != "None" { self.set_existing("defaults_boost_combobox", &format!("{prefix}-{old_boost}")); }
            let mut boosts = ["STR", "DEX", "VIT", "AGI", "MND", "INT", "CHR"].into_iter().map(|stat| format!("{prefix}-{stat}")).collect::<Vec<_>>();
            boosts.push("None".into()); self.replace_choices("defaults_boost_combobox", boosts);
            let (skill, label) = match main {
                Job::Rdm => ("650", "Enhancing Skill:"), Job::Run => ("570", self.enhancing_skill_label.as_str()),
                Job::Drk => ("600", "Dark Skill:"), Job::Pld => ("600", "Divine Skill:"), _ => ("500", "Enhancing Skill:"),
            };
            self.enhancing_skill_label = label.to_owned(); self.set_text("defaults_enhancing_skill_entry", skill);
            if !trigger.contains("static") && let Some(profiles) = profiles { self.load_profile(profiles, "main"); }
        } else if trigger == "sub" {
            self.gear.refilter(&self.engine.catalog, main, sub, false);
            if !can_dual_wield(main, sub) && self.engine.catalog.piece(self.gear.equipped[0][Slot::Sub.index()]).kind == PieceKind::Weapon {
                // The old subjob callback changes the Quicklook item but not its radio selection.
                self.gear.equipped[0][Slot::Sub.index()] = self.engine.catalog.empty;
            }
        }
        let main = self.main_job()?;
        let sub = self.sub_job();
        if self.text("defaults_subjob_combobox") != "None" && sub.is_none() { return Err(CalcError::Input {
            operation: "input", field: "defaults_subjob_combobox", message: format!("Unknown sub job {:?}", self.text("defaults_subjob_combobox")),
        }); }
        for spec in ABILITY_SPECS {
            let available = spec.jobs.contains(&main) || (self.sub_job_level >= spec.level && sub.is_some_and(|job| spec.jobs.contains(&job)));
            let suppressed_sub = (main == Job::Dnc && matches!(spec.ability, Ability::HasteSambaSub | Ability::BoxStepSub)) ||
                (main == Job::War && spec.ability == Ability::WarcrySub);
            let control = self.controls.get_mut(spec.key).expect("ability registry initialized");
            control.hidden = !available || suppressed_sub;
            if !available { control.value = Value::Bool(false); }
        }
        Ok(())
    }

    pub fn update_buffs(&mut self, event: &str) -> Result<(), CalcError> {
        match event {
            "soul voice" if self.checked("defaults_soul_voice_checkbox") => self.uncheck("defaults_marcato_checkbox"),
            "marcato" if self.checked("defaults_marcato_checkbox") => self.uncheck("defaults_soul_voice_checkbox"),
            "bolster" if self.checked("defaults_bolster_checkbox") => self.uncheck("defaults_bog_checkbox"),
            "blaze of glory" if self.checked("defaults_bog_checkbox") => self.uncheck("defaults_bolster_checkbox"),
            "set dia" => self.whm_selections[0] = self.text("defaults_dia_combobox").to_owned(),
            "set haste" => self.whm_selections[1] = self.text("defaults_haste_combobox").to_owned(),
            "set boost" => self.whm_selections[2] = self.text("defaults_boost_combobox").to_owned(),
            "set storm" => self.whm_selections[3] = self.text("defaults_storm_combobox").to_owned(),
            "set food" => {
                let name = self.text("defaults_food_combobox");
                let food = self.engine.catalog.food_by_name(name).ok_or_else(|| CalcError::legacy(
                    "update_buffs", "food", "KeyError", format!("{name:?}"),
                ))?;
                self.cached_food = Some(food);
            }
            _ => {
                const SONGS: [&str; 5] = ["defaults_Song1_combobox", "defaults_Song2_combobox", "defaults_Song3_combobox", "defaults_Song4_combobox", "defaults_Song5_combobox"];
                const ROLLS: [&str; 4] = ["defaults_Roll1_name_combobox", "defaults_Roll2_name_combobox", "defaults_Roll3_name_combobox", "defaults_Roll4_name_combobox"];
                const BUBBLES: [&str; 3] = ["defaults_Indi-_combobox", "defaults_Geo-_combobox", "defaults_Entrust-_combobox"];
                let duplicate = match event {
                    "set Song1" => Some((&SONGS[..], 0, false)), "set Song2" => Some((&SONGS[..], 1, false)),
                    "set Song3" => Some((&SONGS[..], 2, false)), "set Song4" => Some((&SONGS[..], 3, false)),
                    "set Song5" => Some((&SONGS[..], 4, false)), "set Roll1" => Some((&ROLLS[..], 0, false)),
                    "set Roll2" => Some((&ROLLS[..], 1, false)), "set Roll3" => Some((&ROLLS[..], 2, false)),
                    "set Roll4" => Some((&ROLLS[..], 3, false)), "set Indi-" => Some((&BUBBLES[..], 0, true)),
                    "set Geo-" => Some((&BUBBLES[..], 1, true)), "set Entrust-" => Some((&BUBBLES[..], 2, true)), _ => None,
                };
                if let Some((keys, selected, suffix)) = duplicate {
                    let new_name = self.text(keys[selected]);
                    let new_name = if suffix { new_name.rsplit('-').next().unwrap_or(new_name) } else { new_name }.to_owned();
                    for (index, &key) in keys.iter().enumerate() {
                        let old = self.text(key);
                        let old = if suffix { old.rsplit('-').next().unwrap_or(old) } else { old };
                        if index != selected && old != "None" && old == new_name { self.set_existing(key, "None"); }
                    }
                }
                for (main_key, sub_key, main_event, sub_event) in [
                    ("defaults_Haste Samba_checkbox", "defaults_Haste Samba (sub)_checkbox", "haste samba", "haste samba (sub)"),
                    ("defaults_Warcry_checkbox", "defaults_Warcry (sub)_checkbox", "warcry", "warcry (sub)"),
                    ("defaults_Box Step_checkbox", "defaults_Box Step (sub)_checkbox", "box step", "box step (sub)"),
                ] {
                    if event == main_event && self.checked(main_key) { self.uncheck(sub_key); }
                    if event == sub_event && self.checked(sub_key) { self.uncheck(main_key); }
                }
            }
        }
        Ok(())
    }

    pub fn select_enemy(&mut self) -> Result<(), CalcError> {
        let selected = self.text("defaults_enemy_combobox");
        let id = self.engine.catalog.enemy_by_name(selected).ok_or_else(|| CalcError::Input {
            operation: "input", field: "defaults_enemy_combobox", message: format!("Unknown enemy {selected:?}"),
        })?;
        let enemy = self.engine.catalog.enemy(id);
        let values = ENEMY_FIELDS.iter().map(|&(name, key)| {
            let stat = Stat::from_name(name).expect("literal enemy stat");
            let entry = enemy.stats.iter().find(|entry| entry.stat == stat).ok_or_else(|| CalcError::Input {
                operation: "enemy preset", field: key, message: format!("{} has no {name}", enemy.name),
            })?;
            Ok((key, legacy_text(&Value::Number(entry.literal.clone()))))
        }).collect::<Result<Vec<_>, CalcError>>()?;
        let label = format!("{} (Lv.{})", enemy.location, enemy.level);
        for (key, value) in values { self.set_text(key, value); }
        self.set_text("defaults_enemy_level_location_label", label);
        Ok(())
    }

    pub fn select_gear(&mut self, source: GearSource, slot: Slot, item: GearId) -> Result<(), CalcError> {
        let main = self.main_job()?;
        let dual = can_dual_wield(main, self.sub_job());
        let old = self.gear.equipped[source.index()][slot.index()];
        self.gear.equip(&self.engine.catalog, &self.engine.state, source, slot, item, dual);
        if source == GearSource::Quicklook && matches!(slot, Slot::Main | Slot::Ranged) {
            let old_skill = self.engine.state.skill(self.engine.catalog.piece(old));
            let new_skill = self.engine.state.skill(self.engine.catalog.piece(item));
            if slot == Slot::Main { self.main_weapon_skill = new_skill; } else { self.ranged_weapon_skill = new_skill; }
            let mut choices = ws_menu(self.main_weapon_skill).iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
            if !matches!(self.ranged_weapon_skill, SkillType::None | SkillType::Instrument) {
                choices.extend(ws_menu(self.ranged_weapon_skill).iter().map(|name| (*name).to_owned()));
            }
            self.replace_choices("defaults_ws_combobox", choices);
            if old_skill != new_skill && (slot != Slot::Ranged || new_skill != SkillType::Instrument) {
                let current = self.text("defaults_ws_combobox");
                let main_choices = ws_menu(self.main_weapon_skill);
                let ranged_choices = ws_menu(self.ranged_weapon_skill);
                if !main_choices.contains(&current) && (slot == Slot::Main || !ranged_choices.contains(&current)) {
                    self.set_existing("defaults_ws_combobox", main_choices[0]);
                }
            }
        }
        Ok(())
    }

    pub fn copy_set(&mut self, source: GearSource, destination: GearSource) {
        self.gear.copy_set(source, destination);
        self.active_tab = if destination == GearSource::Quicklook { 0 } else { 2 };
    }

    /// Source Equip best set uses direct assignment, not equipment callbacks.
    /// Constraints, WS menus and previously displayed stats are left untouched.
    pub fn equip_best_set(&mut self, best: Gearset) {
        self.gear.equipped[GearSource::Quicklook.index()] = best;
        for slot in Slot::ALL {
            self.gear.radios[GearSource::Quicklook.index()][slot.index()].set_selected(best[slot.index()]);
        }
        self.active_tab = 0;
    }

    pub fn reload_data(&mut self, paths: &AppPaths) -> Result<crate::exchange::ReloadReport, ResourceError> {
        let radio_names: [[Option<Arc<str>>; Slot::COUNT]; 3] = std::array::from_fn(|source|
            std::array::from_fn(|slot| self.gear.radios[source][slot].selected.map(|id| Arc::clone(&self.engine.catalog.piece(id).name2))));
        let food_name = self.cached_food.map(|id| Arc::clone(&self.engine.catalog.piece(id).name2));
        let mut candidates = self.gear.candidate_pools();
        let report = crate::exchange::reload_data(paths, &mut self.engine.catalog, &mut self.gear.equipped, &mut candidates, &mut self.engine.state)?;
        let mut models = GearModels::new(&self.engine.catalog);
        models.equipped = self.gear.equipped;
        models.visible_slots = self.gear.visible_slots;
        models.optimizer_slot = self.gear.optimizer_slot;
        if let Ok(main) = self.main_job() { models.refilter(&self.engine.catalog, main, self.sub_job(), true); }
        models.replace_candidates(&candidates);
        for source in GearSource::ALL {
            for slot in Slot::ALL {
                if let Some(name) = &radio_names[source.index()][slot.index()] &&
                    let Some(id) = self.engine.catalog.by_name2(name) {
                    models.radios[source.index()][slot.index()].set_selected(id);
                } else if report.removed_equipped.iter().any(|removed| removed.source == source && removed.slot == *slot) {
                    models.radios[source.index()][slot.index()].set_selected(self.engine.catalog.empty);
                }
            }
        }
        self.gear = models;
        self.cached_food = food_name.and_then(|name| self.engine.catalog.food_by_name(&name));
        self.last_player = None;
        self.refresh_reference_choices();
        Ok(report)
    }

    fn retain_selection_choices(&mut self, key: &'static str, mut choices: Vec<String>) {
        let current = self.text(key);
        if !current.is_empty() && !choices.iter().any(|choice| choice == current) { choices.push(current.to_owned()); }
        self.replace_choices(key, choices);
    }

    fn refresh_reference_choices(&mut self) {
        let enemies = self.engine.catalog.enemy_menu().iter().map(|&id| self.engine.catalog.enemy(id).name.to_string()).collect();
        self.retain_selection_choices("defaults_enemy_combobox", enemies);
        let mut foods = self.engine.catalog.foods().iter().map(|&id| self.engine.catalog.piece(id).name2.to_string()).collect::<Vec<_>>();
        foods.push("None".into()); self.retain_selection_choices("defaults_food_combobox", foods);
        let mut songs = self.engine.catalog.buffs().table(BuffTableKind::Brd).entries().iter().map(|entry| entry.name.to_string()).collect::<Vec<_>>();
        songs.push("None".into());
        for key in ["defaults_Song1_combobox", "defaults_Song2_combobox", "defaults_Song3_combobox", "defaults_Song4_combobox", "defaults_Song5_combobox"] {
            self.retain_selection_choices(key, songs.clone());
        }
        let mut rolls = self.engine.catalog.buffs().table(BuffTableKind::Cor).entries().iter().map(|entry| format!("{} Roll", entry.name)).collect::<Vec<_>>();
        rolls.push("None".into());
        for key in ["defaults_Roll1_name_combobox", "defaults_Roll2_name_combobox", "defaults_Roll3_name_combobox", "defaults_Roll4_name_combobox"] {
            self.retain_selection_choices(key, rolls.clone());
        }
        let mut bubbles = self.engine.catalog.buffs().table(BuffTableKind::Geo).entries().iter()
            .chain(self.engine.catalog.buffs().table(BuffTableKind::GeoDebuffs).entries()).map(|entry| entry.name.as_ref()).collect::<Vec<_>>();
        bubbles.sort();
        let choices: [Vec<String>; 3] = std::array::from_fn(|index| {
            let prefix = ["Indi-", "Geo-", "Entrust-"][index];
            let mut choices = bubbles.iter().map(|name| format!("{prefix}{name}")).collect::<Vec<_>>();
            choices.push("None".into()); choices
        });
        for (key, choices) in ["defaults_Indi-_combobox", "defaults_Geo-_combobox", "defaults_Entrust-_combobox"].into_iter().zip(choices) {
            self.retain_selection_choices(key, choices);
        }
    }

    pub fn load_profile(&mut self, profiles: &ProfileStore, reason: &str) {
        if let Err(error) = self.restore_profile(profiles, reason) { self.messages.push(error.to_string()); }
    }
    fn restore_profile(&mut self, profiles: &ProfileStore, reason: &str) -> Result<(), CalcError> {
        let selection = if reason == "default" { "default" } else { self.text("defaults_mainjob_combobox") };
        let Some(profile) = profiles.profile(selection) else { return Ok(()); };
        if profile.is_empty() { return Ok(()); }
        for spec in INPUT_REGISTRY {
            if let Some(value) = profile.get(spec.key) { self.set_raw(spec.key, value)?; }
        }
        for slot in Slot::ALL {
            for source in GearSource::ALL {
                let key = GEAR_PROFILE_KEYS[slot.index() * 3 + source.index()];
                if let Some(value) = profile.get(key) {
                    if let Some(name) = value.as_str() && let Some(item) = self.engine.catalog.by_name2(name) {
                        self.gear.equipped[source.index()][slot.index()] = item;
                    } else { self.messages.push(format!("Missing gear in {key}: {}", legacy_text(value))); }
                }
            }
        }
        for trigger in ["main", "sub", "master_level"] {
            if reason != trigger { self.update_job(trigger, Some(profiles))?; }
        }
        // The underscore spelling is source behavior: it does not refresh the
        // cached UI subjob level, unlike a direct master-level activation.
        for event in ["set dia", "set haste", "set boost", "set storm", "set Indi-", "set Geo-", "set Entrust-", "set food",
            "set Song1", "set Song2", "set Song3", "set Song4", "set Roll1", "set Roll2", "set Roll3", "set Roll4"] {
            self.update_buffs(event)?;
        }
        for slot in Slot::ALL {
            for source in GearSource::ALL {
                let item = self.gear.equipped[source.index()][slot.index()];
                self.select_gear(source, *slot, item)?;
            }
        }
        self.refresh_player(GearSource::Quicklook)?;
        Ok(())
    }

    pub fn effective_profile(&self) -> ProfileValues {
        let mut values = ProfileValues::new();
        for spec in INPUT_REGISTRY { values.insert(spec.key.to_owned(), self.controls[spec.key].value.clone()); }
        for slot in Slot::ALL {
            for source in GearSource::ALL {
                values.insert(GEAR_PROFILE_KEYS[slot.index() * 3 + source.index()].to_owned(),
                    Value::String(self.engine.catalog.piece(self.gear.equipped[source.index()][slot.index()]).name2.to_string()));
            }
        }
        values
    }

    fn ability_flags(&self, include_debuffs: bool) -> Abilities {
        let mut abilities = Abilities::default();
        for spec in ABILITY_SPECS {
            if include_debuffs || self.engine.catalog.buffs().table(BuffTableKind::MiscDebuffs).get(spec.name).is_none() {
                abilities.set(spec.ability, self.checked(spec.key));
            }
        }
        abilities
    }

    pub fn abilities(&self) -> Result<Abilities, CalcError> {
        let mut abilities = self.ability_flags(false);
        abilities.enhancing_skill = self.integer("defaults_enhancing_skill_entry")?;
        let aftermath = self.integer("defaults_aftermath_combobox")?;
        if aftermath < i64::MIN as f64 || aftermath >= 9223372036854775808.0 { return Err(CalcError::Input {
            operation: "input", field: "defaults_aftermath_combobox", message: "Aftermath is outside the supported integer range".into(),
        }); }
        abilities.set_aftermath_integer(aftermath as i64);
        abilities.set_storm(Some(Arc::from(self.whm_selections[3].as_str())));
        abilities.set_resist_rank(Arc::from(self.text("defaults_enemy_resist_rank_combobox")));
        abilities.set(Ability::DamageLimitDps, self.damage_limit);
        Ok(abilities)
    }

    pub fn buff_inputs(&self) -> BuffInputs {
        let text = |key| Arc::<str>::from(self.text(key));
        BuffInputs {
            brd_enabled: self.checked("defaults_brd_checkbox"),
            songs: [text("defaults_Song1_combobox"), text("defaults_Song2_combobox"), text("defaults_Song3_combobox"), text("defaults_Song4_combobox"), text("defaults_Song5_combobox")],
            song_bonus: text("defaults_song_bonus_combobox"), soul_voice: self.checked("defaults_soul_voice_checkbox"), marcato: self.checked("defaults_marcato_checkbox"),
            cor_enabled: self.checked("defaults_cor_checkbox"), rolls: [
                RollInput { name: text("defaults_Roll1_name_combobox"), potency: text("defaults_Roll1_potency_combobox") },
                RollInput { name: text("defaults_Roll2_name_combobox"), potency: text("defaults_Roll2_potency_combobox") },
                RollInput { name: text("defaults_Roll3_name_combobox"), potency: text("defaults_Roll3_potency_combobox") },
                RollInput { name: text("defaults_Roll4_name_combobox"), potency: text("defaults_Roll4_potency_combobox") },
            ],
            roll_bonus: text("defaults_roll_bonus_combobox"), crooked_cards: self.checked("defaults_crooked_cards_checkbox"),
            cor_job_bonus: self.checked("defaults_cor_job_bonus_checkbox"), light_shot: self.checked("defaults_light_shot_checkbox"),
            geo_enabled: self.checked("defaults_geo_checkbox"),
            bubbles: [text("defaults_Indi-_combobox"), text("defaults_Geo-_combobox"), text("defaults_Entrust-_combobox")],
            bubble_bonus: text("defaults_bubble_bonus_checkbox"), bubble_potency: text("defaults_bubble_potency_entry"),
            bolster: self.checked("defaults_bolster_checkbox"), blaze_of_glory: self.checked("defaults_bog_checkbox"),
            whm_enabled: self.checked("defaults_whm_checkbox"), whm_selections: std::array::from_fn(|index| Arc::from(self.whm_selections[index].as_str())),
            shell_v: self.checked("defaults_shell5_checkbox"), food_name: text("defaults_food_combobox"), miscellaneous: self.ability_flags(true),
        }
    }

    pub fn enemy_input(&self) -> Result<EnemyInput, CalcError> {
        let mut stats = Stats::default();
        for &(name, key) in ENEMY_FIELDS { stats.set(Stat::from_name(name).expect("literal enemy stat"), self.integer(key)?); }
        let name: Arc<str> = Arc::from(self.text("defaults_enemy_combobox"));
        let location = self.engine.catalog.enemy_by_name(&name).map(|id| Arc::clone(&self.engine.catalog.enemy(id).location)).unwrap_or_else(|| Arc::from(""));
        Ok(EnemyInput { name, location, stats })
    }

    pub fn calculation_inputs(&self, source: GearSource) -> Result<CalculationInputs, CalcError> {
        let main_job = self.main_job()?;
        let sub_job = self.sub_job();
        let master = self.integer("defaults_ml_combobox")?;
        if master < i32::MIN as f64 || master > i32::MAX as f64 { return Err(CalcError::Input {
            operation: "input", field: "defaults_ml_combobox", message: "Master level is outside the supported integer range".into(),
        }); }
        let ws_name: Arc<str> = Arc::from(self.text("defaults_ws_combobox"));
        let spell_name: Arc<str> = Arc::from(self.text("defaults_spell_combobox"));
        let tp = self.integer("defaults_tp_entry")?;
        let abilities = self.abilities()?;
        let enemy_input = self.enemy_input()?;
        let effects = Arc::new(aggregate_buffs(&self.buff_inputs(), &self.engine.catalog)?);
        let enemy = build_enemy(&enemy_input, &effects)?;
        let ranged = ws_menu(SkillType::Marksmanship).contains(&ws_name.as_ref()) || ws_menu(SkillType::Archery).contains(&ws_name.as_ref());
        Ok(CalculationInputs {
            player: PlayerInput { main_job, sub_job, master_level: master as i32,
                gearset: self.gear.equipped[source.index()], effects, abilities },
            enemy, ws_name, ws_type: if ranged { WsType::Ranged } else { WsType::Melee },
            spell_name, tp,
        })
    }

    pub fn refresh_player(&mut self, source: GearSource) -> Result<&Player, CalcError> {
        let input = self.calculation_inputs(source)?.player;
        let mut player = Player::new(input.gearset);
        build_player(&input, &mut self.engine, &mut player)?;
        self.last_player = Some(player);
        Ok(self.last_player.as_ref().expect("player just built"))
    }
}
