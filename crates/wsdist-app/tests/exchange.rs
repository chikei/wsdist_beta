use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use serde_json::{Value as JsonValue, json};
use wsdist_fixtures::{Map, Value, to_value};
use tempfile::TempDir;
use wsdist_app::exchange::{
    BulkAction, BulkFilters, GearSource, IconProvider, IconWarning, ImportOutcome,
    RemovedCandidate, RemovedEquipped, export_lua, import_inventory_file,
    parse_inventory, reload_data, select_bulk, select_inventory,
};
use wsdist_app::{AppPaths, ResourceError};
use wsdist_app::state::UiState;
use wsdist_core::model::{CandidatePools, ModelState, PieceKind, Player};
use wsdist_core::rng::RandomSources;
use wsdist_core::{CalcError, Catalog, GearId, Job, SkillType, Slot, Stat};

mod resource_support;

const REQUIRED_FILES: [&str; 4] = [
    "data/enemies.json5", "data/buffs.json5", "data/pup-attachments.json5", "item_list.csv",
];
const SLOT_NAMES: [&str; 16] = [
    "main", "sub", "ranged", "ammo", "head", "neck", "ear1", "ear2",
    "body", "hands", "ring1", "ring2", "back", "waist", "legs", "feet",
];
static CATALOG: LazyLock<Arc<Catalog>> = LazyLock::new(|| {
    Arc::new(Catalog::load(&repository()).unwrap())
});

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn resource_directory() -> (TempDir, AppPaths) {
    let directory = TempDir::new().unwrap();
    fs::create_dir(directory.path().join("data")).unwrap();
    for relative in REQUIRED_FILES {
        fs::copy(repository().join(relative), directory.path().join(relative)).unwrap();
    }
    resource_support::copy_gear(&repository().join("data/gear"), &directory.path().join("data/gear")).unwrap();
    let paths = AppPaths::from_parts(
        Some(directory.path().as_os_str()), Path::new("unused"), Path::new("unused"),
    ).unwrap();
    (directory, paths)
}

