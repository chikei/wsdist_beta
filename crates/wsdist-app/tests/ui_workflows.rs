use std::collections::BTreeMap;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model};
use wsdist_app::{InputRow, InputWidgetKind};
use wsdist_app::controller::Desktop;
use wsdist_app::exchange::GearSource;
use wsdist_app::jobs::WorkResult;
use wsdist_app::resources::{Resources, AppPaths};
use wsdist_core::model::Slot;
use wsdist_core::rng::RandomSources;

mod ui_support;

slint::slint! {
    import { ListView } from "../ui/scroll.slint";
    export component EstimatedListHarness inherits Window {
        in property <[int]> row-heights;
        in property <length> view-height: 80px;
        out property <length> content-height: list.viewport-height;
        out property <length> scroll-offset: list.viewport-y;
        out property <length> content-width: list.visible-width;
        width: 200px;
        height: 100px;
        list := ListView {
            x: 0px;
            y: 0px;
            width: 200px;
            height: root.view-height;
            viewport-width: self.visible-width;
            scroll-label: "Estimated rows vertical scrollbar";
            for row-height in root.row-heights: Rectangle {
                width: list.visible-width;
                height: row-height * 1px;
            }
        }
    }
}

fn realize_estimated_list(harness: &EstimatedListHarness) {
    // TestingBackend does not render on show. ListView updates its estimated
    // extent while visiting repeated items, not while reading viewport-height.
    let _ = harness.root_element().visit_descendants(|_| std::ops::ControlFlow::<()>::Continue(()));
}

fn pointer(harness: &EstimatedListHarness, position: slint::LogicalPosition, pressed: bool) {
    realize_estimated_list(harness);
    let window = harness.window();
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    let button = slint::platform::PointerEventButton::Left;
    window.dispatch_event(if pressed {
        slint::platform::WindowEvent::PointerPressed { position, button }
    } else {
        slint::platform::WindowEvent::PointerReleased { position, button }
    });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
    realize_estimated_list(harness);
}

fn estimated_wheel(harness: &EstimatedListHarness, delta: f32) {
    realize_estimated_list(harness);
    harness.window().dispatch_event(slint::platform::WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(2.0, harness.get_view_height() / 2.0),
        delta_x: 0.0, delta_y: delta,
    });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(32));
    realize_estimated_list(harness);
}

#[test]
fn scrollbar_drag_reanchors_after_virtual_extent_changes_and_caps_short_tracks() {
    i_slint_backend_testing::init_no_event_loop();
    let harness = EstimatedListHarness::new().unwrap();
    harness.set_row_heights(slint::ModelRc::new(slint::VecModel::from(vec![20; 1024])));
    harness.show().unwrap();
    harness.window().set_size(slint::LogicalSize::new(200.0, 100.0));
    realize_estimated_list(&harness);
    let initial_extent = harness.get_content_height();
    assert!(initial_extent > 1000.0, "Visiting the real virtual delegates must realize a large estimated extent, got {initial_extent}");
    assert_eq!(harness.get_scroll_offset(), 0.0);
    let held = slint::LogicalPosition::new(191.0, 32.0);
    pointer(&harness, slint::LogicalPosition::new(191.0, 16.0), true);
    harness.window().dispatch_event(slint::platform::WindowEvent::PointerMoved { position: held });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(32));
    realize_estimated_list(&harness);
    assert!(harness.get_scroll_offset() < 0.0, "Pointer motion on the thumb must scroll the actual virtual list");
    // Change the real delegate population and heights while the thumb is held.
    // The next stationary pointer event must not reuse the old drag range.
    harness.set_row_heights(slint::ModelRc::new(slint::VecModel::from(
        (0..2048).map(|index| if index < 512 { 20 } else { 80 }).collect::<Vec<_>>())));
    realize_estimated_list(&harness);
    let changed_extent = harness.get_content_height();
    assert!(changed_extent > initial_extent, "The virtual list must update its estimated extent");
    let after_extent_change = harness.get_scroll_offset();
    harness.window().dispatch_event(slint::platform::WindowEvent::PointerMoved { position: held });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
    realize_estimated_list(&harness);
    assert!((harness.get_scroll_offset() - after_extent_change).abs() <= 1.0,
        "A stationary held thumb must not jump when the estimated range changes");
    let next = slint::LogicalPosition::new(held.x, held.y + 2.0);
    harness.window().dispatch_event(slint::platform::WindowEvent::PointerMoved { position: next });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
    realize_estimated_list(&harness);
    let offset = harness.get_scroll_offset();
    let range = (harness.get_content_height() - harness.get_view_height()).max(0.0);
    assert!(offset.is_finite() && offset <= 0.0 && offset >= -range - 1.0);
    assert!(offset <= after_extent_change + 1.0 && (offset - after_extent_change).abs() <= range * 0.1 + 80.0,
        "A two-pixel drag after a range change must continue locally, not jump to an old reference");
    pointer(&harness, next, false);
    estimated_wheel(&harness, 1_000_000.0);
    assert!(harness.get_scroll_offset().abs() <= 1.0);
    for _ in 0..3 { estimated_wheel(&harness, -1_000_000.0); let _ = harness.get_content_height(); }
    assert!((harness.get_scroll_offset() + harness.get_content_height() - harness.get_view_height()).abs() <= 1.0,
        "The virtual list must reach the bottom after its estimates settle");

    // A20px track is shorter than the requested32px thumb minimum. The
    // capped full-track handle has zero travel, so dragging must remain finite.
    harness.set_view_height(20.0);
    estimated_wheel(&harness, 1_000_000.0);
    pointer(&harness, slint::LogicalPosition::new(191.0, 10.0), true);
    harness.window().dispatch_event(slint::platform::WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(191.0, 19.0),
    });
    pointer(&harness, slint::LogicalPosition::new(191.0, 19.0), false);
    assert!(harness.get_scroll_offset().is_finite() && harness.get_scroll_offset().abs() <= 1.0,
        "A minimum thumb capped to a short track cannot divide by zero or move beyond the track");
    estimated_wheel(&harness, -100.0);
    assert!(harness.get_scroll_offset() < 0.0, "The short viewport must retain native wheel scrolling");

    harness.set_row_heights(slint::ModelRc::new(slint::VecModel::<i32>::default()));
    realize_estimated_list(&harness);
    assert!(harness.get_scroll_offset().abs() <= 1.0, "Emptying a scrolled list must clamp it to the top");
    assert!((harness.get_content_width() - 182.0).abs() <= 1.0, "The18px lane remains reserved when empty");
    harness.set_view_height(80.0);
    harness.set_row_heights(slint::ModelRc::new(slint::VecModel::from(vec![10])));
    realize_estimated_list(&harness);
    pointer(&harness, slint::LogicalPosition::new(191.0, 40.0), true);
    pointer(&harness, slint::LogicalPosition::new(191.0, 70.0), false);
    estimated_wheel(&harness, -100.0);
    assert!(harness.get_scroll_offset().is_finite() && harness.get_scroll_offset().abs() <= 1.0,
        "A fitting list has no draggable or paging handle");
    assert!((harness.get_content_width() - 182.0).abs() <= 1.0, "The fitting list keeps the same reserved lane");
}

fn finish(desktop: &Desktop, successes: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        desktop.controller.borrow_mut().drain();
        if !desktop.controller.borrow().is_busy() { break; }
        assert!(Instant::now() < deadline, "controller worker timed out");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(desktop.controller.borrow().completed_jobs, successes,
        "visible controller error: {:?}", desktop.controller.borrow().last_error);
}

fn stats(desktop: &Desktop) -> Value {
    let rows = [desktop.window.get_stats_base(), desktop.window.get_stats_physical(), desktop.window.get_stats_magical(),
        desktop.window.get_stats_multi(), desktop.window.get_stats_haste(), desktop.window.get_stats_defensive(), desktop.window.get_stats_other()];
    Value::Object(rows.into_iter().flat_map(|model| model.iter().map(|row| (row.label.to_string(), Value::String(row.value.to_string()))).collect::<Vec<_>>()).collect())
}

