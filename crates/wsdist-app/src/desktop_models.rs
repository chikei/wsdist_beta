use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::Value;
use slint::{Model, ModelRc, SharedString, VecModel};
use wsdist_core::Catalog;
use wsdist_core::model::{GearId, GearPiece, Slot};

use crate::desktop_inputs::{EXTRA_INPUTS, INPUT_PRESENTATION, InputGroup, InputPresentation};
use crate::exchange::{GearSource, IconProvider};
use crate::profile_registry::InputKind;
use crate::profiles::legacy_text;
use crate::state::{InputControl, UiState};
use crate::{GearChoiceRow, GearSlotRow, InputRow, InputWidgetKind, MainWindow};

pub type ExtraControls = BTreeMap<&'static str, InputControl>;

pub fn extra_controls() -> ExtraControls {
    EXTRA_INPUTS.iter().map(|spec| (spec.presentation.key, InputControl {
        kind: spec.kind,
        value: spec.initial.to_value(),
        choices: spec.choices.iter().map(|text| (*text).to_owned()).collect(),
        enabled: true,
        hidden: false,
    })).collect()
}

pub struct DesktopModels {
    groups: [Rc<VecModel<InputRow>>; InputGroup::COUNT],
    controls: Vec<ControlRow>,
    slots: [Rc<VecModel<GearSlotRow>>; 4],
    items: [Rc<VecModel<GearChoiceRow>>; 4],
    shown_ids: [Arc<[GearId]>; 4],
    equipped_ids: [[Option<GearId>; Slot::COUNT]; 3],
    candidate_counts: [Option<usize>; Slot::COUNT],
    tooltips: HashMap<GearId, SharedString>,
    food_tooltip: Option<(GearId, SharedString)>,
    pub icons: IconProvider,
}

struct ControlRow {
    presentation: &'static InputPresentation,
    index: usize,
    choices: Rc<VecModel<SharedString>>,
}

impl DesktopModels {
    pub fn new(state: &UiState, extras: &ExtraControls, icons: IconProvider) -> Self {
        let groups: [Rc<VecModel<InputRow>>; InputGroup::COUNT] = std::array::from_fn(|_| Rc::new(VecModel::default()));
        let mut controls = Vec::with_capacity(INPUT_PRESENTATION.len() + EXTRA_INPUTS.len());
        for presentation in INPUT_PRESENTATION.iter().chain(EXTRA_INPUTS.iter().map(|spec| &spec.presentation)) {
            let control = state.controls.get(presentation.key).or_else(|| extras.get(presentation.key)).expect("literal input is registered");
            let choices = Rc::new(VecModel::from(control.choices.iter().map(|text| text.as_str().into()).collect::<Vec<_>>()));
            let group = &groups[presentation.group as usize];
            let index = group.row_count();
            group.push(make_input_row(presentation, control, choices.clone().into(), false, state));
            controls.push(ControlRow { presentation, index, choices });
        }
        Self {
            groups, controls,
            slots: std::array::from_fn(|_| Rc::new(VecModel::default())),
            items: std::array::from_fn(|_| Rc::new(VecModel::default())),
            shown_ids: std::array::from_fn(|_| Arc::from([])),
            equipped_ids: [[None; Slot::COUNT]; 3], candidate_counts: [None; Slot::COUNT],
            tooltips: HashMap::new(),
            food_tooltip: state.cached_food.map(|id| (id, gear_tooltip(state.engine.catalog.piece(id)).into())),
            icons,
        }
    }

    pub fn bind(&self, window: &MainWindow) {
        window.set_basic_controls(self.groups[InputGroup::Basic as usize].clone().into());
        window.set_ability_controls(self.groups[InputGroup::Abilities as usize].clone().into());
        window.set_enemy_controls(self.groups[InputGroup::Enemy as usize].clone().into());
        window.set_whm_controls(self.groups[InputGroup::Whm as usize].clone().into());
        window.set_food_controls(self.groups[InputGroup::Food as usize].clone().into());
        window.set_brd_controls(self.groups[InputGroup::Brd as usize].clone().into());
        window.set_cor_controls(self.groups[InputGroup::Cor as usize].clone().into());
        window.set_geo_controls(self.groups[InputGroup::Geo as usize].clone().into());
        window.set_optimize_controls(self.groups[InputGroup::Optimize as usize].clone().into());
        window.set_simulate_controls(self.groups[InputGroup::Simulate as usize].clone().into());
        window.set_quick_slots(self.slots[0].clone().into());
        window.set_tp_slots(self.slots[1].clone().into());
        window.set_ws_slots(self.slots[2].clone().into());
        window.set_optimize_slots(self.slots[3].clone().into());
        window.set_quick_items(self.items[0].clone().into());
        window.set_tp_items(self.items[1].clone().into());
        window.set_ws_items(self.items[2].clone().into());
        window.set_optimize_items(self.items[3].clone().into());
    }

