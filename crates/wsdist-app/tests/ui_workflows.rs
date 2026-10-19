use std::collections::BTreeMap;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model};
use wsdist_app::controller::Desktop;
use wsdist_app::exchange::GearSource;
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
    desktop.window.invoke_action("distribution".into());
    assert!(desktop.window.get_busy());
    assert!(desktop.window.get_basic_controls().iter().all(|row| !row.enabled));
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "2500".into());
    desktop.window.invoke_action("quick-ws".into());
    assert_eq!(desktop.controller.borrow().ui.text("defaults_tp_entry"), "2300");
    assert!(!desktop.window.get_error_message().is_empty());
    desktop.window.invoke_action("cancel".into());
    finish(&desktop, 2);
    assert_eq!(desktop.controller.borrow().cancelled_jobs, 1);
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

fn layout_viewport(desktop: &Desktop, label: &str) -> ElementHandle {
    let content = ElementHandle::find_by_accessible_label(&desktop.window, label).next().unwrap();
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

fn fixed_input_label(desktop: &Desktop, label: &str, field: &ElementHandle, viewport: &ElementHandle,
    baseline: &mut BTreeMap<String, (f32, f32)>) {
    let row = ElementHandle::find_by_element_type_name(&desktop.window, "ControlEditor")
        .find(|row| row.query_descendants().find_all().iter()
            .any(|child| child.accessible_label().is_some_and(|text| text == label)))
        .unwrap_or_else(|| panic!("The visible {label} editor must have its label row"));
    let field_position = field.absolute_position();
    let text = row.query_descendants().match_type_name("Text").find_all().into_iter()
        .find(|text| {
            let position = text.absolute_position();
            let size = text.size();
            size.width > 0.0 && position.x < field_position.x - 1.0 &&
                position.x + size.width <= field_position.x + 1.0 &&
                (position.y - field_position.y).abs() <= 3.0
        }).unwrap_or_else(|| panic!("{label} must retain a readable, non-overlapping label canvas"));
    fixed_horizontal(&row, &format!("input row/{label}"), viewport, baseline);
    fixed_horizontal(&text, &format!("input label/{label}"), viewport, baseline);
}

fn scroll_layout(desktop: &Desktop, label: &str, delta: f32) {
    let viewport = layout_viewport(desktop, label);
    let position = viewport.absolute_position();
    desktop.window.window().dispatch_event(slint::platform::WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(position.x + 2.0, position.y + viewport.size().height / 2.0),
        delta_x: 0.0, delta_y: delta,
    });
}

fn visible_stat_row(desktop: &Desktop, label: &str) -> ElementHandle {
    scroll_layout(desktop, "Stats content", 10000.0);
    for _ in 0..24 {
        let viewport = layout_viewport(desktop, "Stats content");
        if let Some(row) = ElementHandle::find_by_accessible_label(&desktop.window, label)
            .find(|row| row.accessible_role() == Some(AccessibleRole::ListItem) &&
                row.absolute_position().y >= viewport.absolute_position().y - 1.0 &&
                row.absolute_position().y + row.size().height <= viewport.absolute_position().y + viewport.size().height + 1.0) {
            return row;
        }
        scroll_layout(desktop, "Stats content", -160.0);
    }
    panic!("The Stats row {label:?} must be reachable through the outer viewport");
}