#[test]
fn combo_popup_selection_reaches_controller_and_retains_unlisted_restored_values() {
    fn realize(desktop: &Desktop) {
        for _ in 0..3 {
            let _ = desktop.window.root_element().visit_descendants(|_| std::ops::ControlFlow::<()>::Continue(()));
            i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        }
    }
    fn combo(desktop: &Desktop, label: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(&desktop.window, label)
            .find(|element| element.accessible_role() == Some(AccessibleRole::Combobox)).unwrap()
    }
    fn popup(desktop: &Desktop, label: &str) -> Option<ElementHandle> {
        ElementHandle::find_by_accessible_label(&desktop.window, &format!("{label} options"))
            .find(|element| element.accessible_role() == Some(AccessibleRole::List))
    }
    fn option(popup: &ElementHandle, value: &str) -> ElementHandle {
        popup.query_descendants().find_all().into_iter().find(|element|
            element.accessible_role() == Some(AccessibleRole::ListItem) &&
                element.accessible_label().is_some_and(|label| label == value)).unwrap()
    }
    fn click(desktop: &Desktop, element: &ElementHandle, in_popup: bool) {
        let position = element.absolute_position();
        let size = element.size();
        let mut position = slint::LogicalPosition::new(position.x + size.width / 2.0, position.y + size.height / 2.0);
        let window = desktop.window.window();
        if in_popup {
            // Slint 1.16's search API leaves embedded-popup coordinates local.
            // Use its actual native origin, including any window-edge shift.
            use slint::private_unstable_api::re_exports::{ItemRc, WindowInner};
            let inner = WindowInner::from_pub(window);
            let popups = inner.active_popups();
            let popup = popups.last().expect("The clicked option must belong to an open popup");
            let root = ItemRc::new_root(popup.component.clone());
            let origin = root.map_to_native_window(Default::default());
            position.x += origin.x;
            position.y += origin.y;
        }
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        realize(desktop);
    }
    fn key(desktop: &Desktop, pressed: Key) {
        let text: slint::SharedString = pressed.into();
        desktop.window.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        desktop.window.window().dispatch_event(WindowEvent::KeyReleased { text });
        realize(desktop);
    }

    i_slint_backend_testing::init_no_event_loop();
    let fixture = ui_support::cases("ui.profile").into_iter().find(|fixture| fixture["id"] == "ui.profile.10.ninja").unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let mut paths = AppPaths::from_parts(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_os_str()),
        std::path::Path::new("/unused"), std::path::Path::new("/")).unwrap();
    paths.defaults = sandbox.path().join("defaults.json");
    fs::write(&paths.defaults, serde_json::to_vec(&fixture["input"]["profile_document"]).unwrap()).unwrap();
    let desktop = Desktop::with_random(Resources { paths, catalog: ui_support::catalog() }, RandomSources::from_seeds(11, 11)).unwrap();
    desktop.window.show().unwrap();
    desktop.window.window().set_size(slint::LogicalSize::new(1152.0, 900.0));
    desktop.window.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
    let choices = desktop.controller.borrow().ui.controls["defaults_ml_combobox"].choices.clone();
    desktop.window.invoke_text_edited("defaults_ml_combobox".into(), choices[0].as_str().into());
    realize(&desktop);

    click(&desktop, &combo(&desktop, "Master level"), false);
    let opened = popup(&desktop, "Master level").expect("A real pointer click must open the production popup");
    click(&desktop, &option(&opened, &choices[2]), true);
    assert_eq!(desktop.controller.borrow().ui.text("defaults_ml_combobox"), choices[2]);
    assert!(popup(&desktop, "Master level").is_none(), "Pointer selection must close the popup");
    key(&desktop, Key::Return);
    assert!(popup(&desktop, "Master level").is_some(), "The closed combo must regain keyboard focus");
    key(&desktop, Key::DownArrow);
    assert_eq!(desktop.controller.borrow().ui.text("defaults_ml_combobox"), choices[3]);
    key(&desktop, Key::UpArrow);
    assert_eq!(desktop.controller.borrow().ui.text("defaults_ml_combobox"), choices[2]);
    key(&desktop, Key::Return);
    assert!(popup(&desktop, "Master level").is_none());
    key(&desktop, Key::Return);
    assert!(popup(&desktop, "Master level").is_some());
    key(&desktop, Key::Escape);
    assert!(popup(&desktop, "Master level").is_none());
    desktop.window.window().dispatch_event(WindowEvent::KeyPressed { text: "q".into() });
    desktop.window.window().dispatch_event(WindowEvent::KeyReleased { text: "q".into() });
    assert!(desktop.window.window().is_visible(), "Combo focus must still protect the application's q shortcut");

    // Restore a value from disk, then refresh the actual native choices without
    // it. A model reset is not an activation and must not replace that value.
    let restored = "7";
    let mut profile = fixture["input"]["profile_document"].clone();
    profile["Ninja"]["defaults_aftermath_combobox"] = json!(restored);
    fs::write(&desktop.controller.borrow().paths.defaults, serde_json::to_vec(&profile).unwrap()).unwrap();
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    realize(&desktop);
    assert_eq!(combo(&desktop, "Aftermath level").accessible_value().unwrap().as_str(), restored);
    desktop.controller.borrow_mut().ui.controls.get_mut("defaults_aftermath_combobox").unwrap()
        .choices.retain(|value| value != restored);
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "1777".into());
    realize(&desktop);
    assert_eq!(desktop.controller.borrow().ui.text("defaults_aftermath_combobox"), restored);
    assert_eq!(combo(&desktop, "Aftermath level").accessible_value().unwrap().as_str(), restored,
        "Refreshing choices must not erase an unlisted restored display value");
    let first_choice = desktop.controller.borrow().ui.controls["defaults_aftermath_combobox"].choices[0].clone();
    click(&desktop, &combo(&desktop, "Aftermath level"), false);
    let opened = popup(&desktop, "Aftermath level").unwrap();
    let first = option(&opened, &first_choice);
    desktop.controller.borrow_mut().ui.controls.get_mut("defaults_aftermath_combobox").unwrap().enabled = false;
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "1888".into());
    realize(&desktop);
    key(&desktop, Key::DownArrow);
    click(&desktop, &first, true);
    assert_eq!(desktop.controller.borrow().ui.text("defaults_aftermath_combobox"), restored,
        "Disabling a combo with its popup open must reject both keyboard and pointer selection");
    assert_eq!(combo(&desktop, "Aftermath level").accessible_value().unwrap().as_str(), restored);
    key(&desktop, Key::Escape);
    click(&desktop, &combo(&desktop, "Aftermath level"), false);
    key(&desktop, Key::Return);
    key(&desktop, Key::DownArrow);
    assert!(popup(&desktop, "Aftermath level").is_none(), "Disabled input must not reopen its popup");
    assert_eq!(desktop.controller.borrow().ui.text("defaults_aftermath_combobox"), restored);
}