    pub fn sync_controls(&mut self, state: &UiState, extras: &ExtraControls, busy: bool) {
        if self.food_tooltip.as_ref().map(|(id, _)| *id) != state.cached_food {
            self.food_tooltip = state.cached_food.map(|id| (id, gear_tooltip(state.engine.catalog.piece(id)).into()));
        }
        for binding in &self.controls {
            let presentation = binding.presentation;
            let control = state.controls.get(presentation.key).or_else(|| extras.get(presentation.key)).expect("literal input is registered");
            let choices_changed = binding.choices.row_count() != control.choices.len() || control.choices.iter().enumerate().any(|(index, text)| {
                binding.choices.row_data(index).is_none_or(|old| old.as_str() != text)
            });
            if choices_changed {
                binding.choices.set_vec(control.choices.iter().map(|text| text.as_str().into()).collect::<Vec<_>>());
            }
            let group = &self.groups[presentation.group as usize];
            let mut row = group.row_data(binding.index).expect("input row initialized");
            let text = control.value.as_str().unwrap_or("");
            let label = if presentation.key == "defaults_enhancing_skill_entry" { state.enhancing_skill_label.as_str() } else { presentation.label };
            let checked = wsdist_core::model::python_truth(&control.value);
            let tooltip = if presentation.key == "defaults_food_combobox" {
                self.food_tooltip.as_ref().map_or("", |(_, text)| text.as_str())
            } else { presentation.tooltip };
            let selection = control.choices.iter().position(|choice| choice == text).map_or(-1, |index| index as i32);
            let enabled = control.enabled && !busy;
            let visible = !control.hidden;
            if row.text.as_str() != text || row.label.as_str() != label || row.checked != checked ||
                row.selection != selection || row.enabled != enabled || row.visible != visible || row.tooltip.as_str() != tooltip || choices_changed {
                if row.text.as_str() != text { row.text = text.into(); }
                if row.label.as_str() != label { row.label = label.into(); }
                if row.tooltip.as_str() != tooltip { row.tooltip = tooltip.into(); }
                row.checked = checked;
                row.selection = selection;
                row.enabled = enabled;
                row.visible = visible;
                group.set_row_data(binding.index, row);
            }
        }
    }

    pub fn sync_gear(&mut self, state: &UiState, window: &MainWindow) {
        let catalog = &state.engine.catalog;
        for source in GearSource::ALL {
            let index = source.index();
            for slot in Slot::ALL {
                let id = state.gear.equipped[index][slot.index()];
                if self.equipped_ids[index][slot.index()] == Some(id) { continue; }
                let piece = catalog.piece(id);
                let tooltip = self.tooltip(piece);
                let row = GearSlotRow { slot: slot.index() as i32, label: slot.name().into(), name: piece.name2.as_ref().into(), icon: self.icons.icon(catalog, id), tooltip };
                replace_row(&self.slots[index], slot.index(), row);
                self.equipped_ids[index][slot.index()] = Some(id);
            }
            let active = state.gear.visible_slots[index];
            let radio = &state.gear.radios[index][active.index()];
            self.sync_items(index, &radio.items.visible, catalog, |id| radio.selected == Some(id));
        }
        for slot in Slot::ALL {
            let count = state.gear.candidates[slot.index()].selected_count();
            if self.candidate_counts[slot.index()] == Some(count) { continue; }
            let row = GearSlotRow {
                slot: slot.index() as i32, label: slot.name().into(), name: format!("{count} selected").into(),
                icon: self.icons.icon(catalog, catalog.empty), tooltip: format!("{}: {count} checked candidates, including hidden selections", slot.name()).into(),
            };
            replace_row(&self.slots[3], slot.index(), row);
            self.candidate_counts[slot.index()] = Some(count);
        }
        let active = state.gear.optimizer_slot;
        let candidate = &state.gear.candidates[active.index()];
        self.sync_items(3, &candidate.items.visible, catalog, |id| candidate.is_checked(id));
        window.set_quick_active_slot(state.gear.visible_slots[0].index() as i32);
        window.set_tp_active_slot(state.gear.visible_slots[1].index() as i32);
        window.set_ws_active_slot(state.gear.visible_slots[2].index() as i32);
        window.set_optimize_active_slot(active.index() as i32);
    }

