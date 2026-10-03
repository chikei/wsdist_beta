use std::collections::HashSet;
use std::sync::Arc;

use wsdist_core::Catalog;
use wsdist_core::model::{CandidatePools, GearId, Gearset, Job, ModelState, PieceKind, SkillType, Slot};

use crate::exchange::GearSource;

#[derive(Clone, Debug)]
pub struct SlotModel {
    pub master: Arc<[GearId]>,
    pub unique: Arc<[GearId]>,
    pub visible: Arc<[GearId]>,
    members: Arc<HashSet<GearId>>,
}

impl SlotModel {
    fn new(catalog: &Catalog, slot: Slot) -> Self {
        let mut master = catalog.slot(slot).iter().map(|&id| {
            catalog.by_name2(&catalog.piece(id).name2).expect("catalog slot names are indexed")
        }).collect::<Vec<_>>();
        master.sort_by(|&a, &b| catalog.piece(a).name2.cmp(&catalog.piece(b).name2));
        let mut unique = master.clone();
        unique.dedup();
        let members = Arc::new(unique.iter().copied().collect());
        let master: Arc<[GearId]> = master.into();
        Self { visible: Arc::clone(&master), master, unique: unique.into(), members }
    }

    pub fn contains(&self, id: GearId) -> bool { self.members.contains(&id) }
    pub fn is_visible(&self, id: GearId) -> bool { self.visible.contains(&id) }
}

#[derive(Clone, Debug)]
pub struct RadioModel { pub items: SlotModel, pub selected: Option<GearId> }

impl RadioModel {
    pub fn set_selected(&mut self, id: GearId) {
        if self.items.contains(id) { self.selected = Some(id); }
    }
    pub fn visible_selection(&self) -> Option<GearId> {
        self.selected.filter(|&id| self.items.is_visible(id))
    }
}

#[derive(Clone, Debug)]
pub struct CheckboxModel { pub items: SlotModel, checked: HashSet<GearId> }

impl CheckboxModel {
    pub fn is_checked(&self, id: GearId) -> bool { self.checked.contains(&id) }
    pub fn set_checked(&mut self, id: GearId, checked: bool) {
        if !self.items.contains(id) { return; }
        if checked { self.checked.insert(id); } else { self.checked.remove(&id); }
    }
    pub fn clear(&mut self) { self.checked.clear(); }
    pub fn selected_count(&self) -> usize { self.checked.len() }
    pub fn selected(&self) -> Vec<GearId> {
        self.items.unique.iter().copied().filter(|id| self.checked.contains(id)).collect()
    }
}

#[derive(Clone, Debug)]
pub struct GearModels {
    pub equipped: [Gearset; 3],
    pub radios: [[RadioModel; Slot::COUNT]; 3],
    pub candidates: [CheckboxModel; Slot::COUNT],
    pub visible_slots: [Slot; 3],
    pub optimizer_slot: Slot,
}

impl GearModels {
    pub fn new(catalog: &Catalog) -> Self {
        let models: [SlotModel; Slot::COUNT] = std::array::from_fn(|index| SlotModel::new(catalog, Slot::ALL[index]));
        Self {
            equipped: [[catalog.empty; Slot::COUNT]; 3],
            radios: std::array::from_fn(|_| std::array::from_fn(|index| RadioModel { items: models[index].clone(), selected: None })),
            candidates: std::array::from_fn(|index| CheckboxModel { items: models[index].clone(), checked: HashSet::new() }),
            visible_slots: [Slot::Main; 3], optimizer_slot: Slot::Main,
        }
    }

    pub fn candidate_pools(&self) -> CandidatePools { std::array::from_fn(|index| self.candidates[index].selected()) }
    pub fn visible_candidates(&self) -> CandidatePools { std::array::from_fn(|index| self.candidates[index].items.visible.to_vec()) }

    pub fn replace_candidates(&mut self, pools: &CandidatePools) {
        for (model, selected) in self.candidates.iter_mut().zip(pools) {
            model.clear();
            for &id in selected { model.set_checked(id, true); }
        }
    }

    pub fn refilter(&mut self, catalog: &Catalog, main: Job, sub: Option<Job>, main_change: bool) {
        let dual = can_dual_wield(main, sub);
        for slot in Slot::ALL {
            if !main_change && *slot != Slot::Sub { continue; }
            // Filter the source records before resolving GUI name2 aliases: duplicate
            // names can have different jobs while the GUI's lookup still selects the last record.
            let mut visible = catalog.slot(*slot).iter().copied().filter(|&id| {
                let piece = catalog.piece(id);
                piece.eligible_for(main) && (*slot != Slot::Sub ||
                    matches!(piece.kind, PieceKind::Shield | PieceKind::Grip | PieceKind::None) ||
                    (dual && piece.kind == PieceKind::Weapon))
            }).map(|id| catalog.by_name2(&catalog.piece(id).name2).expect("catalog slot names are indexed"))
                .filter(|id| self.candidates[slot.index()].items.contains(*id)).collect::<Vec<_>>();
            visible.sort_by(|&a, &b| catalog.piece(a).name2.cmp(&catalog.piece(b).name2));
            let visible: Arc<[GearId]> = visible.into();
            for radios in &mut self.radios { radios[slot.index()].items.visible = Arc::clone(&visible); }
            let candidates = &mut self.candidates[slot.index()];
            candidates.items.visible = visible;
            if main_change { candidates.clear(); }
            else if !dual {
                for &id in catalog.slot(Slot::Sub) {
                    let piece = catalog.piece(id);
                    if piece.eligible_for(main) && piece.kind == PieceKind::Weapon {
                        if let Some(id) = catalog.by_name2(&piece.name2) { candidates.set_checked(id, false); }
                    }
                }
            }
        }
    }