#[test]
fn real_slint_controller_keeps_results_lazy_freezes_busy_inputs_and_retains_corrupt_profiles() {
    i_slint_backend_testing::init_no_event_loop();
    let fixture = ui_support::cases("ui.profile").into_iter().find(|fixture| fixture["id"] == "ui.profile.10.ninja").unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let mut paths = AppPaths::from_parts(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_os_str()),
        std::path::Path::new("/unused"), std::path::Path::new("/")).unwrap();
    paths.defaults = sandbox.path().join("defaults.json");
    fs::write(&paths.defaults, serde_json::to_vec(&fixture["input"]["profile_document"]).unwrap()).unwrap();
    let desktop = Desktop::with_random(Resources { paths: paths.clone(), catalog: ui_support::catalog() }, RandomSources::from_seeds(11,11)).unwrap();
    desktop.window.show().unwrap();
    desktop.window.window().set_size(slint::LogicalSize::new(1152.0, 900.0));
    let master_level = ElementHandle::find_by_accessible_label(&desktop.window, "Master level")
        .find(|element| element.accessible_role() == Some(AccessibleRole::Combobox)).unwrap();
    let main_job = ElementHandle::find_by_accessible_label(&desktop.window, "Main job")
        .find(|element| element.accessible_role() == Some(AccessibleRole::Combobox)).unwrap();
    assert!(master_level.size().height > 0.0);
    assert!(main_job.absolute_position().y >= master_level.absolute_position().y + master_level.size().height,
        "Visible input editors must not overlap");
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    desktop.window.invoke_action("quick-ws".into());
    finish(&desktop, 1);
    let display = &fixture["expected"]["value"]["steps"][5]["state"]["displayed_analytical"];
    assert_eq!(desktop.window.get_damage().as_str(), format!("{} {}", display["damage_label"].as_str().unwrap(), display["damage"].as_str().unwrap()));
    assert_eq!(desktop.window.get_tp_return().as_str(), format!("{} {}", display["tp_label"].as_str().unwrap(), display["tp"].as_str().unwrap()));
    desktop.window.invoke_action("stats-ws".into());
    finish(&desktop, 2);
    assert_eq!(stats(&desktop), fixture["expected"]["value"]["steps"][3]["state"]["displayed_stats"]);
    let stale_stats = stats(&desktop);
    let stale_damage = desktop.window.get_damage();
    let stale_tp = desktop.window.get_tp_return();
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "2300".into());
    assert_eq!(desktop.controller.borrow().ui.text("defaults_tp_entry"), "2300");
    assert_eq!(desktop.controller.borrow().completed_jobs, 2, "editing does not implicitly calculate");
    assert_eq!(desktop.window.get_damage(), stale_damage);
    assert_eq!(desktop.window.get_tp_return(), stale_tp);
    assert_eq!(stats(&desktop), stale_stats);
    let ws_set = desktop.controller.borrow().ui.gear.equipped[2];
    desktop.window.invoke_copy("ws".into(), "quicklook".into());
    assert_eq!(desktop.controller.borrow().ui.gear.equipped[0], ws_set);
    assert_eq!(desktop.window.get_active_tab(), 0);
    assert_eq!(stats(&desktop), stale_stats, "copy preserves last displayed source until explicit Stats refresh");
    assert_eq!(desktop.window.get_damage(), stale_damage);
    let previous_results = desktop.controller.borrow().result_windows().len();
    desktop.window.invoke_action("distribution".into());
    assert!(desktop.window.get_busy());
    assert!(desktop.window.get_basic_controls().iter().all(|row| !row.enabled));
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "2500".into());
    desktop.window.invoke_action("quick-ws".into());
    assert_eq!(desktop.controller.borrow().ui.text("defaults_tp_entry"), "2300");
    assert!(!desktop.window.get_error_message().is_empty());
    assert_eq!(desktop.controller.borrow().result_windows().len(), previous_results,
        "Rejected input edits and busy actions must not open worker-error popups");
    desktop.window.invoke_action("cancel".into());
    finish(&desktop, 2);
    assert_eq!(desktop.controller.borrow().cancelled_jobs, 1);
    assert_eq!(desktop.controller.borrow().result_windows().len(), previous_results,
        "Cancelling a worker must not create an error popup");
    assert_eq!(desktop.window.get_damage(), stale_damage);
    assert_eq!(stats(&desktop), stale_stats);
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    desktop.window.invoke_action("quick-ws".into());
    finish(&desktop, 3);
    assert_eq!(desktop.window.get_damage().as_str(), format!("{} {}", display["damage_label"].as_str().unwrap(), display["damage"].as_str().unwrap()));
    let mut external = fixture["input"]["profile_document"].clone();
    external["Ninja"]["defaults_tp_entry"] = json!("1777");
    external["Ninja"]["external_unknown"] = json!({"retained": [1, 2, 3]});
    fs::write(&paths.defaults, serde_json::to_vec(&external).unwrap()).unwrap();
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    assert_eq!(desktop.controller.borrow().ui.text("defaults_tp_entry"), "1777", "job activation re-reads current JSON rather than a startup cache");
    assert_eq!(desktop.controller.borrow().completed_jobs, 3);
    let good_document = desktop.controller.borrow().profiles.as_ref().unwrap().document().clone();
    fs::write(&paths.defaults, b"{external-corruption").unwrap();
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    desktop.window.invoke_action("save-defaults".into());
    assert_eq!(fs::read(&paths.defaults).unwrap(), b"{external-corruption", "Save cannot overwrite newly corrupt external JSON");
    assert_eq!(desktop.controller.borrow().profiles.as_ref().unwrap().document(), &good_document);
    fs::write(&paths.defaults, serde_json::to_vec(&external).unwrap()).unwrap();
    desktop.window.invoke_action("save-defaults".into());
    let saved: Value = serde_json::from_slice(&fs::read(&paths.defaults).unwrap()).unwrap();
    assert_eq!(saved["Ninja"]["external_unknown"], external["Ninja"]["external_unknown"]);
    desktop.window.invoke_action("distribution".into());
    desktop.window.window().dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "q".into() });
    assert!(!desktop.controller.borrow().is_busy(), "Close joins the owned computation");
    assert!(!desktop.window.window().is_visible());
    drop(desktop);

    // A malformed existing profile is visible and must never be silently overwritten.
    fs::write(&paths.defaults, b"{corrupt").unwrap();
    let corrupt = Desktop::with_random(Resources { paths: paths.clone(), catalog: ui_support::catalog() }, RandomSources::from_seeds(11,11)).unwrap();
    assert!(!corrupt.window.get_error_message().is_empty());
    assert!(corrupt.controller.borrow().profiles.is_none());
    corrupt.window.invoke_text_edited("defaults_tp_entry".into(), "1777".into());
    corrupt.window.invoke_action("save-defaults".into());
    assert_eq!(fs::read(&paths.defaults).unwrap(), b"{corrupt");
    assert_eq!(corrupt.controller.borrow().ui.controls["defaults_tp_entry"].value, json!("1777"));
    assert!(!corrupt.window.get_error_message().is_empty());
}