fn fixed_stat_geometry(desktop: &Desktop, baseline: &mut BTreeMap<String, (f32, f32)>) {
    desktop.window.set_active_tab(3);
    let groups = [desktop.window.get_stats_base(), desktop.window.get_stats_physical(), desktop.window.get_stats_magical(),
        desktop.window.get_stats_multi(), desktop.window.get_stats_haste(), desktop.window.get_stats_defensive(), desktop.window.get_stats_other()];
    assert_eq!(groups.iter().map(|rows| rows.row_count()).sum::<usize>(), 53, "All player Stats rows must remain available");
    let mut group_boxes = Vec::with_capacity(groups.len());
    for (index, rows) in groups.into_iter().enumerate() {
        let mut previous: Option<ElementHandle> = None;
        for stat in rows.iter() {
            let row = visible_stat_row(desktop, &format!("{}: {}", stat.label, stat.value));
            if let Some(previous) = previous.as_ref() {
                assert!(row.absolute_position().y >= previous.absolute_position().y + previous.size().height - 1.0,
                    "{} must not overlap the preceding Stats row", stat.label);
            } else {
                let row_label = format!("{}: {}", stat.label, stat.value);
                let group = ElementHandle::find_by_element_type_name(&desktop.window, "StatGroup")
                    .find(|group| group.query_descendants().find_all().iter()
                        .any(|child| child.accessible_role() == Some(AccessibleRole::ListItem) &&
                            child.accessible_label().is_some_and(|label| label == row_label)))
                    .expect("Each visible Stats row must fit its group");
                fixed_horizontal(&group, &format!("Stats/group/{index}"), &layout_viewport(desktop, "Stats content"), baseline);
                group_boxes.push(group);
            }
            let value = row.query_descendants().match_type_name("Text").find_all().into_iter()
                .find(|text| text.accessible_label().is_some_and(|label| label == stat.label) &&
                    text.accessible_value().is_some_and(|value| value == stat.value))
                .unwrap_or_else(|| panic!("{} must expose its displayed value", stat.label));
            let label = row.query_descendants().match_type_name("Text").find_all().into_iter()
                .find(|text| text.accessible_label().is_some_and(|label| label == stat.label) && text.accessible_value().is_none())
                .unwrap_or_else(|| panic!("{} must retain its visible label", stat.label));
            assert!(value.size().width >= (if index == 0 { 48.0 } else { 64.0 }),
                "{} must reserve readable value space independently of its digits/units", stat.label);
            assert!(label.size().width >= 20.0 && label.absolute_position().x + label.size().width + 5.0 <= value.absolute_position().x,
                "{} label and value must not overlap", stat.label);
            for text in [&label, &value] {
                assert!(text.absolute_position().y >= row.absolute_position().y - 1.0 &&
                    text.absolute_position().y + text.size().height <= row.absolute_position().y + row.size().height + 1.0,
                    "{} must keep its wrapped label/value inside the row", stat.label);
            }
            let viewport = layout_viewport(desktop, "Stats content");
            for (part, element) in [("row", &row), ("label", &label), ("value", &value)] {
                fixed_horizontal(element, &format!("Stats/{}/{part}", stat.label), &viewport, baseline);
            }
            previous = Some(row);
        }
    }
    for (upper, lower) in [(&group_boxes[0..3], &group_boxes[3..5]), (&group_boxes[3..5], &group_boxes[5..7])] {
        let bottom = upper.iter().map(|group| group.absolute_position().y + group.size().height).fold(f32::NEG_INFINITY, f32::max);
        assert!(lower.iter().all(|group| group.absolute_position().y >= bottom - 1.0),
            "Growing Stats rows must move the following group row down, not overlap it");
    }
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
            scroll_layout(&desktop, "Quicklook content", 10000.0);
            let editor = |label: &str| ElementHandle::find_by_accessible_label(&desktop.window, label).next()
                .unwrap_or_else(|| panic!("{label} must remain visible for {profile} at {width}x{height}"));
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
            let quick_viewport = layout_viewport(&desktop, "Quicklook content");
            for label in ["Master level", "Main job", "Subjob", "Aftermath level", "Spell", "Weapon skill", "TP",
                "Special Toggles inputs", "Enemy preset", "Enemy level / location", "enemy Defense", "enemy Magic Evasion",
                "enemy Magic Defense", "enemy Magic DT%", "enemy VIT", "enemy AGI", "enemy INT", "enemy MND", "enemy CHR", "Resistance rank"] {
                let field = editor(label);
                fixed_horizontal(&field, &format!("Quicklook/{label}"), &quick_viewport, &mut baseline);
            }
            for label in ["Master level", "Main job", "Subjob", "Aftermath level", "Spell", "Weapon skill", "TP", "enemy Defense", "enemy VIT"] {
                fixed_input_label(&desktop, label, &editor(label), &quick_viewport, &mut baseline);
            }
            // Refresh once with different real contents, not once per resize.
            if width == 760.0 && profile == "Black Mage" {
                desktop.window.invoke_action("stats-quicklook".into());
                finish(&desktop, 2);
            }
            fixed_stat_geometry(&desktop, &mut baseline);
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
        fixed_stat_geometry(&desktop, &mut baseline);
        for label in &oversized_labels {
            let row = visible_stat_row(&desktop, label);
            let value = row.query_descendants().match_type_name("Text").find_all().into_iter()
                .find(|text| text.accessible_value().is_some()).unwrap();
            assert!(value.size().height > 24.0, "Oversized displayed values must gain vertical space rather than clip or widen");
        }
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
        for _ in 0..3 {
            let outer = layout_viewport(desktop, "Quicklook content");
            if inside(element, &outer) { return; }
            let position = element.absolute_position();
            let size = element.size();
            let clip = outer.absolute_position();
            let extent = outer.size();
            let delta = if position.y < clip.y { clip.y - position.y }
                else { clip.y + extent.height - position.y - size.height };
            scroll_layout(desktop, "Quicklook content", delta);
            realize(desktop);
        }
        assert!(inside(element, &layout_viewport(desktop, "Quicklook content")),
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
        let outer = layout_viewport(desktop, "Quicklook content");
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
    assert!(inside(&viewport, &layout_viewport(&desktop, "Quicklook content")),
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
        let outer = layout_viewport(&desktop, "Quicklook content");
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