fn load_catalog(path: &Path) -> JsonValue {
    json5::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn save_catalog(path: &Path, value: &JsonValue) {
    // JSON is a JSON5 subset and preserves float literals such as 1.0 and -0.0.
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn fixture(id: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy");
    let index = wsdist_fixtures::read_json_value(&root.join("ui.json")).unwrap();
    assert_eq!(index["schema_version"], 3, "unsupported UI fixture index schema");
    let row = index["cases"].as_array().unwrap().iter()
        .find(|row| row["id"] == id).unwrap();
    let path = root.join(row["path"].as_str().unwrap());
    wsdist_fixtures::read_value(&path, Some(row["content_xxh3_128"].as_str().unwrap()))
        .unwrap_or_else(|error| panic!("{error}"))
}

fn empty_pools() -> CandidatePools {
    std::array::from_fn(|_| Vec::new())
}

fn piece(catalog: &Catalog, name: &str) -> GearId {
    catalog.by_name2(name).unwrap_or_else(|| panic!("missing gear {name:?}"))
}

fn named_pools(catalog: &Catalog, selected: &CandidatePools) -> Value {
    let mut result = Map::new();
    for &slot in Slot::ALL {
        result.insert(slot.name().to_owned(), to_value(selected[slot.index()].iter()
            .map(|id| catalog.piece(*id).name2.as_ref()).collect::<Vec<_>>()).unwrap());
    }
    Value::Object(result)
}

fn new_gear_fragment() -> JsonValue {
    json!({"schema_version": 2, "pieces": []})
}

fn add_piece(document: &mut JsonValue, name: &str, name2: &str, slot: Slot) {
    document["pieces"].as_array_mut().unwrap().push(json!({
        "name": name,
        "name2": name2,
        "jobs": Job::ALL.iter().map(|job| job.name()).collect::<Vec<_>>(),
        "slots": [slot.name()],
        "stats": {},
    }));
}

fn append_aliases(path: &Path, aliases: &[(&str, &str, &str)]) {
    let mut contents = fs::read_to_string(path).unwrap();
    for (id, full_name, abbreviation) in aliases {
        contents.push_str(&format!("{id};\"{}\";\"{}\"\n",
            full_name.replace('"', "\"\""), abbreviation.replace('"', "\"\"")));
    }
    fs::write(path, contents).unwrap();
}

fn lua_with_names(names: [&str; 16]) -> String {
    let mut output = "new_set = {\n".to_owned();
    for (slot, name) in SLOT_NAMES.into_iter().zip(names) {
        output.push_str(&format!("    {slot}=\"{name}\",\n"));
    }
    output.push('}');
    output
}

#[test]
fn exports_distinct_actual_sets_using_base_names_first_aliases_and_lua_escaping() {
    let (_directory, paths) = resource_directory();
    let mut gear = new_gear_fragment();
    add_piece(&mut gear, "exchange quick", "Quick augmented DA+10", Slot::Main);
    add_piece(&mut gear, "exchange tp", "TP augmented Store TP+10", Slot::Main);
    add_piece(&mut gear, "exchange ws", "WS augmented WSD+10", Slot::Main);
    add_piece(&mut gear, "exchange quoted", "Quoted augment", Slot::Sub);
    add_piece(&mut gear, "odd\"blade\\PATH", "Fallback augment", Slot::Neck);
    save_catalog(&paths.data.join("gear/custom/exchange.json5"), &gear);
    append_aliases(&paths.item_list, &[
        ("900001", "EXCHANGE QUICK", "FIRST QUICK"),
        ("900002", "exchange quick", "wrong quick"),
        ("900003", "exchange tp", "TP CHOICE"),
        ("900004", "exchange ws", "WS CHOICE"),
        ("900005", "exchange quoted", "\"BLADE\"\\PATH"),
    ]);
    let catalog = Catalog::load(&paths.root).unwrap();
    let mut sets = [[catalog.empty; 16]; 3];
    sets[0][Slot::Main.index()] = piece(&catalog, "Quick augmented DA+10");
    sets[0][Slot::Sub.index()] = piece(&catalog, "Quoted augment");
    sets[1][Slot::Main.index()] = piece(&catalog, "TP augmented Store TP+10");
    sets[1][Slot::Neck.index()] = piece(&catalog, "Fallback augment");
    sets[2][Slot::Main.index()] = piece(&catalog, "WS augmented WSD+10");

    let mut quick = ["Empty"; 16];
    quick[0] = "First Quick";
    quick[1] = "\\\"blade\\\"\\\\path";
    let mut tp = ["Empty"; 16];
    tp[0] = "Tp Choice";
    tp[5] = "Odd\\\"blade\\\\path";
    let mut ws = ["Empty"; 16];
    ws[0] = "Ws Choice";
    assert_eq!(export_lua(&catalog, &sets, GearSource::Quicklook), lua_with_names(quick));
    assert_eq!(export_lua(&catalog, &sets, GearSource::Tp), lua_with_names(tp));
    assert_eq!(export_lua(&catalog, &sets, GearSource::Ws), lua_with_names(ws));
}

#[test]
fn inventory_uses_first_abbreviated_alias_and_all_visible_base_name_variants() {
    let (_directory, paths) = resource_directory();
    let mut gear = new_gear_fragment();
    for (name, name2) in [
        ("Inventory Blade", "Inventory Blade R15A"),
        ("Inventory Blade +1", "Inventory Blade +1 R15A"),
        ("Other Blade", "Other Blade"),
    ] {
        add_piece(&mut gear, name, name2, Slot::Main);
    }
    save_catalog(&paths.data.join("gear/custom/exchange.json5"), &gear);
    append_aliases(&paths.item_list, &[
        ("900011", "Inventory Blade", "inv. blade"),
        ("900012", "Other Blade", "INV. BLADE"),
        ("900013", "Other Blade", "other blade"),
    ]);
    let catalog = Catalog::load(&paths.root).unwrap();
    let text = "main=\"INV. BLADE\", -- ignored \"other blade\"\n\
                quantity=99\n\
                main='other blade'\n\
                \"missing\" \"other blade\"\n\
                main=\"inv. blade\"\n\
                main=\"other blade\n";
    assert_eq!(parse_inventory(&catalog, text), ["Inventory Blade", "Inventory Blade"]);
    let mut visible = empty_pools();
    visible[0] = vec![
        piece(&catalog, "Inventory Blade +1 R15A"),
        piece(&catalog, "Inventory Blade R15A"),
        piece(&catalog, "Other Blade"),
    ];
    let selected = select_inventory(&catalog, &visible, &BulkFilters::default(), text);
    let mut expected = empty_pools();
    // The caller's model order, not the file's ordering or CSV's ordering.
    expected[0] = visible[0][..2].to_vec();
    assert_eq!(selected, expected);
}

fn filters_from_controls(controls: &Value) -> BulkFilters {
    BulkFilters {
        odyssey_rank: controls["defaults_ody_rank_combobox"].as_str().unwrap().to_owned(),
        nyame_r25_b: controls["defaults_nyame25_checkbox"].as_bool().unwrap(),
        soa_ring: controls["defaults_soa_ring_combobox"].as_str().unwrap().to_owned(),
        tvr_ring: controls["defaults_tvr_ring_combobox"].as_str().unwrap().to_owned(),
        mastery_rank: controls["defaults_mastery_rank_combobox"].as_str().unwrap().to_owned(),
    }
}

fn job_visible(catalog: &Catalog, job: Job) -> CandidatePools {
    let dual_wield = matches!(job, Job::Nin | Job::Dnc | Job::Thf | Job::Blu);
    std::array::from_fn(|index| {
        let slot = Slot::ALL[index];
        let names: BTreeSet<&str> = catalog.slot(slot).iter().map(|id| catalog.piece(*id))
            .filter(|gear| gear.eligible_for(job)
                && (slot != Slot::Sub || dual_wield
                    || matches!(gear.kind, PieceKind::Shield | PieceKind::Grip | PieceKind::None)))
            .map(|gear| gear.name2.as_ref()).collect();
        names.into_iter().map(|name| piece(catalog, name)).collect()
    })
}

#[test]
fn every_frozen_job_rank_ring_mastery_and_slot_bulk_action_matches_source() {
    let captured = fixture("ui.optimizer-bulk-filters");
    let rows = captured["expected"]["value"]["bulk_rows"].as_array().unwrap();
    let catalog = &CATALOG;
    let mut selected = empty_pools();
    let mut previous_job = None;
    let mut visible = empty_pools();
    for row in rows {
        let input = &row["input"];
        let job_name = input["job"].as_str().unwrap();
        let job = Job::ALL.iter().copied().find(|job| job.display_name() == job_name).unwrap();
        if previous_job != Some(job) {
            selected = empty_pools();
            visible = job_visible(catalog, job);
            previous_job = Some(job);
        }
        let filters = row.get("effective_filter_controls")
            .map(filters_from_controls).unwrap_or_default();
        let slot = input.get("slot").and_then(Value::as_str).and_then(Slot::from_name);
        let action = match input["event"].as_str().unwrap() {
            "select all" => BulkAction::SelectAll,
            "select all slot" => BulkAction::SelectSlot(slot.unwrap()),
            "unselect all slot" => BulkAction::UnselectSlot(slot.unwrap()),
            event => panic!("unexpected frozen bulk event {event}"),
        };
        selected = select_bulk(catalog, &visible, &selected, &filters, action);
        assert_eq!(named_pools(catalog, &selected), row["selected"], "bulk input {input}");
    }
}

#[test]
fn frozen_inventory_alias_import_matches_exact_full_model_selection() {
    let captured = fixture("ui.inventory.matching-aliases");
    let input = &captured["input"]["steps"][1]["contents"];
    let last = captured["expected"]["value"]["steps"].as_array().unwrap().last().unwrap();
    let frames = &last["optimizer_selection"];
    let visible = std::array::from_fn(|index| {
        frames[Slot::ALL[index].name()]["visible_data"].as_array().unwrap().iter()
            .map(|name| piece(&CATALOG, name.as_str().unwrap())).collect()
    });
    let selected = select_inventory(&CATALOG, &visible, &BulkFilters::default(), input.as_str().unwrap());
    for &slot in Slot::ALL {
        assert_eq!(to_value(selected[slot.index()].iter()
            .map(|id| CATALOG.piece(*id).name2.as_ref()).collect::<Vec<_>>()).unwrap(),
            frames[slot.name()]["selected"], "inventory slot {}", slot.name());
    }
}

#[test]
fn inventory_keeps_armor_and_neck_exceptions_but_not_nyame_rank_or_earring_exclusions() {
    let catalog = &CATALOG;
    let names = [
        (Slot::Head, "Hattori Zukin +2"),
        (Slot::Head, "Hattori Zukin +3"),
        (Slot::Head, "Nyame Helm R25B"),
        (Slot::Head, "Nyame Helm R30B"),
        (Slot::Neck, "Ninja Nodowa +1 R20"),
        (Slot::Ear1, "Hattori Earring +2"),
    ];
    let mut visible = empty_pools();
    let mut contents = String::new();
    for (slot, name) in names {
        let id = piece(catalog, name);
        visible[slot.index()].push(id);
        let alias = catalog.alias_by_full_name(&catalog.piece(id).name).unwrap();
        contents.push_str(&format!("\"{}\"\n", alias.name2));
    }
    let imported = select_inventory(catalog, &visible, &BulkFilters::default(), &contents);
    let bulk = select_bulk(catalog, &visible, &empty_pools(), &BulkFilters::default(), BulkAction::SelectAll);
    let mut expected_import = empty_pools();
    expected_import[Slot::Head.index()] = vec![piece(catalog, "Hattori Zukin +2"), piece(catalog, "Hattori Zukin +3")];
    expected_import[Slot::Neck.index()] = vec![piece(catalog, "Ninja Nodowa +1 R20")];
    let mut expected_bulk = empty_pools();
    expected_bulk[Slot::Head.index()] = vec![piece(catalog, "Hattori Zukin +3"), piece(catalog, "Nyame Helm R25B")];
    assert_eq!(imported, expected_import);
    assert_eq!(bulk, expected_bulk);
}

#[test]
fn cancelled_and_unreadable_imports_retain_hidden_checks_but_completed_empty_imports_clear() {
    let directory = TempDir::new().unwrap();
    let catalog = &CATALOG;
    let visible = job_visible(catalog, Job::Sch);
    let mut selected = empty_pools();
    selected[0] = vec![piece(catalog, "Heishi Shorinken R15"), piece(catalog, "Heishi Shorinken R15")];
    selected[Slot::Ring1.index()] = vec![catalog.empty];
    let before = selected.clone();
    assert_eq!(import_inventory_file(catalog, &visible, &BulkFilters::default(), &mut selected, None).unwrap(), ImportOutcome::Cancelled);
    assert_eq!(selected, before);
    let missing = directory.path().join("missing-inventory.lua");
    let error = import_inventory_file(catalog, &visible, &BulkFilters::default(), &mut selected, Some(&missing)).unwrap_err();
    assert_eq!(error.path, missing);
    assert_eq!(selected, before);
    let invalid = directory.path().join("non-utf8.lua");
    fs::write(&invalid, [0xff, 0xfe]).unwrap();
    let error = import_inventory_file(catalog, &visible, &BulkFilters::default(), &mut selected, Some(&invalid)).unwrap_err();
    assert_eq!(error.path, invalid);
    assert_eq!(selected, before);
    for contents in ["", "main=\"unknown inventory abbreviation\"\n"] {
        selected = before.clone();
        let file = directory.path().join("inventory.lua");
        fs::write(&file, contents).unwrap();
        assert_eq!(import_inventory_file(catalog, &visible, &BulkFilters::default(), &mut selected, Some(&file)).unwrap(), ImportOutcome::Completed);
        assert_eq!(selected, empty_pools());
    }
}


#[test]
fn reload_remaps_all_sets_preserves_candidate_duplicates_and_resets_identity_overrides() {
    let (_directory, paths) = resource_directory();
    let mut catalog = Arc::clone(&CATALOG);
    let old = Arc::clone(&catalog);
    let removed_weapon = piece(&catalog, "Heishi Shorinken R15");
    let removed_head = piece(&catalog, "Malignance Chapeau");
    let surviving_weapon = piece(&catalog, "Naegling");
    let other_weapon = piece(&catalog, "Daybreak");
    let mut sets = [[catalog.empty; 16]; 3];
    sets[0][Slot::Main.index()] = removed_weapon;
    sets[1][Slot::Main.index()] = surviving_weapon;
    sets[2][Slot::Main.index()] = other_weapon;
    sets[2][Slot::Head.index()] = removed_head;
    let before_sets = sets;
    let mut candidates = empty_pools();
    candidates[0] = vec![surviving_weapon, removed_weapon, surviving_weapon, other_weapon, removed_weapon];
    candidates[Slot::Head.index()] = vec![removed_head, catalog.empty];
    let mut model = ModelState::new(catalog.pieces().len());
    model.set_skill(catalog.empty, SkillType::HandToHand);
    model.set_skill(surviving_weapon, SkillType::Dagger);
    let generation = model.generation();

    for fragment in resource_support::gear_fragments(&paths.data.join("gear")).unwrap() {
        let mut gear = load_catalog(&fragment);
        let records = gear["pieces"].as_array_mut().unwrap();
        records.retain(|record| !matches!(resource_support::record_name2(record),
            Some("Heishi Shorinken R15" | "Malignance Chapeau")));
        records.reverse();
        if let Some(record) = records.iter_mut().find(|record|
            resource_support::record_name2(record) == Some("Naegling")) {
            record["stats"]["Attack"] = json!(999);
        }
        save_catalog(&fragment, &gear);
    }
    let mut enemies = load_catalog(&paths.data.join("enemies.json5"));
    let toad = enemies["enemies"].as_array_mut().unwrap().iter_mut()
        .find(|enemy| enemy["Name"] == "Apex Toad").unwrap();
    toad["Defense"] = json!(2345);
    save_catalog(&paths.data.join("enemies.json5"), &enemies);
    append_aliases(&paths.item_list, &[("900020", "Reload test alias", "reload abbreviation")]);

    let report = reload_data(&paths, &mut catalog, &mut sets, &mut candidates, &mut model).unwrap();
    assert!(!Arc::ptr_eq(&catalog, &old));
    assert_eq!(report.removed_equipped, vec![
        RemovedEquipped { source: GearSource::Quicklook, slot: Slot::Main, name2: "Heishi Shorinken R15".to_owned() },
        RemovedEquipped { source: GearSource::Ws, slot: Slot::Head, name2: "Malignance Chapeau".to_owned() },
    ]);
    assert_eq!(report.removed_candidates, vec![
        RemovedCandidate { slot: Slot::Main, name2: "Heishi Shorinken R15".to_owned() },
        RemovedCandidate { slot: Slot::Main, name2: "Heishi Shorinken R15".to_owned() },
        RemovedCandidate { slot: Slot::Head, name2: "Malignance Chapeau".to_owned() },
    ]);
    for source in GearSource::ALL {
        for &slot in Slot::ALL {
            let old_name = old.piece(before_sets[source.index()][slot.index()]).name2.as_ref();
            let expected = if matches!(old_name, "Heishi Shorinken R15" | "Malignance Chapeau") { "Empty" } else { old_name };
            assert_eq!(catalog.piece(sets[source.index()][slot.index()]).name2.as_ref(), expected);
        }
    }
    assert_eq!(candidates[0], vec![piece(&catalog, "Naegling"), piece(&catalog, "Naegling"), piece(&catalog, "Daybreak")]);
    assert_eq!(candidates[Slot::Head.index()], vec![catalog.empty]);
    assert_eq!(catalog.piece(piece(&catalog, "Naegling")).stat(Stat::from_name("Attack").unwrap()), 999.0);
    assert_eq!(catalog.alias_by_abbreviation("reload abbreviation").unwrap().id.as_ref(), "900020");
    assert_eq!(catalog.enemy(catalog.enemy_by_name("Apex Toad").unwrap())
        .stat(Stat::from_name("Defense").unwrap()), Some(2345.0));
    for piece in catalog.pieces() {
        assert_eq!(model.override_for(piece.id), None);
        assert_eq!(model.skill(piece), piece.skill_type);
    }
    assert_ne!(model.generation(), generation);
}

#[test]
fn any_invalid_required_resource_retains_every_reload_argument_and_reports_its_path() {
    let (_directory, paths) = resource_directory();
    let mut catalog = Arc::clone(&CATALOG);
    let old = Arc::clone(&catalog);
    let mut sets = [[catalog.empty; 16]; 3];
    sets[0][0] = piece(&catalog, "Heishi Shorinken R15");
    sets[1][0] = piece(&catalog, "Naegling");
    sets[2][0] = piece(&catalog, "Daybreak");
    let before_sets = sets;
    let mut candidates = empty_pools();
    candidates[0] = vec![sets[0][0], sets[1][0], sets[0][0]];
    let before_candidates = candidates.clone();
    let mut model = ModelState::new(catalog.pieces().len());
    model.set_skill(catalog.empty, SkillType::HandToHand);
    let generation = model.generation();
    let (fragment, _) = resource_support::piece_fragment(&paths.data.join("gear"), "Empty").unwrap();
    let files = REQUIRED_FILES.map(|relative| paths.root.join(relative)).into_iter().chain([fragment]);
    for file in files {
        let original = fs::read(&file).unwrap();
        fs::write(&file, if file.extension().is_some_and(|extension| extension == "csv") { "not;a;catalog\n" } else { "{" }).unwrap();
        match reload_data(&paths, &mut catalog, &mut sets, &mut candidates, &mut model) {
            Err(ResourceError::Catalog(CalcError::Data { path, .. })) => assert_eq!(path, file),
            Err(error) => panic!("unexpected error {error}"),
            Ok(_) => panic!("invalid {} was accepted", file.display()),
        }
        assert!(Arc::ptr_eq(&catalog, &old));
        assert_eq!(sets, before_sets);
        assert_eq!(candidates, before_candidates);
        assert_eq!(model.override_for(catalog.empty), Some(SkillType::HandToHand));
        assert_eq!(model.generation(), generation);
        fs::write(file, original).unwrap();
    }
}

#[test]
fn nested_custom_fragments_reload_atomically_and_remap_ui_selections_by_name() {
    let (_directory, paths) = resource_directory();
    let mut state = UiState::new(Arc::clone(&CATALOG), RandomSources::from_seed(11), None);
    let mut head = new_gear_fragment();
    add_piece(&mut head, "Reload custom head", "Reload custom head", Slot::Head);
    let head_file = paths.data.join("gear/custom/nested/head.json5");
    save_catalog(&head_file, &head);
    let mut waist = new_gear_fragment();
    add_piece(&mut waist, "Reload temporary waist", "Reload temporary waist", Slot::Waist);
    let waist_file = paths.data.join("gear/custom/temporary.json5");
    save_catalog(&waist_file, &waist);

    let report = state.reload_data(&paths).unwrap();
    assert!(report.removed_equipped.is_empty());
    assert!(report.removed_candidates.is_empty());
    let head = piece(&state.engine.catalog, "Reload custom head");
    let waist = piece(&state.engine.catalog, "Reload temporary waist");
    assert!(state.gear.radios[0][Slot::Head.index()].items.is_visible(head));
    assert!(state.gear.candidates[Slot::Head.index()].items.is_visible(head));
    for source in GearSource::ALL {
        state.select_gear(source, Slot::Head, head).unwrap();
    }
    state.select_gear(GearSource::Ws, Slot::Waist, waist).unwrap();
    state.gear.candidates[Slot::Head.index()].set_checked(head, true);
    state.gear.candidates[Slot::Waist.index()].set_checked(waist, true);
    state.gear.visible_slots = [Slot::Head, Slot::Waist, Slot::Feet];
    state.gear.optimizer_slot = Slot::Head;
    state.active_tab = 3;
    state.damage_limit = true;
    state.verbose_swaps = true;
    state.set_raw("defaults_tp_entry", &json!("1777")).unwrap();
    state.engine.state.set_skill(state.engine.catalog.empty, SkillType::HandToHand);
    state.last_player = Some(Player::new(state.gear.equipped[0]));

    let old_catalog = Arc::clone(&state.engine.catalog);
    let before_profile = state.effective_profile();
    let before_sets = state.gear.equipped;
    let before_candidates = state.gear.candidate_pools();
    let before_radios: [[Option<GearId>; 16]; 3] = std::array::from_fn(|source|
        std::array::from_fn(|slot| state.gear.radios[source][slot].selected));
    let before_choices = state.controls.iter().map(|(&key, control)|
        (key, control.choices.clone())).collect::<Vec<_>>();
    let before_food = state.cached_food;
    let food_name = before_food.map(|id| old_catalog.piece(id).name2.to_string());
    let generation = state.engine.state.generation();
    let session = (state.active_tab, state.gear.visible_slots, state.gear.optimizer_slot,
        state.damage_limit, state.verbose_swaps, state.whm_selections.clone(),
        state.sub_job_level, state.main_weapon_skill, state.ranged_weapon_skill);
    let bad_file = paths.data.join("gear/custom/nested/broken.json5");
    fs::write(&bad_file, "{broken").unwrap();
    match state.reload_data(&paths) {
        Err(ResourceError::Catalog(CalcError::Data { path, .. })) => assert_eq!(path, bad_file),
        Err(error) => panic!("unexpected reload error {error}"),
        Ok(_) => panic!("a malformed newly discovered fragment was accepted"),
    }
    assert!(Arc::ptr_eq(&state.engine.catalog, &old_catalog));
    assert_eq!(state.effective_profile(), before_profile);
    assert_eq!(state.gear.equipped, before_sets);
    assert_eq!(state.gear.candidate_pools(), before_candidates);
    for (source, radios) in before_radios.iter().enumerate() {
        for (slot, selected) in radios.iter().enumerate() {
            assert_eq!(state.gear.radios[source][slot].selected, *selected);
        }
    }
    assert_eq!(state.controls.iter().map(|(&key, control)|
        (key, control.choices.clone())).collect::<Vec<_>>(), before_choices);
    assert_eq!(state.cached_food, before_food);
    assert_eq!(state.last_player.as_ref().unwrap().gearset, before_sets[0]);
    assert_eq!(state.engine.state.override_for(state.engine.catalog.empty), Some(SkillType::HandToHand));
    assert_eq!(state.engine.state.generation(), generation);
    assert_eq!((state.active_tab, state.gear.visible_slots, state.gear.optimizer_slot,
        state.damage_limit, state.verbose_swaps, state.whm_selections.clone(),
        state.sub_job_level, state.main_weapon_skill, state.ranged_weapon_skill), session);

    fs::remove_file(bad_file).unwrap();
    let moved_head = paths.data.join("gear/00-remapped/custom-head.json5");
    fs::create_dir_all(moved_head.parent().unwrap()).unwrap();
    fs::rename(head_file, moved_head).unwrap();
    fs::remove_file(waist_file).unwrap();
    let report = state.reload_data(&paths).unwrap();
    assert!(!Arc::ptr_eq(&state.engine.catalog, &old_catalog));
    assert_eq!(report.removed_equipped, vec![RemovedEquipped {
        source: GearSource::Ws, slot: Slot::Waist, name2: "Reload temporary waist".to_owned(),
    }]);
    assert_eq!(report.removed_candidates, vec![RemovedCandidate {
        slot: Slot::Waist, name2: "Reload temporary waist".to_owned(),
    }]);
    let head = piece(&state.engine.catalog, "Reload custom head");
    for source in GearSource::ALL {
        assert_eq!(state.gear.equipped[source.index()][Slot::Head.index()], head);
        assert_eq!(state.gear.radios[source.index()][Slot::Head.index()].selected, Some(head));
        assert!(state.gear.radios[source.index()][Slot::Head.index()].items.is_visible(head));
    }
    assert_eq!(state.gear.equipped[GearSource::Ws.index()][Slot::Waist.index()], state.engine.catalog.empty);
    assert_eq!(state.gear.candidate_pools()[Slot::Head.index()], [head]);
    assert!(state.gear.candidate_pools()[Slot::Waist.index()].is_empty());
    assert!(state.engine.catalog.by_name2("Reload temporary waist").is_none());
    assert_eq!(state.text("defaults_tp_entry"), "1777");
    assert_eq!(state.cached_food.map(|id| state.engine.catalog.piece(id).name2.to_string()), food_name);
    assert!(state.last_player.is_none());
    assert_eq!(state.engine.state.override_for(state.engine.catalog.empty), None);
    assert_ne!(state.engine.state.generation(), generation);
    assert_eq!((state.active_tab, state.gear.visible_slots, state.gear.optimizer_slot,
        state.damage_limit, state.verbose_swaps, state.whm_selections.clone(),
        state.sub_job_level, state.main_weapon_skill, state.ranged_weapon_skill), session);
}

fn icon_catalog(paths: &AppPaths) -> Catalog {
    let mut gear = new_gear_fragment();
    add_piece(&mut gear, "Icon alias", "Icon alias augmented", Slot::Head);
    add_piece(&mut gear, "Shared icon alias", "Shared icon alias", Slot::Head);
    add_piece(&mut gear, "No icon alias", "No icon alias", Slot::Head);
    save_catalog(&paths.data.join("gear/custom/exchange.json5"), &gear);
    append_aliases(&paths.item_list, &[
        ("000100", "ICON ALIAS", "icon alias"),
        ("000101", "icon alias", "conflicting icon alias"),
        ("000100", "Shared icon alias", "shared icon alias"),
    ]);
    Catalog::load(&paths.root).unwrap()
}

#[test]
fn icons_use_first_full_name_csv_alias_and_load_real_assets() {
    let (_directory, paths) = resource_directory();
    let catalog = icon_catalog(&paths);
    fs::create_dir(&paths.icons).unwrap();
    let hat = paths.icons.join("000100.png");
    fs::copy(repository().join("icons32/23937.png"), &hat).unwrap();
    fs::copy(repository().join("icons32/65536.png"), paths.icons.join("000101.png")).unwrap();
    let mut icons = IconProvider::new(&paths);
    let id = piece(&catalog, "Icon alias augmented");
    let requested = icons.icon(&catalog, id);
    assert_eq!(requested.to_rgba8().unwrap().as_bytes(),
        slint::Image::load_from_path(&hat).unwrap().to_rgba8().unwrap().as_bytes());
    assert_ne!(requested.to_rgba8().unwrap().as_bytes(),
        slint::Image::load_from_path(&paths.icons.join("000101.png")).unwrap().to_rgba8().unwrap().as_bytes());
    assert!(icons.take_warnings().is_empty());
}

fn assert_blank_icon(image: slint::Image) {
    let size = image.size();
    assert_eq!((size.width, size.height), (0, 0));
    assert!(image.to_rgba8().is_none());
}

#[test]
fn missing_requested_icon_does_not_substitute_empty_and_reload_restores_it() {
    let (_directory, paths) = resource_directory();
    let catalog = icon_catalog(&paths);
    fs::create_dir(&paths.icons).unwrap();
    let empty = paths.icons.join("65536.png");
    fs::copy(repository().join("icons32/65536.png"), &empty).unwrap();
    let mut icons = IconProvider::new(&paths);
    assert_eq!(icons.icon(&catalog, catalog.empty).to_rgba8().unwrap().as_bytes(),
        slint::Image::load_from_path(&empty).unwrap().to_rgba8().unwrap().as_bytes());
    assert!(icons.take_warnings().is_empty());

    let id = piece(&catalog, "Icon alias augmented");
    let requested = paths.icons.join("000100.png");
    assert_blank_icon(icons.icon(&catalog, id));
    assert!(matches!(icons.take_warnings().as_slice(), [IconWarning::Asset { path, .. }]
        if path == &requested));
    assert_blank_icon(icons.icon(&catalog, id));
    assert_blank_icon(icons.icon(&catalog, piece(&catalog, "Shared icon alias")));
    assert!(icons.take_warnings().is_empty());

    fs::copy(repository().join("icons32/23937.png"), &requested).unwrap();
    assert_blank_icon(icons.icon(&catalog, id));
    assert!(icons.take_warnings().is_empty());
    icons.clear_cache();
    assert_eq!(icons.icon(&catalog, id).to_rgba8().unwrap().as_bytes(),
        slint::Image::load_from_path(&requested).unwrap().to_rgba8().unwrap().as_bytes());
    assert!(icons.take_warnings().is_empty());
}

#[test]
fn missing_alias_empty_icon_and_icon_directory_stay_blank_and_warn_once() {
    let (_directory, paths) = resource_directory();
    let catalog = icon_catalog(&paths);
    let mut icons = IconProvider::new(&paths);
    let id = piece(&catalog, "Icon alias augmented");
    assert_blank_icon(icons.icon(&catalog, id));
    assert!(matches!(icons.take_warnings().as_slice(), [IconWarning::Asset { path, .. }]
        if path == &paths.icons.join("000100.png")));
    assert_blank_icon(icons.icon(&catalog, id));
    assert_blank_icon(icons.icon(&catalog, piece(&catalog, "Shared icon alias")));
    assert!(icons.take_warnings().is_empty());

    let missing_alias = piece(&catalog, "No icon alias");
    assert_blank_icon(icons.icon(&catalog, missing_alias));
    assert_eq!(icons.take_warnings(), vec![IconWarning::MissingAlias { name: "No icon alias".to_owned() }]);
    assert_blank_icon(icons.icon(&catalog, missing_alias));
    assert!(icons.take_warnings().is_empty());

    assert_blank_icon(icons.icon(&catalog, catalog.empty));
    assert!(matches!(icons.take_warnings().as_slice(), [IconWarning::Asset { path, .. }]
        if path == &paths.icons.join("65536.png")));
    assert_blank_icon(icons.icon(&catalog, catalog.empty));
    assert!(icons.take_warnings().is_empty());
}

#[test]
fn undecodable_icon_stays_blank_and_clear_cache_retains_its_warning() {
    let (_directory, paths) = resource_directory();
    let catalog = icon_catalog(&paths);
    fs::create_dir(&paths.icons).unwrap();
    let requested = paths.icons.join("000100.png");
    fs::write(&requested, b"not a PNG").unwrap();
    let mut icons = IconProvider::new(&paths);
    let id = piece(&catalog, "Icon alias augmented");
    assert_blank_icon(icons.icon(&catalog, id));
    assert_blank_icon(icons.icon(&catalog, id));
    assert_blank_icon(icons.icon(&catalog, piece(&catalog, "Shared icon alias")));

    fs::copy(repository().join("icons32/23937.png"), &requested).unwrap();
    assert_blank_icon(icons.icon(&catalog, id));
    icons.clear_cache();
    assert_eq!(icons.icon(&catalog, id).to_rgba8().unwrap().as_bytes(),
        slint::Image::load_from_path(&requested).unwrap().to_rgba8().unwrap().as_bytes());
    assert!(matches!(icons.take_warnings().as_slice(), [IconWarning::Asset { path, message }]
        if path == &requested && !message.is_empty()));
    assert!(icons.take_warnings().is_empty());
}