#[test]
fn optimizer_buttons_replace_progress_with_real_results_and_keep_main_usable() {
    fn displayed_number(text: &str, value: f64, digits: usize, unit: &str) {
        assert!(value.is_finite() && value >= 0.0, "The optimizer must return a finite nonnegative {unit} value, got {value}");
        let expected = format!("{value:.digits$}");
        assert!(text.lines().any(|line| line.split_whitespace().last() == Some(unit) &&
            line.split_whitespace().any(|word| word == expected)),
            "The visible report must display the computed {expected} {unit}:\n{text}");
    }

    i_slint_backend_testing::init_no_event_loop();
    let fixture = ui_support::cases("ui.profile").into_iter().find(|fixture| fixture["id"] == "ui.profile.10.ninja").unwrap();
    let profile = &fixture["input"]["profile_document"]["Ninja"];
    let sandbox = tempfile::tempdir().unwrap();
    let mut paths = AppPaths::from_parts(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_os_str()),
        std::path::Path::new("/unused"), std::path::Path::new("/")).unwrap();
    paths.defaults = sandbox.path().join("defaults.json");
    let defaults = serde_json::to_vec(&fixture["input"]["profile_document"]).unwrap();
    fs::write(&paths.defaults, &defaults).unwrap();
    let desktop = Desktop::with_random(Resources { paths: paths.clone(), catalog: ui_support::catalog() },
        RandomSources::from_seeds(11, 11)).unwrap();
    desktop.window.show().unwrap();
    desktop.window.window().set_size(slint::LogicalSize::new(1152.0, 900.0));
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    for (key, value) in [
        ("defaults_optimizer_seed_entry", "11"), ("defaults_optimizer_iterations_entry", "1"),
        ("defaults_optimizer_restart_count_entry", "1"), ("defaults_optimizer_workers_entry", "1"),
        ("defaults_optimizer_swap_slots_combobox", "1"), ("defaults_optimizer_dt_step_entry", "1"),
        ("optimize_tab.pdt_requirements_entry", "0"), ("optimize_tab.mdt_requirements_entry", "0"),
    ] {
        desktop.window.invoke_text_edited(key.into(), value.into());
    }
    {
        let mut state = desktop.controller.borrow_mut();
        assert_eq!(state.ui.text("defaults_ws_combobox"), profile["defaults_ws_combobox"].as_str().unwrap());
        let spell = profile["defaults_spell_combobox"].as_str().unwrap();
        assert_eq!(state.ui.text("defaults_spell_combobox"), spell);
        assert!(state.ui.controls["defaults_spell_combobox"].choices.iter().any(|choice| choice == spell),
            "The selected Ninja spell must be a real available spell, not a synthetic restored value");
        let mut candidates = std::array::from_fn(|_| Vec::new());
        candidates[Slot::Head.index()] = ["Empty", "Malignance Chapeau", "Hattori Zukin +2", "Ryuo Somen +1 A"].iter()
            .map(|name| state.ui.engine.catalog.by_name2(name).unwrap()).collect();
        state.ui.gear.replace_candidates(&candidates);
    }

    let mut previous_text = None;
    for (index, (label, metric_key, metric, close_progress)) in [
        ("Optimize WS", "optimize_tab.ws_metric_combobox", "Damage dealt", false),
        ("Optimize Spell", "optimize_tab.spell_metric_combobox", "Damage dealt", true),
        ("Optimize TP", "optimize_tab.tp_metric_combobox", "Time to WS", false),
        ("Optimize TP", "optimize_tab.tp_metric_combobox", "DPS", false),
    ].into_iter().enumerate() {
        desktop.window.set_active_tab(1);
        desktop.window.invoke_text_edited(metric_key.into(), metric.into());
        let _ = desktop.window.root_element().visit_descendants(|_| std::ops::ControlFlow::<()>::Continue(()));
        let previous_windows = desktop.controller.borrow().result_windows().len();
        let button = ElementHandle::find_by_accessible_label(&desktop.window, label)
            .find(|element| element.accessible_role() == Some(AccessibleRole::Button)).unwrap();
        assert_ne!(button.accessible_enabled(), Some(false), "{label} must be usable before starting");
        button.invoke_accessible_default_action();
        let progress = {
            let state = desktop.controller.borrow();
            assert!(state.is_busy(), "{label} must start a real controller worker");
            assert!(desktop.window.get_busy());
            assert_eq!(state.completed_jobs, index);
            assert_eq!(state.result_windows().len(), previous_windows, "No final result may appear before completion");
            state.progress_window().expect("Every optimizer action must open progress while working").clone_strong()
        };
        assert!(progress.window().is_visible());
        if close_progress {
            // Close the production progress window before the controller drains
            // completion; this must not cancel or suppress the final report.
            ElementHandle::find_by_accessible_label(&progress, "Close progress")
                .find(|element| element.accessible_role() == Some(AccessibleRole::Button)).unwrap()
                .invoke_accessible_default_action();
            assert!(!progress.window().is_visible());
            assert!(desktop.controller.borrow().is_busy());
        }
        finish(&desktop, index + 1);
        assert!(!progress.window().is_visible(), "{label} must dismiss progress so it cannot obscure the result");

        let text = {
            let state = desktop.controller.borrow();
            assert!(state.last_error.is_none(), "{label}: {:?}", state.last_error);
            assert!(!desktop.window.get_busy());
            assert!(state.progress_window().is_none_or(|window| !window.window().is_visible()));
            assert_eq!(state.result_windows().len(), previous_windows + 1, "Each successful action must show exactly one final report");
            let window = state.result_windows().last().unwrap();
            assert!(window.window().is_visible(), "{label} must show its final ResultWindow");
            let Some(WorkResult::Optimize { result, report }) = state.last_result.as_ref() else {
                panic!("{label} must return an actual optimizer result");
            };
            let player = result.player.as_ref().expect("The small real candidate pool must produce a winning player");
            assert!(result.feasible && result.scored > 0);
            assert!(result.metric.is_finite() && result.metric > 0.0);
            assert_eq!(player.gearset, result.gearset);
            assert_eq!(state.best_set(), Some(result.gearset));
            assert_eq!(window.get_report_title().as_str(), report.title);
            let text = ElementHandle::find_by_accessible_label(window, "Computation result")
                .find_map(|element| element.accessible_value()).expect("The final report must be exposed by the real text editor").to_string();
            assert_eq!(text, report.text, "The visible editor must contain this completion's computed report");
            assert!(text.contains(metric), "{label} must report the explicitly selected metric");
            if label != "Optimize TP" {
                let key = if label == "Optimize WS" { "defaults_ws_combobox" } else { "defaults_spell_combobox" };
                assert!(text.contains(profile[key].as_str().unwrap()), "The final report must identify the selected WS or spell");
            }
            for &slot in Slot::ALL {
                let piece = state.ui.engine.catalog.piece(result.gearset[slot.index()]);
                if slot == Slot::Head {
                    assert!(["Empty", "Malignance Chapeau", "Hattori Zukin +2", "Ryuo Somen +1 A"].contains(&piece.name2.as_ref()));
                } else {
                    assert_eq!(piece.name2.as_ref(), profile[format!("quicklook_{}_item", slot.name())].as_str().unwrap(),
                        "The one-slot search must retain the fixture's other equipment");
                }
                assert!(text.lines().any(|line| line.trim().strip_prefix(slot.name())
                    .is_some_and(|name| name.trim() == piece.name2.as_ref())),
                    "{label} must display the computed winning {}: {}", slot.name(), piece.name2);
            }
            if metric == "Time to WS" {
                displayed_number(&text, result.metric.powi(i32::from(result.invert)), 3, "s");
            } else {
                assert!(result.legacy_output[0] > 0.0, "The winning WS, spell or TP round must deal real damage");
                displayed_number(&text, result.legacy_output[0], if label == "Optimize TP" { 3 } else { 1 }, "damage");
                if label == "Optimize TP" {
                    // DPS is a TP metric: its real report displays the winning
                    // round's damage and duration, not a separate DPS action.
                    displayed_number(&text, result.legacy_output[2], 1, "s");
                }
            }
            displayed_number(&text, result.legacy_output[1], 1, "TP");
            text
        };
        if let Some(previous) = previous_text {
            assert_ne!(text, previous, "The newest report must not retain the previous action or TP metric");
        }
        previous_text = Some(text);
    }

    let results: Vec<_> = desktop.controller.borrow().result_windows().iter().map(|window| window.clone_strong()).collect();
    for window in results {
        ElementHandle::find_by_accessible_label(&window, "Close result")
            .find(|element| element.accessible_role() == Some(AccessibleRole::Button)).unwrap()
            .invoke_accessible_default_action();
        assert!(!window.window().is_visible());
        assert!(desktop.window.window().is_visible(), "Closing a final report must not close the main window");
    }
    desktop.controller.borrow_mut().drain();
    assert!(desktop.controller.borrow().result_windows().is_empty());
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "1777".into());
    assert_eq!(desktop.controller.borrow().ui.text("defaults_tp_entry"), "1777");
    let best = desktop.controller.borrow().best_set().unwrap();
    ElementHandle::find_by_accessible_label(&desktop.window, "Equip best set")
        .find(|element| element.accessible_role() == Some(AccessibleRole::Button)).unwrap()
        .invoke_accessible_default_action();
    assert_eq!(desktop.controller.borrow().ui.gear.equipped[GearSource::Quicklook.index()], best,
        "The main window must still accept meaningful actions after final reports close");
    assert_eq!(desktop.window.get_active_tab(), 0);
    assert!(desktop.window.window().is_visible());
    assert_eq!(fs::read(&paths.defaults).unwrap(), defaults, "The workflow must leave its temporary defaults unchanged");
}