    fn sync_items(&mut self, index: usize, ids: &Arc<[GearId]>, catalog: &Catalog, checked: impl Fn(GearId) -> bool) {
        if !Arc::ptr_eq(&self.shown_ids[index], ids) {
            let rows: Vec<_> = ids.iter().map(|&id| GearChoiceRow {
                id: id.index() as i32, name: catalog.piece(id).name2.as_ref().into(),
                checked: checked(id), tooltip: self.tooltip(catalog.piece(id)),
            }).collect();
            self.items[index].set_vec(rows);
            self.shown_ids[index] = Arc::clone(ids);
        } else {
            for (row_index, &id) in ids.iter().enumerate() {
                let new_checked = checked(id);
                if let Some(mut row) = self.items[index].row_data(row_index) && row.checked != new_checked {
                    row.checked = new_checked;
                    self.items[index].set_row_data(row_index, row);
                }
            }
        }
    }

    fn tooltip(&mut self, piece: &GearPiece) -> SharedString {
        self.tooltips.entry(piece.id).or_insert_with(|| gear_tooltip(piece).into()).clone()
    }

    pub fn reset_catalog(&mut self) {
        self.icons.clear_cache();
        self.tooltips.clear();
        self.food_tooltip = None;
        self.equipped_ids = [[None; Slot::COUNT]; 3];
        self.candidate_counts = [None; Slot::COUNT];
        self.shown_ids = std::array::from_fn(|_| Arc::from([]));
    }
}

fn replace_row<T: Clone + 'static>(model: &VecModel<T>, index: usize, row: T) {
    if index < model.row_count() { model.set_row_data(index, row); } else { model.push(row); }
}

fn make_input_row(presentation: &InputPresentation, control: &InputControl, choices: ModelRc<SharedString>, busy: bool, state: &UiState) -> InputRow {
    let text = control.value.as_str().unwrap_or("");
    InputRow {
        key: presentation.key.into(),
        label: if presentation.key == "defaults_enhancing_skill_entry" { state.enhancing_skill_label.as_str().into() } else { presentation.label.into() },
        control_kind: match control.kind { InputKind::Combo => InputWidgetKind::Combo, InputKind::Checkbox => InputWidgetKind::Checkbox, InputKind::Entry => InputWidgetKind::Entry, InputKind::Label => InputWidgetKind::Label },
        text: text.into(), checked: wsdist_core::model::python_truth(&control.value),
        choices, selection: control.choices.iter().position(|choice| choice == text).map_or(-1, |index| index as i32),
        enabled: control.enabled && !busy, visible: !control.hidden, tooltip: presentation.tooltip.into(),
    }
}

pub fn gear_tooltip(piece: &GearPiece) -> String {
    const BASE: &[&str] = &["STR", "DEX", "VIT", "AGI", "INT", "MND", "CHR"];
    const MAIN: &[&str] = &["Accuracy", "Attack", "Ranged Accuracy", "Ranged Attack", "Magic Accuracy", "Magic Damage", "Magic Attack"];
    const DEF: &[&str] = &["Evasion", "Magic Evasion", "Magic Defense", "DT", "MDT", "PDT", "MDT2", "PDT2", "Subtle Blow", "Subtle Blow II"];
    let mut tooltip = String::new();
    let mut newline = false;
    for (key, value) in [("DMG", piece.dmg), ("Delay", piece.delay)] {
        if value != 0.0 { write!(tooltip, "{key}:{value},").expect("String formatting"); newline = true; }
        if key == "Delay" && newline { tooltip.push('\n'); }
    }
    newline = false;
    for key in BASE {
        if let Some(entry) = piece.stats.iter().find(|entry| entry.stat.name() == *key) && entry.value != 0.0 {
            write!(tooltip, "{key}:{},", legacy_text(&Value::Number(entry.literal.clone()))).expect("String formatting"); newline = true;
        }
        if *key == "CHR" && newline { tooltip.push('\n'); }
    }
    newline = false;
    for key in MAIN {
        if let Some(entry) = piece.stats.iter().find(|entry| entry.stat.name() == *key) && entry.value != 0.0 {
            write!(tooltip, "{key}:{},", legacy_text(&Value::Number(entry.literal.clone()))).expect("String formatting"); newline = true;
        }
        if key.contains("Attack") && newline { tooltip.push('\n'); newline = false; }
    }
    for entry in &piece.stats {
        let key = entry.stat.name();
        if BASE.contains(&key) || MAIN.contains(&key) || DEF.contains(&key) { continue; }
        writeln!(tooltip, "{key}:{}", legacy_text(&Value::Number(entry.literal.clone()))).expect("String formatting");
    }
    newline = false;
    for key in DEF {
        if let Some(entry) = piece.stats.iter().find(|entry| entry.stat.name() == *key) && entry.value != 0.0 {
            write!(tooltip, "{key}:{},", legacy_text(&Value::Number(entry.literal.clone()))).expect("String formatting"); newline = true;
        }
        if key.contains("Def") && newline { tooltip.push('\n'); newline = false; }
    }
    let end = tooltip.trim_end().len();
    tooltip.truncate(end);
    let start = tooltip.len() - tooltip.trim_start().len();
    if start != 0 { tooltip.drain(..start); }
    tooltip
}