    pub fn equip(&mut self, catalog: &Catalog, state: &ModelState, source: GearSource, slot: Slot, item: GearId, dual: bool) {
        let index = source.index();
        let before = self.equipped[index];
        self.equipped[index][slot.index()] = item;
        let main_skill = state.skill(catalog.piece(self.equipped[index][Slot::Main.index()]));
        let sub_kind = catalog.piece(self.equipped[index][Slot::Sub.index()]).kind;
        let one_handed = matches!(main_skill, SkillType::Axe | SkillType::Club | SkillType::Dagger | SkillType::Sword | SkillType::Katana);
        if slot == Slot::Main && ((is_two_handed(main_skill) && !matches!(sub_kind, PieceKind::Grip | PieceKind::None)) ||
            main_skill == SkillType::HandToHand || (one_handed && !matches!(sub_kind, PieceKind::Shield | PieceKind::None) && !dual)) {
            self.clear_slot(catalog, source, Slot::Sub);
        }
        if slot == Slot::Sub && ((sub_kind == PieceKind::Grip && !is_two_handed(main_skill)) ||
            (sub_kind == PieceKind::Shield && !one_handed && main_skill != SkillType::None)) {
            self.clear_slot(catalog, source, Slot::Main);
        }
        let ranged_kind = catalog.piece(self.equipped[index][Slot::Ranged.index()]).kind;
        let ammo_kind = catalog.piece(self.equipped[index][Slot::Ammo.index()]).kind;
        if slot == Slot::Ranged && ((ranged_kind == PieceKind::Gun && ammo_kind != PieceKind::Bullet) ||
            (ranged_kind == PieceKind::Bow && ammo_kind != PieceKind::Arrow) ||
            (ranged_kind == PieceKind::Crossbow && ammo_kind != PieceKind::Bolt) ||
            matches!(ranged_kind, PieceKind::Instrument | PieceKind::Equipment)) {
            self.clear_slot(catalog, source, Slot::Ammo);
        }
        if slot == Slot::Ammo && ((ammo_kind == PieceKind::Bullet && ranged_kind != PieceKind::Gun) ||
            (ammo_kind == PieceKind::Arrow && ranged_kind != PieceKind::Bow) ||
            (ammo_kind == PieceKind::Bolt && ranged_kind != PieceKind::Crossbow) ||
            matches!(ammo_kind, PieceKind::Equipment | PieceKind::Shuriken)) {
            self.clear_slot(catalog, source, Slot::Ranged);
        }
        for (first, second) in [(Slot::Ring1, Slot::Ring2), (Slot::Ear1, Slot::Ear2)] {
            let other = if slot == first { Some(second) } else if slot == second { Some(first) } else { None };
            if let Some(other) = other && catalog.piece(item).name.as_ref() != "Empty" &&
                catalog.piece(item).value_equal(catalog.piece(before[other.index()]), state) {
                self.equipped[index][first.index()] = before[second.index()];
                self.equipped[index][second.index()] = before[first.index()];
                self.radios[index][first.index()].set_selected(before[second.index()]);
                self.radios[index][second.index()].set_selected(before[first.index()]);
            }
        }
        if slot == Slot::Body && contains_cloak(&catalog.piece(item).name2) {
            self.clear_slot(catalog, source, Slot::Head);
        }
        if slot == Slot::Head && contains_cloak(&catalog.piece(self.equipped[index][Slot::Body.index()]).name) {
            self.clear_slot(catalog, source, Slot::Body);
        }
        self.radios[index][slot.index()].set_selected(item);
    }

    pub fn clear_slot(&mut self, catalog: &Catalog, source: GearSource, slot: Slot) {
        self.equipped[source.index()][slot.index()] = catalog.empty;
        self.radios[source.index()][slot.index()].set_selected(catalog.empty);
    }

    pub fn copy_set(&mut self, source: GearSource, destination: GearSource) {
        self.equipped[destination.index()] = self.equipped[source.index()];
        for slot in Slot::ALL {
            self.radios[destination.index()][slot.index()].set_selected(self.equipped[source.index()][slot.index()]);
        }
    }
}

pub fn can_dual_wield(main: Job, sub: Option<Job>) -> bool {
    matches!(main, Job::Nin | Job::Dnc | Job::Thf | Job::Blu) || matches!(sub, Some(Job::Nin | Job::Dnc))
}

fn is_two_handed(skill: SkillType) -> bool {
    matches!(skill, SkillType::GreatSword | SkillType::GreatKatana | SkillType::GreatAxe | SkillType::Polearm | SkillType::Scythe | SkillType::Staff)
}

fn contains_cloak(value: &str) -> bool {
    let needle = ['c', 'l', 'o', 'a', 'k'];
    let mut matched = 0;
    for ch in value.chars().flat_map(char::to_lowercase) {
        matched = if ch == needle[matched] { matched + 1 } else { usize::from(ch == needle[0]) };
        if matched == needle.len() { return true; }
    }
    false
}