#[test]
fn optimizer_candidate_failures_show_exact_error_and_preserve_successful_state() {
    const ERROR: &str = "Optimize: candidate selection: AssertionError: At least two items must be selected in at least one slot to find the best set.";

    i_slint_backend_testing::init_no_event_loop();
    let fixture = ui_support::cases("ui.profile").into_iter().find(|fixture| fixture["id"] == "ui.profile.10.ninja").unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let mut paths = AppPaths::from_parts(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_os_str()),
        std::path::Path::new("/unused"), std::path::Path::new("/")).unwrap();
    paths.defaults = sandbox.path().join("defaults.json");
    let defaults = serde_json::to_vec(&fixture["input"]["profile_document"]).unwrap();
    fs::write(&paths.defaults, &defaults).unwrap();
    let desktop = Desktop::with_random(Resources { paths: paths.clone(), catalog: ui_support::catalog() },
        RandomSources::from_seeds(11, 11)).unwrap();
    desktop.window.show().unwrap();
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    for (key, value) in [
        ("defaults_optimizer_seed_entry", "11"), ("defaults_optimizer_iterations_entry", "1"),
        ("defaults_optimizer_restart_count_entry", "1"), ("defaults_optimizer_workers_entry", "1"),
        ("defaults_optimizer_swap_slots_combobox", "1"), ("defaults_optimizer_dt_step_entry", "1"),
        ("optimize_tab.pdt_requirements_entry", "0"), ("optimize_tab.mdt_requirements_entry", "0"),
    ] {
        desktop.window.invoke_text_edited(key.into(), value.into());
    }
    let valid_candidates = {
        let state = desktop.controller.borrow();
        let mut candidates = std::array::from_fn(|_| Vec::new());
        candidates[Slot::Head.index()] = ["Malignance Chapeau", "Hattori Zukin +2"].iter()
            .map(|name| state.ui.engine.catalog.by_name2(name).unwrap()).collect();
        candidates
    };
    let visible_text = |window: &wsdist_app::ResultWindow| {
        ElementHandle::find_by_accessible_label(window, "Computation result")
            .find(|element| element.accessible_role() == Some(AccessibleRole::TextInput)).unwrap()
            .accessible_value().expect("The visible result editor must expose its entire contents").to_string()
    };
    let snapshot = || {
        let state = desktop.controller.borrow();
        let result = state.last_result.as_ref().map(|value| {
            let WorkResult::Optimize { result, report } = value else { panic!("Expected a real optimizer result"); };
            (format!("{result:?}"), report.title.clone(), report.text.clone())
        });
        (result, state.best_set(), state.ui.gear.equipped, stats(&desktop),
            desktop.window.get_damage(), desktop.window.get_tp_return(), desktop.window.get_time_to_ws())
    };
    let successful_run = |successes| {
        desktop.controller.borrow_mut().ui.gear.replace_candidates(&valid_candidates);
        let previous_windows = desktop.controller.borrow().result_windows().len();
        desktop.window.invoke_action("optimize-ws".into());
        finish(&desktop, successes);
        let state = desktop.controller.borrow();
        assert!(state.last_error.is_none(), "{:?}", state.last_error);
        assert!(!desktop.window.get_busy());
        assert!(state.progress_window().is_none());
        assert_eq!(state.result_windows().len(), previous_windows + 1);
        let WorkResult::Optimize { result, report } = state.last_result.as_ref().unwrap() else {
            panic!("Recovery must install an actual successful optimizer result");
        };
        assert!(result.feasible && result.player.is_some());
        assert_eq!(state.best_set(), Some(result.gearset));
        let window = state.result_windows().last().unwrap();
        assert!(window.window().is_visible());
        assert_eq!(visible_text(window), report.text);
    };

    assert!(desktop.controller.borrow().last_result.is_none());
    assert!(desktop.controller.borrow().best_set().is_none());
    for singleton in [false, true] {
        if singleton { successful_run(1); }
        {
            let mut state = desktop.controller.borrow_mut();
            let quicklook = state.ui.gear.equipped[GearSource::Quicklook.index()];
            let candidates = std::array::from_fn(|slot| if singleton { vec![quicklook[slot]] } else { Vec::new() });
            state.ui.gear.replace_candidates(&candidates);
        }
        let expected_state = snapshot();
        for action in ["optimize-ws", "optimize-spell", "optimize-round"] {
            let previous_windows = desktop.controller.borrow().result_windows().len();
            desktop.window.invoke_action(action.into());
            let progress = {
                let state = desktop.controller.borrow();
                assert!(state.is_busy(), "{action} must start a real worker");
                assert!(desktop.window.get_busy());
                assert_eq!(state.result_windows().len(), previous_windows);
                state.progress_window().expect("Optimizer work must open progress").clone_strong()
            };
            assert!(progress.window().is_visible());
            finish(&desktop, usize::from(singleton));
            {
                let state = desktop.controller.borrow();
                assert!(!state.is_busy());
                assert!(!desktop.window.get_busy());
                assert!(state.progress_window().is_none());
                assert!(!progress.window().is_visible(), "{action} failure must dismiss progress");
                assert_eq!(state.cancelled_jobs, 0);
                assert_eq!(state.last_error.as_deref(), Some(ERROR));
                assert_eq!(desktop.window.get_error_message().as_str(), ERROR);
                assert_eq!(state.result_windows().len(), previous_windows + 1,
                    "Each failed worker must show exactly one final popup");
                let window = state.result_windows().last().unwrap();
                assert!(window.window().is_visible());
                assert_eq!(window.get_report_text().as_str(), ERROR);
                assert_eq!(visible_text(window), ERROR,
                    "The real accessible result editor must preserve the original candidate-selection assertion");
            }
            assert_eq!(snapshot(), expected_state, "{action} failure must retain the entire last successful result, best set and displayed gear");
            desktop.controller.borrow_mut().drain();
            assert_eq!(desktop.controller.borrow().result_windows().len(), previous_windows + 1);
        }
    }
    successful_run(2);
    assert_eq!(fs::read(&paths.defaults).unwrap(), defaults, "Failed and recovered runs must not alter saved defaults");
}

fn layout_viewport(root: &ElementHandle, label: &str) -> ElementHandle {
    let content_label = label.to_owned();
    let content = root.query_descendants()
        .match_predicate(move |element| element.accessible_label().is_some_and(|label| label == content_label))
        .find_first().unwrap();
    // Inspect the actual Flickable rather than the accessible wrapper, whose
    // stable18px reserved lane is deliberately outside the viewport.
    let viewport = content.query_descendants().match_inherits("Flickable").find_all().into_iter()
        .max_by(|left, right| left.size().width.total_cmp(&right.size().width))
        .expect("The content must expose its actual scrolling viewport");
    let clip_left = viewport.absolute_position().x;
    let clip_right = clip_left + viewport.size().width;
    let content_left = content.absolute_position().x;
    let gutter_left = content_left + content.size().width - 18.0;
    assert!((clip_left - content_left).abs() <= 1.0,
        "{label}: the actual visible clip must remain left-aligned with its content, not implicitly centered: {clip_left} vs {content_left}");
    assert!(clip_right <= gutter_left + 1.0,
        "{label}: the actual visible clip must end before the reserved scrollbar lane: {clip_right} vs {gutter_left}");
    viewport
}

fn fixed_horizontal(element: &ElementHandle, key: &str, viewport: &ElementHandle,
    baseline: &mut BTreeMap<String, (f32, f32)>) {
    let position = element.absolute_position();
    let size = element.size();
    let left = viewport.absolute_position().x;
    let right = left + viewport.size().width;
    assert!(size.width > 0.0 && size.height > 0.0, "{key} must have a usable canvas");
    assert!(position.x >= left - 1.0 && position.x + size.width <= right + 1.0,
        "{key} must fit the actual viewport without entering the reserved scrollbar lane: {position:?} {size:?}, {left}..{right}");
    if let Some(&(x, width)) = baseline.get(key) {
        assert!((position.x - x).abs() <= 1.0 && (size.width - width).abs() <= 1.0,
            "{key} changed its horizontal geometry: expected x={x}, width={width}; actual {position:?} {size:?}");
    } else {
        baseline.insert(key.to_owned(), (position.x, size.width));
    }
}

fn fixed_input_label(label: &str, field: &ElementHandle, viewport: &ElementHandle,
    baseline: &mut BTreeMap<String, (f32, f32)>) {
    let row_label: slint::SharedString = label.into();
    let row = viewport.query_descendants().match_type_name("ControlEditor")
        .match_predicate(move |row| {
            let row_label = row_label.clone();
            row.query_descendants()
                .match_predicate(move |child| child.accessible_label().is_some_and(|text| text == row_label))
                .find_first().is_some()
        }).find_first().unwrap_or_else(|| panic!("The visible {label} editor must have its label row"));
    let field_position = field.absolute_position();
    let text = row.query_descendants().match_type_name("Text")
        .match_predicate(move |text| {
            let position = text.absolute_position();
            let size = text.size();
            size.width > 0.0 && position.x < field_position.x - 1.0 &&
                position.x + size.width <= field_position.x + 1.0 &&
                (position.y - field_position.y).abs() <= 3.0
        }).find_first().unwrap_or_else(|| panic!("{label} must retain a readable, non-overlapping label canvas"));
    fixed_horizontal(&row, &format!("input row/{label}"), viewport, baseline);
    fixed_horizontal(&text, &format!("input label/{label}"), viewport, baseline);
}

fn scroll_layout(desktop: &Desktop, viewport: &ElementHandle, delta: f32) {
    let position = viewport.absolute_position();
    desktop.window.window().dispatch_event(slint::platform::WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(position.x + 2.0, position.y + viewport.size().height / 2.0),
        delta_x: 0.0, delta_y: delta,
    });
}

fn fixed_stat_geometry(desktop: &Desktop, baseline: &mut BTreeMap<String, (f32, f32)>, oversized_labels: &[String]) {
    desktop.window.set_active_tab(3);
    let viewport = layout_viewport(&desktop.window.root_element(), "Stats content");
    let groups = [desktop.window.get_stats_base(), desktop.window.get_stats_physical(), desktop.window.get_stats_magical(),
        desktop.window.get_stats_multi(), desktop.window.get_stats_haste(), desktop.window.get_stats_defensive(), desktop.window.get_stats_other()];
    assert_eq!(groups.iter().map(|rows| rows.row_count()).sum::<usize>(), 53, "All player Stats rows must remain available");
    let mut expected = groups.map(|rows| rows.iter()
        .map(|stat| (format!("{}: {}", stat.label, stat.value), stat, None::<ElementHandle>)).collect::<Vec<_>>());
    let mut group_boxes: [Option<ElementHandle>; 7] = std::array::from_fn(|_| None);
    let mut remaining = 53;
    scroll_layout(desktop, &viewport, 10000.0);
    // Groups share horizontal rows, so visit every exposed column before
    // advancing the outer viewport rather than scrolling in model-group order.
    for _ in 0..24 {
        let top = viewport.absolute_position().y;
        let bottom = top + viewport.size().height;
        let visible = viewport.query_descendants().match_accessible_role(AccessibleRole::ListItem)
            .match_predicate(move |row| {
                let position = row.absolute_position();
                position.y >= top - 1.0 && position.y + row.size().height <= bottom + 1.0
            }).find_all();
        for row in visible {
            let Some(row_label) = row.accessible_label() else { continue; };
            let Some((index, (_, stat, checked))) = expected.iter_mut().enumerate().find_map(|(index, rows)| {
                rows.iter_mut().find(|(label, _, _)| label.as_str() == row_label.as_str())
                    .map(|row| (index, row))
            }) else { continue; };
            if checked.is_some() { continue; }
            if group_boxes[index].is_none() {
                let row_label = row_label.clone();
                let group = viewport.query_descendants().match_type_name("StatGroup")
                    .match_predicate(move |group| {
                        let row_label = row_label.clone();
                        group.query_descendants().match_accessible_role(AccessibleRole::ListItem)
                            .match_predicate(move |child| child.accessible_label().is_some_and(|label| label == row_label))
                            .find_first().is_some()
                    }).find_first().expect("Each visible Stats row must fit its group");
                fixed_horizontal(&group, &format!("Stats/group/{index}"), &viewport, baseline);
                group_boxes[index] = Some(group);
            }
            let texts = row.query_descendants().match_type_name("Text").find_all();
            let value = texts.iter()
                .find(|text| text.accessible_label().is_some_and(|label| label == stat.label) &&
                    text.accessible_value().is_some_and(|value| value == stat.value))
                .unwrap_or_else(|| panic!("{} must expose its displayed value", stat.label));
            let label = texts.iter()
                .find(|text| text.accessible_label().is_some_and(|label| label == stat.label) && text.accessible_value().is_none())
                .unwrap_or_else(|| panic!("{} must retain its visible label", stat.label));
            assert!(value.size().width >= (if index == 0 { 48.0 } else { 64.0 }),
                "{} must reserve readable value space independently of its digits/units", stat.label);
            assert!(label.size().width >= 20.0 && label.absolute_position().x + label.size().width + 5.0 <= value.absolute_position().x,
                "{} label and value must not overlap", stat.label);
            for text in [label, value] {
                assert!(text.absolute_position().y >= row.absolute_position().y - 1.0 &&
                    text.absolute_position().y + text.size().height <= row.absolute_position().y + row.size().height + 1.0,
                    "{} must keep its wrapped label/value inside the row", stat.label);
            }
            for (part, element) in [("row", &row), ("label", label), ("value", value)] {
                fixed_horizontal(element, &format!("Stats/{}/{part}", stat.label), &viewport, baseline);
            }
            if oversized_labels.iter().any(|label| label.as_str() == row_label.as_str()) {
                assert!(value.size().height > 24.0, "Oversized displayed values must gain vertical space rather than clip or widen");
            }
            *checked = Some(row);
            remaining -= 1;
        }
        if remaining == 0 { break; }
        scroll_layout(desktop, &viewport, -160.0);
    }
    // Ordinary repeater handles remain valid within this pass. Compare all
    // neighbors and group rows now, in one consistent final scroll state.
    for rows in &expected {
        for (label, _, row) in rows {
            assert!(row.is_some(), "The Stats row {label:?} must be reachable through the outer viewport");
        }
        for pair in rows.windows(2) {
            let previous = pair[0].2.as_ref().unwrap();
            let row = pair[1].2.as_ref().unwrap();
            assert!(row.absolute_position().y >= previous.absolute_position().y + previous.size().height - 1.0,
                "{} must not overlap the preceding Stats row", pair[1].1.label);
        }
    }
    let group_boxes = group_boxes.map(|group| group.expect("Every Stats group must have an exposed row"));
    for (upper, lower) in [(&group_boxes[0..3], &group_boxes[3..5]), (&group_boxes[3..5], &group_boxes[5..7])] {
        let bottom = upper.iter().map(|group| group.absolute_position().y + group.size().height).fold(f32::NEG_INFINITY, f32::max);
        assert!(lower.iter().all(|group| group.absolute_position().y >= bottom - 1.0),
            "Growing Stats rows must move the following group row down, not overlap it");
    }
}

fn quicklook_content_height_geometry(desktop: &Desktop, outer: &ElementHandle, baseline: &mut BTreeMap<String, (f32, f32)>) {
    let element = |label: &str, role: AccessibleRole| {
        let element_label = label.to_owned();
        outer.query_descendants().match_accessible_role(role)
            .match_predicate(move |element| element.accessible_label().is_some_and(|label| label == element_label))
            .find_first().unwrap_or_else(|| panic!("{label:?} must expose its rendered {role:?} geometry"))
    };
    let bottom = |element: &ElementHandle| element.absolute_position().y + element.size().height;
    let role = |row: &InputRow| match row.control_kind {
        InputWidgetKind::Entry => AccessibleRole::TextInput,
        InputWidgetKind::Combo => AccessibleRole::Combobox,
        InputWidgetKind::Checkbox => AccessibleRole::Checkbox,
        InputWidgetKind::Label => AccessibleRole::Text,
    };
    let basics = element("Job, spell, weapon skill & TP inputs", AccessibleRole::List);
    let enemy = element("Enemy Inputs inputs", AccessibleRole::List);
    let toggles = element("Special Toggles inputs", AccessibleRole::List);
    let enemy_group = element("Enemy Inputs", AccessibleRole::Groupbox);
    let buffs_group = element("Active Buffs", AccessibleRole::Groupbox);
    let mut enemy_bottom = enemy.absolute_position().y;
    for (model, viewport) in [(desktop.window.get_basic_controls(), &basics), (desktop.window.get_enemy_controls(), &enemy)] {
        for row in model.iter().filter(|row| row.visible) {
            let control = element(row.label.as_str(), role(&row));
            assert!((20.0..=30.0).contains(&control.size().height),
                "{} must retain its normal one-row editor height", row.label);
            assert!(control.absolute_position().y >= viewport.absolute_position().y - 1.0 && bottom(&control) <= bottom(viewport) + 1.0,
                "{} must fit its column without clipping or inner scrolling", row.label);
            if viewport.accessible_label().as_deref() == Some("Enemy Inputs inputs") {
                enemy_bottom = enemy_bottom.max(bottom(&control));
            }
        }
    }
    assert!((bottom(&enemy) - enemy_bottom).abs() <= 1.0,
        "The Enemy viewport must end at its last rendered field, without surplus blank space");
    assert!((bottom(&toggles) - enemy_bottom).abs() <= 1.0,
        "The Special Toggles viewport must end at the last rendered Enemy row");
    assert!((buffs_group.absolute_position().y - bottom(&enemy_group)).abs() <= 1.0,
        "The buff section must immediately follow the first input row");
    let enemy_chrome = enemy_group.size().height - enemy.size().height;
    let enemy_bottom_padding = bottom(&enemy_group) - enemy_bottom;

    // Scroll the whole buff section into the outer viewport, not one of its
    // editable fields. Its final controls must fit without an inner wheel.
    scroll_layout(desktop, outer, outer.absolute_position().y - buffs_group.absolute_position().y);
    let groups = [
        (format!("White Magic — {} inputs", desktop.window.get_skill_label()),
            vec![desktop.window.get_whm_controls(), desktop.window.get_food_controls()]),
        ("Bard Songs inputs".to_owned(), vec![desktop.window.get_brd_controls()]),
        ("Corsair Rolls inputs".to_owned(), vec![desktop.window.get_cor_controls()]),
        ("Geomancy Bubbles inputs".to_owned(), vec![desktop.window.get_geo_controls()]),
    ];
    let mut content_bottom = f32::NEG_INFINITY;
    let mut buff_top: Option<f32> = None;
    let mut buff_bottom: Option<f32> = None;
    for (label, models) in groups {
        let viewport = layout_viewport(outer, &label);
        let top = viewport.absolute_position().y;
        let end = bottom(&viewport);
        if let Some(expected) = buff_top {
            assert!((top - expected).abs() <= 1.0, "Buff columns must share their top edge");
        }
        if let Some(expected) = buff_bottom {
            assert!((end - expected).abs() <= 1.0, "Buff columns must share their bottom edge");
        }
        buff_top = Some(top);
        buff_bottom = Some(end);
        for model in models {
            for row in model.iter().filter(|row| row.visible) {
                let control = element(row.label.as_str(), role(&row));
                assert!((20.0..=30.0).contains(&control.size().height),
                    "{} must retain its normal buff editor height", row.label);
                assert!(control.absolute_position().y >= top - 1.0 && bottom(&control) <= end + 1.0,
                    "{} must fit its buff column without clipping or inner scrolling", row.label);
                assert!(control.absolute_position().y >= outer.absolute_position().y - 1.0 && bottom(&control) <= bottom(&outer) + 1.0,
                    "{} must be fully reachable through the outer viewport", row.label);
                fixed_horizontal(&control, &format!("Quicklook/buff/{}", row.key), &viewport, baseline);
                content_bottom = content_bottom.max(bottom(&control));
            }
        }
    }
    let buff_height = buff_bottom.unwrap() - buff_top.unwrap();
    assert!((buff_bottom.unwrap() - content_bottom).abs() <= 1.0,
        "The buff row must end at its tallest natural content, not retain a fixed-height blank gap");
    let buff_chrome = buffs_group.size().height - buff_height;
    assert!((enemy_chrome - buff_chrome).abs() <= 1.0 &&
        (enemy_bottom_padding - (bottom(&buffs_group) - content_bottom)).abs() <= 1.0,
        "Both input rows must contain only their rendered content and matching GroupBox overhead: enemy chrome={enemy_chrome}, buff chrome={buff_chrome}, enemy bottom padding={enemy_bottom_padding}, buff bottom padding={}, buff height={buff_height}", bottom(&buffs_group) - content_bottom);
}

#[test]
fn compact_window_preserves_input_columns_and_visible_editors() {
    i_slint_backend_testing::init_no_event_loop();
    let sandbox = tempfile::tempdir().unwrap();
    let mut paths = AppPaths::from_parts(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_os_str()),
        std::path::Path::new("/unused"), std::path::Path::new("/")).unwrap();
    paths.defaults = sandbox.path().join("defaults.json");
    let desktop = Desktop::with_random(Resources { paths, catalog: ui_support::catalog() }, RandomSources::from_seeds(11, 11)).unwrap();
    desktop.window.show().unwrap();

    let fixture = ui_support::cases("ui.profile").into_iter().find(|fixture| fixture["id"] == "ui.profile.10.ninja").unwrap();
    fs::write(&desktop.controller.borrow().paths.defaults, serde_json::to_vec(&fixture["input"]["profile_document"]).unwrap()).unwrap();
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    desktop.window.invoke_action("stats-quicklook".into());
    finish(&desktop, 1);
    let mut baseline = BTreeMap::new();
    for (width, height) in [(760.0, 440.0), (720.0, 560.0), (1440.0, 1080.0), (720.0, 440.0)] {
        desktop.window.window().set_size(slint::LogicalSize::new(width, height));
        for profile in ["Ninja", "Black Mage"] {
            desktop.window.set_active_tab(0);
            desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), profile.into());
            let quick_viewport = layout_viewport(&desktop.window.root_element(), "Quicklook content");
            scroll_layout(&desktop, &quick_viewport, 10000.0);
            let editor = |label: &str| {
                let editor_label = label.to_owned();
                quick_viewport.query_descendants()
                    .match_predicate(move |element| element.accessible_label().is_some_and(|label| label == editor_label))
                    .find_first().unwrap_or_else(|| panic!("{label} must remain visible for {profile} at {width}x{height}"))
            };
            let basic = editor("Master level");
            let abilities = editor("Special Toggles inputs");
            let enemy = editor("Enemy preset");
            assert!(basic.size().width >= 60.0 && basic.size().height >= 20.0);
            assert!(abilities.absolute_position().x >= basic.absolute_position().x + basic.size().width,
                "Special toggles must remain beside, not below, basic inputs");
            assert!(enemy.absolute_position().x >= abilities.absolute_position().x + abilities.size().width,
                "Enemy inputs must remain in the third column");
            assert!((enemy.absolute_position().y - basic.absolute_position().y).abs() < 50.0,
                "Basic and enemy inputs must remain in the same row");
            assert!(enemy.size().height <= 30.0,
                "The enemy preset must remain a single compact row, not stretch over its siblings");
            let location = editor("Enemy level / location");
            assert!(location.absolute_position().y >= enemy.absolute_position().y + enemy.size().height,
                "Enemy preset and location must not overlap");
            let mut previous_bottom = abilities.absolute_position().y;
            for row in desktop.window.get_ability_controls().iter().filter(|row| row.visible).take(2) {
                let control = editor(row.label.as_str());
                assert!((20.0..=30.0).contains(&control.size().height),
                    "Special toggles must retain one-row height");
                assert!(control.absolute_position().y >= previous_bottom,
                    "Special toggles must not overlap");
                previous_bottom = control.absolute_position().y + control.size().height;
            }
            let main_job = editor("Main job");
            assert!(main_job.absolute_position().y >= basic.absolute_position().y + basic.size().height,
                "Inline input editors must not overlap");
            let tp = editor("TP");
            let viewport = editor("Job, spell, weapon skill & TP inputs");
            assert!(tp.absolute_position().y + tp.size().height <= viewport.absolute_position().y + viewport.size().height,
                "TP must fit in the compact basic-input group");
            assert!(editor("enemy Defense").size().width >= 56.0,
                "Enemy fields must show a four-digit value without clipping");
            assert!(editor("enemy VIT").size().width >= 48.0,
                "Paired enemy attributes must retain readable values");
            let resistance = editor("Resistance rank");
            let enemy_viewport = editor("Enemy Inputs inputs");
            assert!(resistance.size().height >= 20.0 &&
                resistance.absolute_position().y + resistance.size().height <= enemy_viewport.absolute_position().y + enemy_viewport.size().height,
                "The final enemy field must remain visible without inner scrolling");
            for label in ["Master level", "Main job", "Subjob", "Aftermath level", "Spell", "Weapon skill", "TP",
                "Special Toggles inputs", "Enemy preset", "Enemy level / location", "enemy Defense", "enemy Magic Evasion",
                "enemy Magic Defense", "enemy Magic DT%", "enemy VIT", "enemy AGI", "enemy INT", "enemy MND", "enemy CHR", "Resistance rank"] {
                let field = editor(label);
                fixed_horizontal(&field, &format!("Quicklook/{label}"), &quick_viewport, &mut baseline);
            }
            for label in ["Master level", "Main job", "Subjob", "Aftermath level", "Spell", "Weapon skill", "TP", "enemy Defense", "enemy VIT"] {
                fixed_input_label(label, &editor(label), &quick_viewport, &mut baseline);
            }
            quicklook_content_height_geometry(&desktop, &quick_viewport, &mut baseline);
            // Refresh once with different real contents, not once per resize.
            if width == 760.0 && profile == "Black Mage" {
                desktop.window.invoke_action("stats-quicklook".into());
                finish(&desktop, 2);
            }
            fixed_stat_geometry(&desktop, &mut baseline, &[]);
        }
    }
    // Stress the displayed content itself without changing calculations or
    // formatting rules. Only vertical allocation may react to longer values.
    let mut base = desktop.window.get_stats_base().iter().collect::<Vec<_>>();
    let mut physical = desktop.window.get_stats_physical().iter().collect::<Vec<_>>();
    base[0].value = "123456789012345678901234567890.125 %".into();
    physical[0].value = "987654321098765432109876543210.875 / 100 %".into();
    let oversized_labels = [format!("{}: {}", base[0].label, base[0].value), format!("{}: {}", physical[0].label, physical[0].value)];
    desktop.window.set_stats_base(slint::ModelRc::new(slint::VecModel::from(base)));
    desktop.window.set_stats_physical(slint::ModelRc::new(slint::VecModel::from(physical)));
    for (width, height) in [(760.0, 440.0), (720.0, 560.0), (1440.0, 1080.0), (720.0, 440.0)] {
        desktop.window.window().set_size(slint::LogicalSize::new(width, height));
        fixed_stat_geometry(&desktop, &mut baseline, &oversized_labels);
    }
}

#[test]
fn gear_keyboard_selection_stays_visible_after_virtual_realization_and_user_scrolls_independently() {
    fn realize(desktop: &Desktop) {
        for _ in 0..3 {
            let _ = desktop.window.root_element().visit_descendants(|_| std::ops::ControlFlow::<()>::Continue(()));
            // Virtual layout can correct viewport-y after input has already
            // drained its changed handlers. Flush that supported lifecycle too.
            i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        }
    }
    fn inside(element: &ElementHandle, viewport: &ElementHandle) -> bool {
        let position = element.absolute_position();
        let size = element.size();
        let clip = viewport.absolute_position();
        let extent = viewport.size();
        element.is_valid() && size.width > 0.0 && size.height > 0.0 &&
            position.x >= clip.x - 1.0 && position.y >= clip.y - 1.0 &&
            position.x + size.width <= clip.x + extent.width + 1.0 &&
            position.y + size.height <= clip.y + extent.height + 1.0
    }
    fn expose_outer(desktop: &Desktop, element: &ElementHandle) {
        let outer = layout_viewport(&desktop.window.root_element(), "Quicklook content");
        for _ in 0..3 {
            if inside(element, &outer) { return; }
            let position = element.absolute_position();
            let size = element.size();
            let clip = outer.absolute_position();
            let extent = outer.size();
            let delta = if position.y < clip.y { clip.y - position.y }
                else { clip.y + extent.height - position.y - size.height };
            scroll_layout(desktop, &outer, delta);
            realize(desktop);
        }
        assert!(inside(element, &outer),
            "The real outer wheel must expose the complete interaction target");
    }
    fn click(desktop: &Desktop, element: &ElementHandle) {
        expose_outer(desktop, element);
        let position = element.absolute_position();
        let size = element.size();
        let position = slint::LogicalPosition::new(position.x + size.width / 2.0, position.y + size.height / 2.0);
        let window = desktop.window.window();
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        realize(desktop);
    }
    fn radio(choices: &ElementHandle, name: &str) -> Option<ElementHandle> {
        let row = choices.query_descendants().find_all().into_iter().find(|row|
            row.accessible_role() == Some(AccessibleRole::RadioButton) &&
            row.accessible_label().is_some_and(|label| label == name));
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        row
    }
    fn offset(choices: &ElementHandle) -> f32 {
        let bar = choices.query_descendants().find_all().into_iter().find(|control|
            control.accessible_role() == Some(AccessibleRole::Slider) &&
            control.accessible_label().is_some_and(|label| label == "Available equipment vertical scrollbar"))
            .expect("The overflowing production gear list must expose its scrollbar");
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        bar.accessible_value().unwrap().as_str().parse().unwrap()
    }
    fn wheel(desktop: &Desktop, viewport: &ElementHandle, delta: f32) {
        let position = viewport.absolute_position();
        let position = slint::LogicalPosition::new(position.x + 2.0, position.y + viewport.size().height / 2.0);
        let outer = layout_viewport(&desktop.window.root_element(), "Quicklook content");
        assert!(position.x >= outer.absolute_position().x && position.y >= outer.absolute_position().y &&
            position.x <= outer.absolute_position().x + outer.size().width &&
            position.y <= outer.absolute_position().y + outer.size().height,
            "The actual inner wheel target must remain within the outer clip");
        desktop.window.window().dispatch_event(WindowEvent::PointerScrolled {
            position, delta_x: 0.0, delta_y: delta,
        });
        realize(desktop);
    }

    i_slint_backend_testing::init_no_event_loop();
    let fixture = ui_support::cases("ui.profile").into_iter().find(|fixture| fixture["id"] == "ui.profile.10.ninja").unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let mut paths = AppPaths::from_parts(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_os_str()),
        std::path::Path::new("/unused"), std::path::Path::new("/")).unwrap();
    paths.defaults = sandbox.path().join("defaults.json");
    fs::write(&paths.defaults, serde_json::to_vec(&fixture["input"]["profile_document"]).unwrap()).unwrap();
    let desktop = Desktop::with_random(Resources { paths, catalog: ui_support::catalog() }, RandomSources::from_seeds(11, 11)).unwrap();
    desktop.window.show().unwrap();
    desktop.window.window().set_size(slint::LogicalSize::new(760.0, 900.0));
    // TestingBackend has no native activation/rendering on show. Model the
    // backend activation event, then use normal pointer/key/wheel input only.
    desktop.window.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    realize(&desktop);
    let slot = desktop.window.get_quick_slots().row_data(Slot::Main.index()).unwrap();
    let slot = ElementHandle::find_by_accessible_label(&desktop.window, &format!("{}: {}", slot.label, slot.name))
        .find(|control| control.accessible_role() == Some(AccessibleRole::Button)).unwrap();
    click(&desktop, &slot);
    let items = desktop.window.get_quick_items();
    assert!(items.row_count() >= 30, "Use a large real catalog/controller radio list, not synthetic delegates");
    let first = items.row_data(0).unwrap();
    let last = items.row_data(items.row_count() - 1).unwrap();
    let choices = ElementHandle::find_by_accessible_label(&desktop.window, "Available equipment")
        .find(|control| control.accessible_role() == Some(AccessibleRole::List)).unwrap();
    let viewport = choices.query_descendants().match_inherits("Flickable").find_first().unwrap();
    expose_outer(&desktop, &viewport);
    assert!(inside(&viewport, &layout_viewport(&desktop.window.root_element(), "Quicklook content")),
        "The tall production window must expose the entire inner clip");
    wheel(&desktop, &viewport, 1_000_000.0);
    let first_row = radio(&choices, first.name.as_str()).unwrap();
    assert!(inside(&first_row, &viewport));
    click(&desktop, &first_row);
    assert_eq!(desktop.controller.borrow().ui.gear.equipped[GearSource::Quicklook.index()][Slot::Main.index()].index() as i32, first.id);

    for (pressed, expected, end) in [(Key::End, &last, true), (Key::Home, &first, false)] {
        let text: slint::SharedString = pressed.into();
        desktop.window.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        desktop.window.window().dispatch_event(WindowEvent::KeyReleased { text });
        realize(&desktop);
        assert_eq!(desktop.controller.borrow().ui.gear.equipped[GearSource::Quicklook.index()][Slot::Main.index()].index() as i32,
            expected.id, "Keyboard navigation must select the actual controller/catalog ID");
        let row = radio(&choices, expected.name.as_str()).expect("The selected radio must remain realized");
        let viewport = choices.query_descendants().match_inherits("Flickable").find_first().unwrap();
        let outer = layout_viewport(&desktop.window.root_element(), "Quicklook content");
        assert!(inside(&row, &viewport),
            "{} must fit fully after virtual realization: row={:?}/{:?}, inner={:?}/{:?}",
            expected.name, row.absolute_position(), row.size(), viewport.absolute_position(), viewport.size());
        assert!(inside(&row, &outer),
            "{} must also fit fully within the real outer clip", expected.name);
        if end {
            let row_height = row.size().height;
            let remainder = viewport.size().height % row_height;
            assert!(remainder > 1.0 && row_height - remainder > 1.0,
                "Exercise the fractional-row boundary: inner height={}, row height={row_height}", viewport.size().height);
        }
    }
    assert!(offset(&choices).abs() <= 1.0, "Home must reach the real top before explicit user scrolling");
    let viewport = choices.query_descendants().match_inherits("Flickable").find_first().unwrap();
    let row_height = radio(&choices, first.name.as_str()).unwrap().size().height;
    wheel(&desktop, &viewport, -row_height * 7.0);
    realize(&desktop);
    assert!(offset(&choices) >= row_height * 4.0,
        "Explicit user scrolling must persist through deferred layout, not snap back to the focused row");
    assert_eq!(desktop.controller.borrow().ui.gear.equipped[GearSource::Quicklook.index()][Slot::Main.index()].index() as i32,
        first.id, "Scrolling moves the viewport without changing the equipped selection");
    assert!(!radio(&choices, first.name.as_str()).is_some_and(|row| inside(&row, &viewport)),
        "The unchanged selected first row may leave the clip when the user explicitly scrolls away");
}
