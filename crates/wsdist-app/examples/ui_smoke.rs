//! Development-only native Winit/software smoke. Never initializes the testing backend.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::{Duration, Instant};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::{Value, json};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};
use wsdist_app::controller::Desktop;
use wsdist_app::exchange::{GearSource, ImportOutcome};
use wsdist_app::jobs::WorkResult;
use wsdist_app::resources::{AppPaths, Resources, load_resources_at};
use wsdist_app::{GearSlotRow, InputRow, InputWidgetKind, ProgressWindow, ResultWindow};
use wsdist_core::buffs::aggregate_buffs;
use wsdist_core::model::{CalcError, Slot};
use wsdist_core::rng::RandomSources;
use wsdist_core::stats::Stats;
use wsdist_fixtures::Value as FixtureValue;

#[path = "../tests/resource_support/mod.rs"]
mod resource_support;

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

fn main() -> Result<()> {
    let mut output = PathBuf::from("target/ui-smoke");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().ok_or("--output needs a directory")?.into(),
            _ => return Err(format!("Unknown argument {arg:?}").into()),
        }
    }
    fs::create_dir_all(&output)?;
    let source = AppPaths::discover()?;
    let sandbox = tempfile::tempdir()?;
    let paths = AppPaths::from_parts(Some(sandbox.path().as_os_str()), Path::new("/unused"), Path::new("/"))?;
    copy_resources(&source, &paths)?;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy");
    let index = wsdist_fixtures::read_json_value(&fixtures.join("ui.json"))
        .map_err(|error| -> Box<dyn Error> { error })?;
    if index["schema_version"].as_u64() != Some(3) { return Err("Unsupported UI fixture index schema".into()); }
    let ninja = fixture(&fixtures, &index, "ui.profile.10.ninja")?;
    // The original worktree is untouched, even when the workflow saves/reloads.
    fs::write(&paths.defaults, serde_json::to_vec(&ninja["input"]["profile_document"])?)?;
    let resources = load_resources_at(paths)?;
    let default = fixture(&fixtures, &index, "ui.profile.22.default")?;
    let missing_icon = remove_requested_icon(&resources, &default)?;
    let desktop = Rc::new(Desktop::with_random(resources, RandomSources::from_seed(11))?);
    desktop.window.show()?;
    let outcome = Rc::new(RefCell::new(None));
    let completed = Rc::clone(&outcome);
    let app = Rc::clone(&desktop);
    let task = slint::spawn_local(async move {
        let result = workflow(&app, &output, &fixtures, &index, &ninja, &missing_icon).await.map_err(|error| error.to_string());
        app.controller.borrow_mut().shutdown();
        *completed.borrow_mut() = Some(result);
        if let Err(error) = slint::quit_event_loop() { eprintln!("Exit smoke event loop: {error}"); }
    })?;
    slint::run_event_loop()?;
    drop(task);
    match outcome.borrow_mut().take().ok_or("Native event loop exited before smoke completion")? {
        Ok(()) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn fixture(root: &Path, index: &FixtureValue, id: &str) -> Result<FixtureValue> {
    let entry = index["cases"].as_array().ok_or("Invalid UI index")?.iter()
        .find(|entry| entry["id"] == id).ok_or_else(|| format!("Missing fixture {id}"))?;
    wsdist_fixtures::read_value(&root.join(entry["path"].as_str().ok_or("Fixture path missing")?), Some(entry["content_xxh3_128"].as_str().ok_or("Fixture XXH3-128 content hash missing")?))
        .map_err(|error| -> Box<dyn Error> { error })
}

fn copy_resources(source: &AppPaths, destination: &AppPaths) -> Result<()> {
    fs::create_dir(&destination.data)?;
    for name in ["buffs.json5", "enemies.json5", "pup-attachments.json5"] {
        fs::copy(source.data.join(name), destination.data.join(name))?;
    }
    resource_support::copy_gear(&source.data.join("gear"), &destination.data.join("gear"))?;
    fs::copy(&source.item_list, &destination.item_list)?;
    fs::create_dir(&destination.icons)?;
    for entry in fs::read_dir(&source.icons)? {
        let entry = entry?;
        if entry.file_type()?.is_file() { fs::copy(entry.path(), destination.icons.join(entry.file_name()))?; }
    }
    Ok(())
}

struct MissingIcon {
    name: String,
    path: PathBuf,
    bytes: Vec<u8>,
}

fn remove_requested_icon(resources: &Resources, default: &FixtureValue) -> Result<MissingIcon> {
    let name = default["expected"]["value"]["initial_state"]["equipped_sets"]["quicklook"]["main"]["piece"]["effective"]["name2"]
        .as_str().ok_or("Default main weapon is missing")?;
    let id = resources.catalog.by_name2(name).ok_or("Default main weapon is absent from the catalog")?;
    let alias = resources.catalog.alias_by_full_name(&resources.catalog.piece(id).name)
        .ok_or("Default main weapon has no pointed icon alias")?;
    if alias.id.as_ref() == "23937" { return Err("Missing-icon probe must preserve the native window icon".into()); }
    let path = resources.paths.icons.join(format!("{}.png", alias.id));
    let bytes = fs::read(&path)?;
    if !resources.paths.icons.join("23937.png").is_file() { return Err("Native window icon23937 is missing from the sandbox".into()); }
    fs::remove_file(&path)?;
    Ok(MissingIcon { name: name.to_owned(), path, bytes })
}

async fn pause(milliseconds: u64) {
    let ready = Rc::new(Cell::new(false));
    let wake: Rc<RefCell<Option<Waker>>> = Rc::new(RefCell::new(None));
    let finished = Rc::clone(&ready);
    let timer_wake = Rc::clone(&wake);
    slint::Timer::single_shot(Duration::from_millis(milliseconds), move || {
        finished.set(true);
        if let Some(waker) = timer_wake.borrow_mut().take() { waker.wake(); }
    });
    std::future::poll_fn(move |context| {
        if ready.get() { Poll::Ready(()) }
        else { *wake.borrow_mut() = Some(context.waker().clone()); Poll::Pending }
    }).await;
}

fn assert_native_logical_size(window: &slint::Window, width: f32, height: f32, phase: &str) -> Result<()> {
    let actual = window.size();
    let scale = window.scale_factor();
    if (actual.width as f32 / scale - width).abs() > 1.0 ||
        (actual.height as f32 / scale - height).abs() > 1.0 {
        return Err(format!("{phase}: native client did not settle at {width}x{height} logical pixels: actual={actual:?}, scale={scale}").into());
    }
    Ok(())
}

fn key(window: &slint::Window, text: impl Into<SharedString>) {
    let text = text.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

fn control_key(window: &slint::Window, text: &str) {
    window.dispatch_event(WindowEvent::KeyPressed { text: Key::Control.into() });
    key(window, text);
    window.dispatch_event(WindowEvent::KeyReleased { text: Key::Control.into() });
}

async fn tab(desktop: &Desktop, index: i32) -> Result<()> {
    let before = desktop.window.get_active_tab();
    // Core discards focus in controls outside their Flickable clip. Clicking an
    // already-current header doesn't trigger MainWindow's keyboard.focus()
    // binding; changing active-tab does. Cycle through a safe tab and BACK
    // before the Ctrl macro, so only the shortcut selects its requested target.
    let other = if before == 0 { 1 } else { 0 };
    for candidate in [other, before] {
        let label = match candidate { 0 => "Quicklook", 1 => "Optimize", 2 => "Simulate", _ => "Player Stats" };
        let header = element(&desktop.window, label, AccessibleRole::Tab)?;
        let origin = header.absolute_position();
        let size = header.size();
        pointer_click(desktop.window.window(), slint::LogicalPosition::new(
            origin.x + size.width / 2.0, origin.y + size.height / 2.0)).await?;
        pause(80).await;
        let clicked = desktop.window.get_active_tab();
        if clicked != candidate {
            return Err(format!("Native tab focus precondition failed before Ctrl+{}: clicked={label:?}, expected={candidate}, actual={clicked}, original={before}, target={index}",
                index + 1).into());
        }
    }
    control_key(desktop.window.window(), &(index + 1).to_string());
    pause(80).await;
    let after = desktop.window.get_active_tab();
    if after != index {
        let candidates = ElementHandle::find_by_accessible_label(&desktop.window, "Available equipment")
            .find(|scope| scope.accessible_role() == Some(AccessibleRole::List))
            .map(|scope| json!({"rectangle": rectangle(&scope), "valid": scope.is_valid(),
                "description": scope.accessible_description().map(|description| description.to_string()),
                "item_count": scope.accessible_item_count()}));
        return Err(format!("Native tab transition Ctrl+{} did not select tab {index}: before={before}, after={after}, window_visible={}, client={:?}, candidate_scope={candidates:?}",
            index + 1, desktop.window.window().is_visible(), desktop.window.window().size()).into());
    }
    Ok(())
}

fn element(component: &impl ComponentHandle, label: &str, role: AccessibleRole) -> Result<ElementHandle> {
    ElementHandle::find_by_accessible_label(component, label)
        .find(|element| element.accessible_role() == Some(role))
        .ok_or_else(|| format!("Visible accessible {role:?} {label:?} not found").into())
}

async fn scroll_content(desktop: &Desktop, delta: f32) -> Result<()> {
    let label = match desktop.window.get_active_tab() {
        0 => "Quicklook content", 1 => "Optimize content", 2 => "Simulate content", _ => "Stats content",
    };
    let viewport = element(&desktop.window, label, AccessibleRole::List)?;
    let position = viewport.absolute_position();
    desktop.window.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(position.x + 2.0, position.y + viewport.size().height * 0.5),
        delta_x: 0.0, delta_y: delta,
    });
    pause(80).await;
    Ok(())
}

async fn visible_element(desktop: &Desktop, label: &str, role: AccessibleRole) -> Result<ElementHandle> {
    if let Ok(found) = element(&desktop.window, label, role) { return Ok(found); }
    // Wheel the outer margin, not an inner candidate/input list. Overlapping
    // steps cannot skip a short control in a compact viewport.
    scroll_content(desktop, 10000.0).await?;
    for _ in 0..16 {
        if let Ok(found) = element(&desktop.window, label, role) { return Ok(found); }
        scroll_content(desktop, -240.0).await?;
    }
    element(&desktop.window, label, role)
}

async fn click(desktop: &Desktop, label: &str) -> Result<()> {
    let button = visible_element(desktop, label, AccessibleRole::Button).await?;
    if button.accessible_enabled() == Some(false) { return Err(format!("Button {label:?} disabled unexpectedly").into()); }
    // Accessibility actions go through the actual native component's callback.
    button.invoke_accessible_default_action();
    pause(25).await;
    Ok(())
}

async fn finished(desktop: &Desktop, previous: usize) -> Result<()> {
    let start = Instant::now();
    while desktop.controller.borrow().is_busy() {
        if start.elapsed() > Duration::from_secs(120) { return Err("Worker did not complete within 120 seconds".into()); }
        pause(16).await;
    }
    let state = desktop.controller.borrow();
    if state.completed_jobs != previous + 1 {
        return Err(format!("Computation failed: {:?}", state.last_error).into());
    }
    if let Some(error) = &state.last_error { return Err(format!("Completed computation reported an error: {error}").into()); }
    drop(state);
    pause(80).await;
    Ok(())
}

async fn compute(desktop: &Desktop, label: &str) -> Result<()> {
    let previous = desktop.controller.borrow().completed_jobs;
    click(desktop, label).await?;
    finished(desktop, previous).await
}

fn snapshot(component: &impl ComponentHandle, output: &Path, name: &str, screenshots: &mut Vec<Value>) -> Result<()> {
    let mut pixels = component.window().take_snapshot()?;
    // Slint1.16 software take_snapshot copies RGB but leaves alpha at zero
    // (renderer-software/lib.rs:1230-1238). The native software scene is opaque.
    // Normalize only that unused alpha channel; preserve every rendered RGB byte.
    for pixel in pixels.make_mut_slice() { pixel.a = 255; }
    let path = output.join(format!("{name}.png"));
    image::save_buffer_with_format(&path, pixels.as_bytes(), pixels.width(), pixels.height(), image::ColorType::Rgba8, image::ImageFormat::Png)?;
    screenshots.push(json!({"file": format!("{name}.png"), "width": pixels.width(), "height": pixels.height(), "scale_factor": component.window().scale_factor()}));
    println!("Snapshot {name}: {} x {}", pixels.width(), pixels.height());
    Ok(())
}

fn snapshot_result(desktop: &Desktop, output: &Path, name: &str, screenshots: &mut Vec<Value>) -> Result<()> {
    let state = desktop.controller.borrow();
    let window = state.result_windows().last().ok_or("No completed result window")?;
    snapshot(window, output, name, screenshots)
}

fn displayed_stats(desktop: &Desktop) -> wsdist_fixtures::Map {
    let mut values = wsdist_fixtures::Map::new();
    for model in [desktop.window.get_stats_base(), desktop.window.get_stats_physical(), desktop.window.get_stats_magical(),
        desktop.window.get_stats_multi(), desktop.window.get_stats_haste(), desktop.window.get_stats_defensive(), desktop.window.get_stats_other()] {
        for row in model.iter() { values.insert(row.label.to_string(), FixtureValue::String(row.value.to_string())); }
    }
    values
}

fn assert_display(desktop: &Desktop, expected: &FixtureValue) -> Result<()> {
    let damage = format!("{} {}", expected["damage_label"].as_str().ok_or("Damage label absent")?, expected["damage"].as_str().ok_or("Damage value absent")?);
    let tp = format!("{} {}", expected["tp_label"].as_str().ok_or("TP label absent")?, expected["tp"].as_str().ok_or("TP value absent")?);
    if desktop.window.get_damage().as_str() != damage || desktop.window.get_tp_return().as_str() != tp {
        return Err(format!("Frozen displayed result mismatch: expected {damage:?} / {tp:?}, actual {:?} / {:?}", desktop.window.get_damage(), desktop.window.get_tp_return()).into());
    }
    Ok(())
}

fn assert_sets(desktop: &Desktop, expected: &FixtureValue) -> Result<()> {
    let state = desktop.controller.borrow();
    for source in GearSource::ALL {
        let name = match source { GearSource::Quicklook => "quicklook", GearSource::Tp => "tp", GearSource::Ws => "ws" };
        for slot in Slot::ALL {
            let actual = &state.ui.engine.catalog.piece(state.ui.gear.equipped[source.index()][slot.index()]).name2;
            let captured = &expected[name][slot.name()]["piece"]["original"]["name2"];
            if captured != actual.as_ref() { return Err(format!("{name}/{}: actual {actual}, expected {captured}", slot.name()).into()); }
        }
    }
    Ok(())
}

fn dropdown_text(owner: &ElementHandle, label: &str) -> Result<ElementHandle> {
    let origin = owner.absolute_position();
    let size = owner.size();
    owner.query_descendants().match_type_name("Text").find_all().into_iter()
        .filter(|text| {
            let position = text.absolute_position();
            let text_size = text.size();
            text_size.width > 0.0 && text_size.height > 0.0 && position.x >= origin.x - 0.5 &&
                position.y >= origin.y - 0.5 && position.x + text_size.width <= origin.x + size.width + 0.5 &&
                position.y + text_size.height <= origin.y + size.height + 0.5
        })
        .max_by(|left, right| left.size().width.total_cmp(&right.size().width))
        .ok_or_else(|| format!("{label}: selected/option Text descendant is absent").into())
}

fn dropdown_options(popup: &ElementHandle) -> Vec<ElementHandle> {
    popup.query_descendants().find_all().into_iter().filter(|option|
        option.accessible_role() == Some(AccessibleRole::ListItem) && option.accessible_label().is_some()).collect()
}

fn dropdown_option_visible(option: &ElementHandle, viewport: &ElementHandle) -> bool {
    let position = option.absolute_position();
    let clip = viewport.absolute_position();
    let size = option.size();
    size.width > 0.0 && size.height > 0.0 && position.x >= clip.x - 0.5 && position.y >= clip.y - 0.5 &&
        position.x + size.width <= clip.x + viewport.size().width + 0.5 &&
        position.y + size.height <= clip.y + viewport.size().height + 0.5
}

fn dropdown_popup(desktop: &Desktop, label: &str) -> Result<(ElementHandle, ElementHandle)> {
    let popup = element(&desktop.window, &format!("{label} options"), AccessibleRole::List)?;
    let viewport = popup.query_descendants().match_inherits("Flickable").find_all().into_iter()
        .max_by(|left, right| left.size().width.total_cmp(&right.size().width))
        .ok_or_else(|| format!("{label}: active options have no actual native scrolling viewport"))?;
    Ok((popup, viewport))
}

fn dropdown_native_position(window: &slint::Window, position: slint::LogicalPosition) -> Result<slint::LogicalPosition> {
    use slint::private_unstable_api::re_exports::{ItemRc, WindowInner};
    // Slint1.16 ElementHandle positions stop at PopupWindow boundaries, even
    // when Winit embeds the popup in the main window. Its single_click/scroll
    // helpers have the same limitation. Obtain the real, possibly edge-adjusted
    // child origin instead of inferring it from the combo's requested position.
    let inner = WindowInner::from_pub(window);
    let popups = inner.active_popups();
    let popup = popups.last().ok_or("Native dropdown input has no active popup")?;
    let root = ItemRc::new_root(popup.component.clone());
    let offset = root.map_to_native_window(Default::default()) - root.map_to_window(Default::default());
    Ok(slint::LogicalPosition::new(position.x + offset.x, position.y + offset.y))
}

fn assert_dropdown_value(desktop: &Desktop, label: &str, key: &str, expected: &str) -> Result<()> {
    let state = desktop.controller.borrow();
    let actual = state.ui.controls.get(key).or_else(|| state.extras.get(key))
        .ok_or_else(|| format!("{label}: controller input {key:?} is absent"))?;
    let combo = element(&desktop.window, label, AccessibleRole::Combobox)?;
    if actual.value.as_str() != Some(expected) || combo.accessible_value().as_deref() != Some(expected) ||
        combo.accessible_id().as_deref() != Some(key) {
        return Err(format!("{label}: native selection/public accessibility differs: expected={expected:?}, controller={}, accessible={:?}, id={:?}",
            actual.value, combo.accessible_value(), combo.accessible_id()).into());
    }
    Ok(())
}

fn selected_dropdown_option(desktop: &Desktop, label: &str, key: &str, expected: &str) -> Result<ElementHandle> {
    assert_dropdown_value(desktop, label, key, expected)?;
    let combo = element(&desktop.window, label, AccessibleRole::Combobox)?;
    let (popup, viewport) = dropdown_popup(desktop, label)?;
    let options = dropdown_options(&popup);
    let selected = options.iter().filter(|option| option.accessible_item_selected() == Some(true)).collect::<Vec<_>>();
    if combo.accessible_expanded() != Some(true) || selected.len() != 1 ||
        selected[0].accessible_label().as_deref() != Some(expected) ||
        selected[0].accessible_item_selectable() != Some(true) || !dropdown_option_visible(selected[0], &viewport) {
        let visible = options.iter().map(|option| json!({"label": option.accessible_label().map(|text| text.to_string()),
            "selected": option.accessible_item_selected(), "rectangle": rectangle(option)})).collect::<Vec<_>>();
        return Err(format!("{label}: current item {expected:?} is not the unique fully visible, accessible selected popup option: expanded={:?}, viewport={}, options={visible:?}",
            combo.accessible_expanded(), rectangle(&viewport)).into());
    }
    Ok(selected[0].clone())
}

fn assert_dropdown_closed(desktop: &Desktop, label: &str, key: &str, expected: &str) -> Result<()> {
    assert_dropdown_value(desktop, label, key, expected)?;
    if element(&desktop.window, label, AccessibleRole::Combobox)?.accessible_expanded() != Some(false) ||
        ElementHandle::find_by_accessible_label(&desktop.window, &format!("{label} options")).next().is_some() {
        return Err(format!("{label}: native popup remained open after selection/closing key").into());
    }
    Ok(())
}

async fn compact_dropdowns(desktop: &Desktop, output: &Path, screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    let original_tab = desktop.window.get_active_tab();
    // Master-level activation can hide and clear abilities. Restore the whole
    // input state, not only the three directly exercised values.
    let (original_controls, original_extras, original_sub_level) = {
        let state = desktop.controller.borrow();
        (state.ui.controls.clone(), state.extras.clone(), state.ui.sub_job_level)
    };
    let original_values = original_controls.iter().chain(&original_extras)
        .map(|(key, control)| (*key, control.value.clone())).collect::<BTreeMap<_, _>>();
    let mut proof = Vec::new();
    for (label, key_name, tab_index, name) in [
        ("Master level", "defaults_ml_combobox", 0, "master-level"),
        ("Resistance rank", "defaults_enemy_resist_rank_combobox", 0, "resistance-rank"),
        ("TP set metric", "optimize_tab.tp_metric_combobox", 1, "tp-metric"),
    ] {
        tab(desktop, tab_index).await?;
        let combo = geometry_element(desktop, label, AccessibleRole::Combobox).await?;
        let (initial_value, choices) = {
            let state = desktop.controller.borrow();
            let control = state.ui.controls.get(key_name).or_else(|| state.extras.get(key_name)).ok_or("Dropdown input is absent")?;
            (control.value.as_str().ok_or("Dropdown value is not text")?.to_owned(), control.choices.clone())
        };
        assert_dropdown_closed(desktop, label, key_name, &initial_value)?;
        let text = dropdown_text(&combo, label)?;
        let closed_inset = text.absolute_position().x - combo.absolute_position().x;
        let closed_center = text.absolute_position().y + text.size().height / 2.0 -
            combo.absolute_position().y - combo.size().height / 2.0;
        let closed = json!({"control": rectangle(&combo), "text": rectangle(&text), "text_inset": closed_inset,
            "text_center_offset_y": closed_center});
        let prefix = format!("00-dropdown-{name}");
        snapshot(&desktop.window, output, &format!("{prefix}-closed"), screenshots)?;
        let origin = combo.absolute_position();
        pointer_click(desktop.window.window(), slint::LogicalPosition::new(
            origin.x + combo.size().width / 2.0, origin.y + combo.size().height / 2.0)).await?;
        pause(80).await;
        let (popup, viewport) = dropdown_popup(desktop, label)?;
        let current = selected_dropdown_option(desktop, label, key_name, &initial_value)?;
        let current_text = dropdown_text(&current, label)?;
        let options = dropdown_options(&popup);
        let mut rows = Vec::new();
        for option in options.iter().filter(|option| dropdown_option_visible(option, &viewport)) {
            let text = dropdown_text(option, label)?;
            // Include the popup's outer padding: row-local padding alone is not
            // the inset of the displayed option relative to the closed control.
            let effective_inset = text.absolute_position().x - popup.absolute_position().x;
            let center = text.absolute_position().y + text.size().height / 2.0 -
                option.absolute_position().y - option.size().height / 2.0;
            if (option.size().height - combo.size().height).abs() > 0.5 ||
                (closed_inset - 11.0).abs() > 0.5 || (effective_inset - closed_inset).abs() > 0.5 ||
                closed_center.abs() > 0.5 || (center - closed_center).abs() > 0.5 ||
                option.accessible_item_selected().is_none() || option.accessible_item_selectable() != Some(true) {
                return Err(format!("{label}: popup option padding/geometry/accessibility differs from its selected display: closed={closed}, popup={}, option={}, text={}, effective_inset={effective_inset}, text_center_offset={center}",
                    rectangle(&popup), rectangle(option), rectangle(&text)).into());
            }
            rows.push(json!({"label": option.accessible_label().map(|text| text.to_string()), "selected": option.accessible_item_selected(),
                "option": rectangle(option), "text": rectangle(&text), "effective_text_inset": effective_inset,
                "row_local_text_inset": text.absolute_position().x - option.absolute_position().x, "text_center_offset_y": center}));
        }
        if rows.len() < 2 { return Err(format!("{label}: fewer than two fully visible native popup options").into()); }
        let native_origin = dropdown_native_position(desktop.window.window(), popup.absolute_position())?;
        let open = json!({"popup": rectangle(&popup), "viewport": rectangle(&viewport), "visible_options": rows,
            "coordinate_space": "popup-local", "native_origin": {"x": native_origin.x, "y": native_origin.y},
            "selected_text": rectangle(&current_text), "typography": "shared Fluent body font; Text bounds and screenshots"});
        snapshot(&desktop.window, output, &format!("{prefix}-open"), screenshots)?;
        println!("Dropdown {label}: closed={}px, option={}px, closed inset={closed_inset}px, popup effective inset={}px, text center offset={closed_center}px",
            combo.size().height, current.size().height, current_text.absolute_position().x - popup.absolute_position().x);

        let long_list = label == "Master level";
        let mut scrolling = Value::Null;
        let pointer_option = if long_list {
            if choices.len() < 40 || viewport.size().height >= choices.len() as f32 * combo.size().height {
                return Err("Master level no longer supplies a real long-list scrolling probe".into());
            }
            let before = options.iter().filter(|option| dropdown_option_visible(option, &viewport))
                .filter_map(|option| option.accessible_label().map(|text| text.to_string())).collect::<Vec<_>>();
            let origin = viewport.absolute_position();
            let position = slint::LogicalPosition::new(origin.x + viewport.size().width / 2.0,
                origin.y + viewport.size().height / 2.0);
            let position = dropdown_native_position(desktop.window.window(), position)?;
            PointerClip::new(desktop.window.window(), None).check(position, label)?;
            desktop.window.window().dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: -10000.0 });
            pause(100).await;
            assert_dropdown_value(desktop, label, key_name, &initial_value)?;
            let (scrolled_popup, scrolled_viewport) = dropdown_popup(desktop, label)?;
            let after_options = dropdown_options(&scrolled_popup);
            let after = after_options.iter().filter(|option| dropdown_option_visible(option, &scrolled_viewport))
                .filter_map(|option| option.accessible_label().map(|text| text.to_string())).collect::<Vec<_>>();
            if before == after { return Err("Master level native popup wheel did not move its visible options".into()); }
            let last = choices.last().ok_or("Master level choices are empty")?;
            let last_option = after_options.into_iter().find(|option| option.accessible_label().as_deref() == Some(last.as_str()) &&
                dropdown_option_visible(option, &scrolled_viewport)).ok_or("Native popup wheel did not reveal the final Master level option")?;
            scrolling = json!({"wheel_delta_y": -10000, "before_visible_labels": before, "after_visible_labels": after,
                "controller_value_unchanged": initial_value, "tail_option": rectangle(&last_option)});
            snapshot(&desktop.window, output, &format!("{prefix}-wheel-tail"), screenshots)?;
            last_option
        } else {
            options.into_iter().find(|option| dropdown_option_visible(option, &viewport) &&
                option.accessible_label().as_deref() != Some(initial_value.as_str()) &&
                option.accessible_label().as_deref() != choices.last().map(String::as_str))
                .ok_or_else(|| format!("{label}: no different fully visible option available for real pointer selection"))?
        };
        let pointer_value = pointer_option.accessible_label().ok_or("Pointer option lost its accessible label")?.to_string();
        let pointer_index = choices.iter().position(|choice| choice == &pointer_value).ok_or("Pointer option is not a real controller choice")?;
        let origin = pointer_option.absolute_position();
        let hit = dropdown_native_position(desktop.window.window(), slint::LogicalPosition::new(
            origin.x + pointer_option.size().width / 2.0, origin.y + pointer_option.size().height / 2.0))?;
        pointer_click(desktop.window.window(), hit).await?;
        assert_dropdown_closed(desktop, label, key_name, &pointer_value)?;
        key(desktop.window.window(), Key::Return);
        pause(80).await;
        let reopened = selected_dropdown_option(desktop, label, key_name, &pointer_value)?;
        let reopened_geometry = rectangle(&reopened);
        let mut keyboard = Vec::new();
        let mut index = pointer_index;
        // Eight steps cross the six-row long-list viewport; every newly
        // selected value must remain visible, not merely update the controller.
        for step in 0..if long_list { 9 } else { 2 } {
            let up = if long_list { step < 8 } else { step == 1 };
            index = if up { index.saturating_sub(1) } else { (index + 1).min(choices.len() - 1) };
            key(desktop.window.window(), if up { Key::UpArrow } else { Key::DownArrow });
            pause(60).await;
            let selected = selected_dropdown_option(desktop, label, key_name, &choices[index])?;
            keyboard.push(json!({"key": if up { "Up" } else { "Down" },
                "controller_value": choices[index], "selected_visible_option": rectangle(&selected)}));
        }
        snapshot(&desktop.window, output, &format!("{prefix}-keyboard-current-visible"), screenshots)?;
        key(desktop.window.window(), Key::Escape);
        pause(80).await;
        assert_dropdown_closed(desktop, label, key_name, &choices[index])?;
        key(desktop.window.window(), Key::Return);
        pause(80).await;
        selected_dropdown_option(desktop, label, key_name, &choices[index])?;
        key(desktop.window.window(), Key::Return);
        pause(80).await;
        assert_dropdown_closed(desktop, label, key_name, &choices[index])?;
        proof.push(json!({"label": label, "accessible_id": key_name, "initial_value": initial_value, "choices": choices.len(),
            "closed": closed, "open": open, "scrolling": scrolling, "pointer_selected_value": pointer_value,
            "selected_visible_after_return_reopen": reopened_geometry, "keyboard": keyboard,
            "closing": "Escape closes; Return reopens and closes; both preserve the immediately committed selection"}));
    }
    {
        let mut state = desktop.controller.borrow_mut();
        state.ui.controls = original_controls;
        state.extras = original_extras;
        state.ui.sub_job_level = original_sub_level;
        // Publish restored state without activating a job/profile or re-clearing
        // the restored ability values through the Master level callback.
        state.edit("defaults_tp_entry", original_values["defaults_tp_entry"].clone())?;
    }
    pause(80).await;
    let mut restored = 0;
    for model in [desktop.window.get_basic_controls(), desktop.window.get_ability_controls(), desktop.window.get_enemy_controls(),
        desktop.window.get_whm_controls(), desktop.window.get_food_controls(), desktop.window.get_brd_controls(),
        desktop.window.get_cor_controls(), desktop.window.get_geo_controls(), desktop.window.get_optimize_controls(),
        desktop.window.get_simulate_controls()] {
        for row in model.iter() {
            let expected = original_values.get(row.key.as_str()).ok_or("Restored input model contains an unknown key")?;
            if row.text.as_str() != expected.as_str().unwrap_or("") || row.checked != wsdist_core::model::python_truth(expected) {
                return Err(format!("Dropdown native proof failed to restore input {:?}", row.key).into());
            }
            restored += 1;
        }
    }
    tab(desktop, original_tab).await?;
    scroll_content(desktop, 10000.0).await?;
    checks.push(json!({"workflow": "native compact dropdown padding and selection parity", "scale_factor": desktop.window.window().scale_factor(),
        "controls": proof, "restored_input_count": original_values.len(), "restored_published_rows": restored,
        "restored_tab": original_tab, "restored_sub_job_level": original_sub_level,
        "input_isolation": "all controller input values, choices, hidden/enabled state and published models restored before subsequent workflows"}));
    Ok(())
}

fn assert_input_spacing(desktop: &Desktop) -> Result<()> {
    let first = element(&desktop.window, "Master level", AccessibleRole::Combobox)?;
    let second = element(&desktop.window, "Main job", AccessibleRole::Combobox)?;
    if first.size().height <= 0.0 || second.absolute_position().y < first.absolute_position().y + first.size().height {
        return Err("Input editors overlap or have no usable height".into());
    }
    Ok(())
}

fn assert_columns(desktop: &Desktop, labels: &[&str]) -> Result<Vec<Value>> {
    let mut right = f32::NEG_INFINITY;
    let mut first_y = None;
    let mut rectangles = Vec::new();
    for label in labels {
        let control = ElementHandle::find_by_accessible_label(&desktop.window, label).next()
            .ok_or_else(|| format!("Compact control {label:?} is not visible"))?;
        let position = control.absolute_position();
        let size = control.size();
        if size.width < 20.0 || size.height < 12.0 ||
            (size.height > 32.0 && control.accessible_role() != Some(AccessibleRole::List)) ||
            position.x < right - 1.0 || first_y.is_some_and(|y: f32| (position.y - y).abs() > 50.0) {
            return Err(format!("Compact columns collapsed, overlap, or clip at {label:?}: {position:?} {size:?}").into());
        }
        first_y.get_or_insert(position.y);
        right = position.x + size.width;
        rectangles.push(json!({"label": label, "x": position.x, "y": position.y, "width": size.width, "height": size.height}));
    }
    Ok(rectangles)
}

type HorizontalBaseline = BTreeMap<String, (f32, f32)>;

fn content_viewport(desktop: &Desktop) -> Result<ElementHandle> {
    let label = match desktop.window.get_active_tab() {
        0 => "Quicklook content", 1 => "Optimize content", 2 => "Simulate content", _ => "Stats content",
    };
    let content = element(&desktop.window, label, AccessibleRole::List)?;
    let viewport = content.query_descendants().match_inherits("Flickable").find_all().into_iter()
        .max_by(|left, right| left.size().width.total_cmp(&right.size().width))
        .ok_or_else(|| format!("{label} has no native scrolling viewport"))?;
    let clip_left = viewport.absolute_position().x;
    let clip_right = clip_left + viewport.size().width;
    let content_left = content.absolute_position().x;
    let gutter_left = content_left + content.size().width - 18.0;
    if (clip_left - content_left).abs() > 1.0 || clip_right > gutter_left + 1.0 {
        return Err(format!("{label}: actual visible clip is shifted or enters the reserved scrollbar lane: content {}, viewport {}, gutter left {gutter_left}",
            rectangle(&content), rectangle(&viewport)).into());
    }
    Ok(viewport)
}

fn rectangle(control: &ElementHandle) -> Value {
    let position = control.absolute_position();
    let size = control.size();
    json!({"x": position.x, "y": position.y, "width": size.width, "height": size.height,
        "right": position.x + size.width, "bottom": position.y + size.height})
}

#[derive(Clone, Copy)]
enum ScrollSurface { Standard, Report }

struct ScrollState {
    viewport: ElementHandle,
    bar: Option<ElementHandle>,
    thumb: Option<ElementHandle>,
    offset: f32,
    maximum: f32,
    lane_left: f32,
    geometry: Value,
}

fn scroll_state(content: &ElementHandle, label: &str, surface: ScrollSurface) -> Result<ScrollState> {
    let viewport = content.query_descendants().match_inherits("Flickable").find_all().into_iter()
        .max_by(|left, right| left.size().width.total_cmp(&right.size().width))
        .ok_or_else(|| format!("{label}: actual native Flickable is absent"))?;
    let bar = content.query_descendants().find_all().into_iter()
        .find(|control| control.accessible_label().is_some_and(|text| text == label));
    let content_right = content.absolute_position().x + content.size().width;
    let lane_right = content_right - if matches!(surface, ScrollSurface::Report) { 12.0 } else { 0.0 };
    let lane_left = lane_right - 18.0;
    let viewport_right = viewport.absolute_position().x + viewport.size().width;
    let clip_left = viewport.absolute_position().x;
    let expected_clip_left = content.absolute_position().x + if matches!(surface, ScrollSurface::Report) { 12.0 } else { 0.0 };
    let bar_left = bar.as_ref().map_or(lane_left, |bar| bar.absolute_position().x);
    if (clip_left - expected_clip_left).abs() > 1.0 ||
        (matches!(surface, ScrollSurface::Standard) && viewport_right > bar_left + 1.0) {
        return Err(format!("{label}: actual visible clip is misaligned or overlaps the bar: content {}, viewport {}, expected left {expected_clip_left}, bar left {bar_left}",
            rectangle(content), rectangle(&viewport)).into());
    }
    let wrapping_text = if matches!(surface, ScrollSurface::Report) {
        Some(content.query_descendants().match_id("TextEditBase::text-input").find_first()
            .ok_or("Report lost its actual native TextInput")?)
    } else { None };
    let usable_right = wrapping_text.as_ref().map_or(viewport_right, |text| text.absolute_position().x + text.size().width);
    if (usable_right - lane_left).abs() > 1.0 {
        return Err(format!("{label}: reserved18px lane does not meet the actual content right: usable={usable_right}, gutter={lane_left}, viewport={}", rectangle(&viewport)).into());
    }
    let (thumb, offset, maximum) = if let Some(bar) = bar.as_ref() {
        if bar.accessible_role() != Some(AccessibleRole::Slider) || (bar.size().width - 18.0).abs() > 1.0 ||
            (bar.absolute_position().x - lane_left).abs() > 1.0 ||
            (bar.size().height - viewport.size().height).abs() > 1.0 || bar.computed_opacity() < 0.999 {
            return Err(format!("{label}: native visible scrollbar is not an opaque18px reserved lane: {}", rectangle(bar)).into());
        }
        let offset = bar.accessible_value().ok_or_else(|| format!("{label}: scrollbar offset is absent"))?.parse::<f32>()?;
        let maximum = bar.accessible_value_maximum().ok_or_else(|| format!("{label}: scrollbar range is absent"))?;
        if !offset.is_finite() || !maximum.is_finite() || maximum <= 0.0 || offset < -1.0 || offset > maximum + 1.0 {
            return Err(format!("{label}: scrollbar offset/range is invalid: {offset}/{maximum}").into());
        }
        let thumb = bar.query_descendants().match_id("VerticalScrollBar::thumb").find_first()
            .ok_or_else(|| format!("{label}: overflowing view has no custom thumb"))?;
        let thumb_position = thumb.absolute_position();
        let thumb_size = thumb.size();
        let bar_position = bar.absolute_position();
        let minimum = 32.0_f32.min(bar.size().height);
        if (thumb_size.width - 14.0).abs() > 1.0 || thumb_size.height < minimum - 1.0 ||
            thumb_size.height > bar.size().height + 1.0 ||
            (thumb_position.x + thumb_size.width / 2.0 - bar_position.x - 9.0).abs() > 1.0 ||
            thumb_position.y < bar_position.y - 1.0 ||
            thumb_position.y + thumb_size.height > bar_position.y + bar.size().height + 1.0 {
            return Err(format!("{label}:14px thumb violates the32px/capped minimum or track bounds: {}", rectangle(&thumb)).into());
        }
        (Some(thumb), offset, maximum)
    } else {
        let covering_thumb = content.query_descendants().match_id("VerticalScrollBar::thumb").find_all().into_iter()
            .any(|thumb| thumb.absolute_position().x >= lane_left - 1.0 &&
                thumb.absolute_position().x + thumb.size().width <= lane_right + 1.0 && thumb.size().height > 0.0);
        if covering_thumb { return Err(format!("{label}: fitting/empty view retained a handle in its reserved lane").into()); }
        (None, 0.0, 0.0)
    };
    let geometry = json!({"label": label, "content": rectangle(content), "native_viewport": rectangle(&viewport),
        "wrapping_text": wrapping_text.as_ref().map(rectangle), "usable_content_right": usable_right,
        "reserved_lane": {"left": lane_left, "right": lane_right, "width": 18},
        "bar": bar.as_ref().map(rectangle), "thumb": thumb.as_ref().map(rectangle),
        "overflow": bar.is_some(), "offset": offset, "maximum": maximum,
        "required_thumb_width": 14, "required_minimum_length": 32.0_f32.min(viewport.size().height)});
    Ok(ScrollState { viewport, bar, thumb, offset, maximum, lane_left, geometry })
}

fn scroll_content_diagnostic(content: &ElementHandle, state: &ScrollState) -> (Vec<(SharedString, f32)>, Value) {
    let origin = state.viewport.absolute_position();
    let size = state.viewport.size();
    let mut texts = content.query_descendants().match_type_name("Text").find_all().into_iter()
        .filter_map(|text| {
            let position = text.absolute_position();
            let extent = text.size();
            if extent.height <= 0.0 || extent.width <= 0.0 ||
                position.y + extent.height <= origin.y || position.y >= origin.y + size.height ||
                position.x + extent.width <= origin.x || position.x >= origin.x + size.width {
                return None;
            }
            let label = text.accessible_label()?;
            if label.is_empty() { return None; }
            Some((position.y, label, rectangle(&text)))
        }).collect::<Vec<_>>();
    texts.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut anchors = Vec::with_capacity(texts.len());
    let visible_text = texts.into_iter().map(|(y, text, rectangle)| {
        let text_prefix = text.chars().take(160).collect::<String>();
        anchors.push((text, y));
        json!({"text_prefix": text_prefix, "rectangle": rectangle})
    }).collect::<Vec<_>>();
    let observed_offset = state.bar.as_ref().and_then(|bar| bar.accessible_value()).map(|value| value.to_string());
    let observed_maximum = state.bar.as_ref().and_then(|bar| bar.accessible_value_maximum());
    let diagnostic = json!({"offset": state.offset, "maximum": state.maximum, "estimated_extent": state.maximum + size.height,
        "observed_offset": observed_offset, "observed_maximum": observed_maximum,
        "observed_estimated_extent": observed_maximum.map(|maximum| maximum + size.height),
        "native_viewport": rectangle(&state.viewport), "wheel_target": [origin.x + 2.0, origin.y + size.height / 2.0],
        "visible_text": visible_text});
    (anchors, diagnostic)
}

fn unique_text_movements<'a>(before: &'a [(SharedString, f32)], after: &'a [(SharedString, f32)]) -> Vec<(&'a str, f32, f32)> {
    let mut identities = BTreeMap::<&str, (usize, usize, f32, f32)>::new();
    for (text, y) in before {
        let (count, _, position, _) = identities.entry(text.as_str()).or_default();
        *count += 1;
        *position = *y;
    }
    for (text, y) in after {
        let (_, count, _, position) = identities.entry(text.as_str()).or_default();
        *count += 1;
        *position = *y;
    }
    identities.into_iter().filter_map(|(text, (before_count, after_count, before_y, after_y))|
        (before_count == 1 && after_count == 1).then_some((text, before_y, after_y))).collect()
}

fn clear_scrollbar_lane(control: &ElementHandle, state: &ScrollState, label: &str) -> Result<Value> {
    if control.absolute_position().x + control.size().width > state.lane_left + 1.0 {
        return Err(format!("{label}: rightmost control enters the scrollbar lane: {}", rectangle(control)).into());
    }
    Ok(json!({"label": label, "control": rectangle(control), "gutter_left": state.lane_left}))
}

struct PointerClip { left: f32, top: f32, right: f32, bottom: f32 }

impl PointerClip {
    fn new(window: &slint::Window, outer: Option<&ElementHandle>) -> Self {
        let scale = window.scale_factor();
        let size = window.size();
        let mut clip = Self { left: 0.0, top: 0.0, right: size.width as f32 / scale, bottom: size.height as f32 / scale };
        if let Some(outer) = outer {
            let position = outer.absolute_position();
            clip.left = clip.left.max(position.x);
            clip.top = clip.top.max(position.y);
            clip.right = clip.right.min(position.x + outer.size().width);
            clip.bottom = clip.bottom.min(position.y + outer.size().height);
        }
        clip
    }

    fn check(&self, position: slint::LogicalPosition, label: &str) -> Result<()> {
        if !position.x.is_finite() || !position.y.is_finite() || position.x < self.left || position.x >= self.right ||
            position.y < self.top || position.y >= self.bottom {
            return Err(format!("{label}: native pointer target {position:?} is outside its actual visible clip {}", self.geometry()).into());
        }
        Ok(())
    }

    fn thumb_center(&self, thumb: &ElementHandle, label: &str) -> Result<slint::LogicalPosition> {
        let position = thumb.absolute_position();
        let size = thumb.size();
        let left = position.x.max(self.left);
        let right = (position.x + size.width).min(self.right);
        let top = position.y.max(self.top);
        let bottom = (position.y + size.height).min(self.bottom);
        if right <= left || bottom <= top {
            return Err(format!("{label}: custom thumb has no visible native hit area: thumb {}, clip {}", rectangle(thumb), self.geometry()).into());
        }
        let center = slint::LogicalPosition::new((left + right) / 2.0, (top + bottom) / 2.0);
        self.check(center, label)?;
        Ok(center)
    }

    fn geometry(&self) -> Value { json!({"left": self.left, "top": self.top, "right": self.right, "bottom": self.bottom}) }
}

async fn pointer_click(window: &slint::Window, position: slint::LogicalPosition) -> Result<()> {
    PointerClip::new(window, None).check(position, "native click")?;
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
    pause(20).await;
    window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
    pause(60).await;
    Ok(())
}

async fn pointer_drag(window: &slint::Window, from: slint::LogicalPosition, to: slint::LogicalPosition,
    content: &ElementHandle, label: &str, surface: ScrollSurface, clip: &PointerClip) -> Result<Vec<Value>> {
    clip.check(from, label)?;
    clip.check(to, label)?;
    window.dispatch_event(WindowEvent::PointerMoved { position: from });
    window.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
    pause(20).await;
    let mut samples = vec![json!({"event": "PointerPressed", "position": [from.x, from.y], "state": scroll_state(content, label, surface)?.geometry})];
    for step in 1..=8 {
        let fraction = step as f32 / 8.0;
        let position = slint::LogicalPosition::new(from.x + (to.x - from.x) * fraction, from.y + (to.y - from.y) * fraction);
        clip.check(position, label)?;
        window.dispatch_event(WindowEvent::PointerMoved { position });
        pause(20).await;
        samples.push(json!({"event": "PointerMoved", "position": [position.x, position.y],
            "state": scroll_state(content, label, surface)?.geometry}));
    }
    window.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
    pause(60).await;
    samples.push(json!({"event": "PointerReleased", "position": [to.x, to.y], "state": scroll_state(content, label, surface)?.geometry}));
    Ok(samples)
}

async fn native_wheel(window: &slint::Window, content: &ElementHandle, label: &str,
    surface: ScrollSurface, delta: f32) -> Result<ScrollState> {
    let state = scroll_state(content, label, surface)?;
    let origin = state.viewport.absolute_position();
    let position = slint::LogicalPosition::new(origin.x + 2.0, origin.y + state.viewport.size().height / 2.0);
    PointerClip::new(window, None).check(position, label)?;
    window.dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x: 0.0, delta_y: delta,
    });
    pause(80).await;
    scroll_state(content, label, surface)
}

async fn exercise_scrollbar(window: &slint::Window, content: &ElementHandle, label: &str,
    surface: ScrollSurface, outer: Option<&ElementHandle>) -> Result<Value> {
    let initial = scroll_state(content, label, surface)?;
    if initial.bar.is_none() { return Ok(json!({"initial": initial.geometry, "interaction": "fitting/empty: lane retained, no visible handle"})); }
    let clip = PointerClip::new(window, outer);
    let origin = initial.viewport.absolute_position();
    clip.check(slint::LogicalPosition::new(origin.x + 2.0, origin.y + initial.viewport.size().height / 2.0), label)?;
    let top = native_wheel(window, content, label, surface, 1_000_000.0).await?;
    if top.offset > 1.0 { return Err(format!("{label}: native wheel did not reach the top").into()); }
    let bar = top.bar.as_ref().ok_or("Overflow disappeared unexpectedly at the top")?;
    let thumb = top.thumb.as_ref().ok_or("Top thumb is absent")?;
    let travel = bar.size().height - thumb.size().height;
    if travel <= 1.0 {
        return Ok(json!({"initial": initial.geometry, "top": top.geometry, "interaction": "minimum thumb capped to available short track"}));
    }
    let page_position = slint::LogicalPosition::new(bar.absolute_position().x + 9.0,
        (thumb.absolute_position().y + thumb.size().height + 4.0).min(bar.absolute_position().y + bar.size().height - 1.0).min(clip.bottom - 1.0));
    clip.check(page_position, label)?;
    pointer_click(window, page_position).await?;
    let page = scroll_state(content, label, surface)?;
    if page.offset <= top.offset + 0.5 { return Err(format!("{label}: actual track page click did not advance content").into()); }
    let top_again = native_wheel(window, content, label, surface, 1_000_000.0).await?;
    let bar = top_again.bar.as_ref().ok_or("Track disappeared before native drag")?;
    let thumb = top_again.thumb.as_ref().ok_or("Thumb disappeared before native drag")?;
    let from = clip.thumb_center(thumb, label)?;
    let to = slint::LogicalPosition::new(from.x,
        (bar.absolute_position().y + bar.size().height - thumb.size().height / 2.0).clamp(clip.top + 0.5, clip.bottom - 0.5));
    let down = pointer_drag(window, from, to, content, label, surface, &clip).await?;
    let dragged = scroll_state(content, label, surface)?;
    if dragged.offset <= 0.5 { return Err(format!("{label}: native thumb down/move/up did not scroll content").into()); }
    let mut bottom = native_wheel(window, content, label, surface, -1_000_000.0).await?;
    // Visiting variable-height virtual delegates can refine the estimated range.
    for _ in 0..2 { bottom = native_wheel(window, content, label, surface, -1_000_000.0).await?; }
    if (bottom.offset - bottom.maximum).abs() > 1.0 { return Err(format!("{label}: native wheel did not reach the settled bottom").into()); }
    let wheel_delta = 120.0_f32.min(bottom.offset / 4.0).max(0.1);
    let (wheel_before_anchors, wheel_before_content) = scroll_content_diagnostic(content, &bottom);
    let wheel_up = native_wheel(window, content, label, surface, wheel_delta).await?;
    let (wheel_after_anchors, wheel_after_content) = scroll_content_diagnostic(content, &wheel_up);
    let minimum_movement = 0.1_f32.min(bottom.offset / 8.0);
    let movements = unique_text_movements(&wheel_before_anchors, &wheel_after_anchors);
    // Variable-list estimates can rebase offset/max together. Consumer-visible
    // wheel-up movement is DOWNWARD motion of the same uniquely identified text.
    // Every shared unique anchor must move correctly; duplicates cannot prove it.
    let moved_up = if movements.is_empty() {
        wheel_up.offset < bottom.offset - minimum_movement
    } else {
        movements.iter().all(|(_, before_y, after_y)| after_y - before_y > minimum_movement)
    };
    let wheel_up_direction = json!({"basis": if movements.is_empty() { "numeric offset decrease: no shared unique rendered Text" }
            else { "positive rendered y displacement of unique full Text identities" },
        "minimum_displacement": minimum_movement,
        "anchors": movements.iter().map(|(text, before_y, after_y)| json!({
            "text_prefix": text.chars().take(160).collect::<String>(), "before_y": before_y, "after_y": after_y,
            "delta_y": after_y - before_y})).collect::<Vec<_>>()});
    if !moved_up {
        return Err(format!("{label}: upward native wheel did not move content in the required direction: delta={wheel_delta}, proof={wheel_up_direction}, before={wheel_before_content}, after={wheel_after_content}").into());
    }
    let bar = wheel_up.bar.as_ref().ok_or("Track disappeared before upward drag")?;
    let thumb = wheel_up.thumb.as_ref().ok_or("Thumb disappeared before upward drag")?;
    let from = clip.thumb_center(thumb, label)?;
    let to = slint::LogicalPosition::new(from.x,
        (bar.absolute_position().y + thumb.size().height / 2.0).clamp(clip.top + 0.5, clip.bottom - 0.5));
    let up = pointer_drag(window, from, to, content, label, surface, &clip).await?;
    if scroll_state(content, label, surface)?.offset >= wheel_up.offset - 0.5 {
        return Err(format!("{label}: native upward thumb drag did not move content").into());
    }
    let final_top = native_wheel(window, content, label, surface, 1_000_000.0).await?;
    if final_top.offset > 1.0 { return Err(format!("{label}: final top boundary is not clamped").into()); }
    Ok(json!({"initial": initial.geometry, "pointer_clip": clip.geometry(), "top": top.geometry,
        "track_page_click": {"position": [page_position.x, page_position.y], "state": page.geometry},
        "thumb_down_pointer_events": down, "after_drag": dragged.geometry, "bottom": bottom.geometry,
        "wheel_up": wheel_up.geometry, "wheel_up_direction": wheel_up_direction,
        "wheel_up_content_before": wheel_before_content, "wheel_up_content_after": wheel_after_content,
        "thumb_up_pointer_events": up, "final_top": final_top.geometry}))
}

fn record_horizontal(desktop: &Desktop, key: &str, control: &ElementHandle,
    baseline: &mut HorizontalBaseline, rectangles: &mut Vec<Value>) -> Result<()> {
    let viewport = content_viewport(desktop)?;
    let position = control.absolute_position();
    let size = control.size();
    let left = viewport.absolute_position().x;
    let right = left + viewport.size().width;
    let client_right = desktop.window.window().size().width as f32 / desktop.window.window().scale_factor();
    if size.width <= 0.0 || size.height <= 0.0 || position.x < left - 1.0 ||
        position.x + size.width > right + 1.0 || right > client_right + 1.0 {
        return Err(format!("{key} clips the actual native viewport: control {}, viewport {}, client right {client_right}",
            rectangle(control), rectangle(&viewport)).into());
    }
    let reference = if let Some(&reference) = baseline.get(key) {
        reference
    } else {
        let reference = (position.x, size.width);
        baseline.insert(key.to_owned(), reference);
        reference
    };
    if (position.x - reference.0).abs() > 1.0 || (size.width - reference.1).abs() > 1.0 {
        return Err(format!("{key} changed horizontal geometry: baseline {reference:?}, actual {}", rectangle(control)).into());
    }
    rectangles.push(json!({"key": key, "label": control.accessible_label().map(|label| label.to_string()),
        "value": control.accessible_value().map(|value| value.to_string()), "rectangle": rectangle(control),
        "baseline": {"x": reference.0, "width": reference.1}, "viewport": rectangle(&viewport), "client_right": client_right}));
    Ok(())
}

fn fully_visible(desktop: &Desktop, control: &ElementHandle) -> Result<bool> {
    let viewport = content_viewport(desktop)?;
    Ok(control.absolute_position().y >= viewport.absolute_position().y - 1.0 &&
        control.absolute_position().y + control.size().height <= viewport.absolute_position().y + viewport.size().height + 1.0)
}

async fn geometry_element(desktop: &Desktop, label: &str, role: AccessibleRole) -> Result<ElementHandle> {
    if let Ok(control) = element(&desktop.window, label, role) {
        if fully_visible(desktop, &control)? { return Ok(control); }
    }
    scroll_content(desktop, 10000.0).await?;
    for _ in 0..24 {
        if let Ok(control) = element(&desktop.window, label, role) {
            if fully_visible(desktop, &control)? { return Ok(control); }
        }
        scroll_content(desktop, -160.0).await?;
    }
    Err(format!("{label:?} cannot be made fully visible through the native outer wheel path").into())
}

fn input_role(row: &InputRow) -> AccessibleRole {
    match row.control_kind {
        InputWidgetKind::Entry => AccessibleRole::TextInput,
        InputWidgetKind::Combo => AccessibleRole::Combobox,
        InputWidgetKind::Checkbox => AccessibleRole::Checkbox,
        InputWidgetKind::Label => AccessibleRole::Text,
    }
}

async fn stacked_input_elements(desktop: &Desktop, row: &InputRow, display_label: &str) -> Result<(ElementHandle, ElementHandle)> {
    for attempt in 0..26 {
        if let (Ok(field), Ok(text)) = (element(&desktop.window, row.label.as_str(), input_role(row)),
            element(&desktop.window, display_label, AccessibleRole::Text)) {
            if fully_visible(desktop, &field)? && fully_visible(desktop, &text)? { return Ok((field, text)); }
        }
        if attempt == 25 { break; }
        scroll_content(desktop, if attempt == 0 { 10000.0 } else { -160.0 }).await?;
    }
    Err(format!("{display_label:?} and its entry cannot be made fully visible together through the native outer wheel path").into())
}

async fn record_input(desktop: &Desktop, prefix: &str, row: &InputRow, with_label: bool,
    baseline: &mut HorizontalBaseline, rectangles: &mut Vec<Value>) -> Result<()> {
    let stacked_label = match (prefix, row.key.as_str()) {
        ("Quicklook", "defaults_enhancing_skill_entry") => Some(row.label.as_str()),
        ("Quicklook", "defaults_bubble_potency_entry") => Some("Bubble Potency:"),
        _ => None,
    };
    let (field, stacked_text) = if let Some(label) = stacked_label {
        let (field, text) = stacked_input_elements(desktop, row, label).await?;
        (field, Some(text))
    } else {
        (geometry_element(desktop, row.label.as_str(), input_role(row)).await?, None)
    };
    let key = format!("{prefix}/{}", row.label);
    record_horizontal(desktop, &format!("{key}/editor"), &field, baseline, rectangles)?;
    let editor = ElementHandle::find_by_element_type_name(&desktop.window, "ControlEditor")
        .find(|editor| editor.query_descendants().find_all().iter().any(|child|
            child.accessible_label().is_some_and(|label| label == row.label) && child.accessible_role() == Some(input_role(row))))
        .ok_or_else(|| format!("{} has no visible editor row", row.label))?;
    record_horizontal(desktop, &format!("{key}/row"), &editor, baseline, rectangles)?;
    if let Some(text) = stacked_text {
        let column = if row.key == "defaults_enhancing_skill_entry" {
            element(&desktop.window, &format!("White Magic — {} inputs", desktop.window.get_skill_label()), AccessibleRole::List)?
        } else {
            element(&desktop.window, "Geomancy Bubbles inputs", AccessibleRole::List)?
        };
        let column_viewport = column.query_descendants().match_inherits("Flickable").find_all().into_iter()
            .max_by(|left, right| left.size().width.total_cmp(&right.size().width))
            .ok_or_else(|| format!("{} has no native column viewport", row.label))?;
        let outer_viewport = content_viewport(desktop)?;
        let label_position = text.absolute_position();
        let label_size = text.size();
        let field_position = field.absolute_position();
        let field_size = field.size();
        let column_position = column_viewport.absolute_position();
        let column_size = column_viewport.size();
        let clip_top = column_position.y.max(outer_viewport.absolute_position().y);
        let clip_bottom = (column_position.y + column_size.height)
            .min(outer_viewport.absolute_position().y + outer_viewport.size().height);
        if label_size.width <= 0.0 || label_size.height <= 0.0 ||
            (label_position.x - column_position.x).abs() > 1.0 || (label_size.width - column_size.width).abs() > 1.0 ||
            (field_position.x - column_position.x).abs() > 1.0 || (field_size.width - column_size.width).abs() > 1.0 ||
            (editor.absolute_position().x - column_position.x).abs() > 1.0 || (editor.size().width - column_size.width).abs() > 1.0 ||
            (field_position.y - label_position.y - label_size.height).abs() > 1.0 ||
            label_position.y < clip_top - 1.0 || field_position.y + field_size.height > clip_bottom + 1.0 {
            return Err(format!("{} must have an unclipped, full-column-width label immediately above its full-width entry: label {}, entry {}, editor {}, column viewport {}, outer viewport {}",
                row.label, rectangle(&text), rectangle(&field), rectangle(&editor), rectangle(&column_viewport), rectangle(&outer_viewport)).into());
        }
        record_horizontal(desktop, &format!("{key}/label"), &text, baseline, rectangles)?;
        record_horizontal(desktop, &format!("{key}/column-viewport"), &column_viewport, baseline, rectangles)?;
        rectangles.push(json!({"key": key, "layout": "label-above-entry", "display_label": stacked_label,
            "label": rectangle(&text), "entry": rectangle(&field), "column_viewport": rectangle(&column_viewport),
            "visible_vertical_clip": {"top": clip_top, "bottom": clip_bottom}}));
    } else if with_label && row.control_kind != InputWidgetKind::Checkbox {
        let text = editor.query_descendants().match_type_name("Text").find_all().into_iter()
            .find(|text| {
                let position = text.absolute_position();
                position.x < field.absolute_position().x - 1.0 && text.size().width > 0.0 &&
                    position.x + text.size().width <= field.absolute_position().x + 1.0 &&
                    (position.y - field.absolute_position().y).abs() <= 3.0
            }).ok_or_else(|| format!("{} lost its visible, non-overlapping label", row.label))?;
        record_horizontal(desktop, &format!("{key}/label"), &text, baseline, rectangles)?;
    }
    Ok(())
}

async fn quick_input_geometry(desktop: &Desktop, baseline: &mut HorizontalBaseline, rectangles: &mut Vec<Value>) -> Result<()> {
    assert_input_spacing(desktop)?;
    assert_columns(desktop, &["Master level", "Special Toggles inputs", "Enemy preset"])?;
    let abilities = element(&desktop.window, "Special Toggles inputs", AccessibleRole::List)?;
    record_horizontal(desktop, "Quicklook/Special Toggles", &abilities, baseline, rectangles)?;
    for row in desktop.window.get_basic_controls().iter().filter(|row| row.visible) {
        record_input(desktop, "Quicklook", &row, true, baseline, rectangles).await?;
    }
    for row in desktop.window.get_enemy_controls().iter().filter(|row| row.visible) {
        let labelled = !matches!(row.label.as_str(), "Enemy preset" | "Enemy level / location");
        record_input(desktop, "Quicklook", &row, labelled, baseline, rectangles).await?;
    }
    let tp = element(&desktop.window, "TP", AccessibleRole::TextInput)?;
    let basics = element(&desktop.window, "Job, spell, weapon skill & TP inputs", AccessibleRole::List)?;
    if tp.absolute_position().y + tp.size().height > basics.absolute_position().y + basics.size().height + 1.0 {
        return Err("TP no longer fits its compact basic-input group".into());
    }
    let defense = element(&desktop.window, "enemy Defense", AccessibleRole::TextInput)?;
    let vit = element(&desktop.window, "enemy VIT", AccessibleRole::TextInput)?;
    let resistance = element(&desktop.window, "Resistance rank", AccessibleRole::Combobox)?;
    let enemy = element(&desktop.window, "Enemy Inputs inputs", AccessibleRole::List)?;
    if defense.size().width < 56.0 || vit.size().width < 48.0 || resistance.size().height < 20.0 ||
        resistance.absolute_position().y + resistance.size().height > enemy.absolute_position().y + enemy.size().height + 1.0 {
        return Err("Enemy values clip or the final enemy field needs inner scrolling".into());
    }
    quicklook_content_height_geometry(desktop, rectangles).await?;
    // The height proof wheels Active Buffs to the top, clipping Basic Inputs.
    // Restore the outer viewport before the next profile's spacing checks.
    let content = element(&desktop.window, "Quicklook content", AccessibleRole::List)?;
    let before = scroll_state(&content, "Quicklook content vertical scrollbar", ScrollSurface::Standard)?;
    let master_visible_before = element(&desktop.window, "Master level", AccessibleRole::Combobox).is_ok();
    scroll_content(desktop, 10000.0).await?;
    let after = scroll_state(&content, "Quicklook content vertical scrollbar", ScrollSurface::Standard)?;
    let master = element(&desktop.window, "Master level", AccessibleRole::Combobox)?;
    rectangles.push(json!({"key": "Quicklook/restore-first-row", "before": before.geometry,
        "master_level_visible_before": master_visible_before, "after": after.geometry,
        "master_level_after": rectangle(&master)}));
    Ok(())
}

async fn quicklook_content_height_geometry(desktop: &Desktop, rectangles: &mut Vec<Value>) -> Result<()> {
    let bottom = |element: &ElementHandle| element.absolute_position().y + element.size().height;
    let enemy = element(&desktop.window, "Enemy Inputs inputs", AccessibleRole::List)?;
    let toggles = element(&desktop.window, "Special Toggles inputs", AccessibleRole::List)?;
    let resistance = element(&desktop.window, "Resistance rank", AccessibleRole::Combobox)?;
    let enemy_group = element(&desktop.window, "Enemy Inputs", AccessibleRole::Groupbox)?;
    let buffs_group = element(&desktop.window, "Active Buffs", AccessibleRole::Groupbox)?;
    let enemy_bottom = bottom(&resistance);
    if (bottom(&enemy) - enemy_bottom).abs() > 1.0 || (bottom(&toggles) - enemy_bottom).abs() > 1.0 ||
        (buffs_group.absolute_position().y - bottom(&enemy_group)).abs() > 1.0 {
        return Err(format!("Quicklook first-row content must end with the last Enemy row and immediately precede Active Buffs: enemy {}, last row {}, toggles {}, enemy group {}, buffs group {}",
            rectangle(&enemy), rectangle(&resistance), rectangle(&toggles), rectangle(&enemy_group), rectangle(&buffs_group)).into());
    }
    let enemy_chrome = enemy_group.size().height - enemy.size().height;
    let enemy_bottom_padding = bottom(&enemy_group) - enemy_bottom;
    rectangles.push(json!({"key": "Quicklook/content-tight-first-row", "enemy": rectangle(&enemy),
        "last_enemy_row": rectangle(&resistance), "special_toggles": rectangle(&toggles),
        "enemy_group": rectangle(&enemy_group), "buff_group": rectangle(&buffs_group)}));
    let outer = content_viewport(desktop)?;
    scroll_content(desktop, outer.absolute_position().y - buffs_group.absolute_position().y).await?;
    let groups = [
        (format!("White Magic — {} inputs", desktop.window.get_skill_label()),
            vec![desktop.window.get_whm_controls(), desktop.window.get_food_controls()]),
        ("Bard Songs inputs".to_owned(), vec![desktop.window.get_brd_controls()]),
        ("Corsair Rolls inputs".to_owned(), vec![desktop.window.get_cor_controls()]),
        ("Geomancy Bubbles inputs".to_owned(), vec![desktop.window.get_geo_controls()]),
    ];
    let mut content_bottom = f32::NEG_INFINITY;
    let mut bounds: Option<(f32, f32)> = None;
    for (label, models) in groups {
        let viewport = element(&desktop.window, &label, AccessibleRole::List)?;
        let top = viewport.absolute_position().y;
        let end = bottom(&viewport);
        if let Some((expected_top, expected_bottom)) = bounds {
            if (top - expected_top).abs() > 1.0 || (end - expected_bottom).abs() > 1.0 {
                return Err(format!("{label}: buff columns no longer share their natural-height row").into());
            }
        }
        bounds = Some((top, end));
        for model in models {
            for row in model.iter().filter(|row| row.visible) {
                let control = element(&desktop.window, row.label.as_str(), input_role(&row))?;
                if !(20.0..=30.0).contains(&control.size().height) || control.absolute_position().y < top - 1.0 ||
                    bottom(&control) > end + 1.0 || !fully_visible(desktop, &control)? {
                    return Err(format!("{} must retain its normal, unclipped buff editor without inner scrolling: control {}, column {}",
                        row.label, rectangle(&control), rectangle(&viewport)).into());
                }
                content_bottom = content_bottom.max(bottom(&control));
            }
        }
        rectangles.push(json!({"key": format!("Quicklook/content-tight-buffs/{label}"), "viewport": rectangle(&viewport)}));
    }
    let (top, end) = bounds.ok_or("Quicklook buff columns are missing")?;
    let buff_chrome = buffs_group.size().height - (end - top);
    let buff_bottom_padding = bottom(&buffs_group) - content_bottom;
    if (end - content_bottom).abs() > 1.0 || (enemy_chrome - buff_chrome).abs() > 1.0 ||
        (enemy_bottom_padding - buff_bottom_padding).abs() > 1.0 {
        return Err(format!("Quicklook rows must contain only natural content plus matching GroupBox title/padding, without surplus height: buff bottom={end}, final content bottom={content_bottom}, enemy chrome={enemy_chrome}, buff chrome={buff_chrome}, enemy bottom padding={enemy_bottom_padding}, buff bottom padding={buff_bottom_padding}").into());
    }
    rectangles.push(json!({"key": "Quicklook/content-tight-row-overheads", "enemy_chrome": enemy_chrome,
        "buff_chrome": buff_chrome, "enemy_bottom_padding": enemy_bottom_padding, "buff_bottom_padding": buff_bottom_padding,
        "buff_content_bottom": content_bottom, "buff_group": rectangle(&buffs_group)}));
    Ok(())
}

fn centered_canvas(canvas: &ElementHandle, parent: &ElementHandle, label: &str) -> Result<()> {
    let position = canvas.absolute_position();
    let size = canvas.size();
    let origin = parent.absolute_position();
    let bounds = parent.size();
    if (position.x + size.width / 2.0 - origin.x - bounds.width / 2.0).abs() > 1.0 ||
        (position.y + size.height / 2.0 - origin.y - bounds.height / 2.0).abs() > 1.0 ||
        position.x < origin.x - 1.0 || position.y < origin.y - 1.0 ||
        position.x + size.width > origin.x + bounds.width + 1.0 || position.y + size.height > origin.y + bounds.height + 1.0 {
        return Err(format!("{label} is not centered inside its card/button: canvas {}, parent {}", rectangle(canvas), rectangle(parent)).into());
    }
    Ok(())
}

async fn missing_icon_workflow(desktop: &Desktop, missing: &MissingIcon, output: &Path,
    screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    let blank = desktop.window.get_quick_slots().row_data(Slot::Main.index()).ok_or("Missing-icon main row is absent")?;
    if blank.name != missing.name || blank.icon.to_rgba8().is_some() ||
        blank.icon.size().width != 0 || blank.icon.size().height != 0 {
        return Err("A requested missing pointed PNG did not yield a blank GearSlotRow image".into());
    }
    let label = format!("{}: {}", blank.label, blank.name);
    let button = geometry_element(desktop, &label, AccessibleRole::Button).await?;
    let card = ElementHandle::find_by_element_type_name(&desktop.window, "GearCard")
        .find(|card| card.query_descendants().match_inherits("Button").find_all().iter()
            .any(|child| child.accessible_label().is_some_and(|text| text == label)))
        .ok_or("Missing-icon gear card is absent")?;
    let canvas = card.query_descendants().match_id("GearCard::slot-icon").find_first()
        .ok_or("Missing-icon gear card lost its image canvas")?;
    if (card.size().width - 48.0).abs() > 1.0 || (card.size().height - 48.0).abs() > 1.0 ||
        (canvas.size().width - 28.0).abs() > 1.0 || (canvas.size().height - 28.0).abs() > 1.0 {
        return Err("A blank icon changed the fixed28px canvas/48px card geometry".into());
    }
    centered_canvas(&canvas, &card, "missing pointed icon")?;
    centered_canvas(&canvas, &button, "missing pointed icon button")?;
    let blank_geometry = json!({"card": rectangle(&card), "button": rectangle(&button), "canvas": rectangle(&canvas)});
    snapshot(&desktop.window, output, "00-missing-pointed-icon-blank", screenshots)?;
    fs::write(&missing.path, &missing.bytes)?;
    desktop.window.invoke_action("reload-data".into());
    pause(80).await;
    let restored = desktop.window.get_quick_slots().row_data(Slot::Main.index()).ok_or("Reloaded main row is absent")?;
    if restored.name != missing.name || restored.icon.to_rgba8().is_none() ||
        restored.icon.size().width == 0 || restored.icon.size().height == 0 {
        return Err("Normal Reload Data did not recover the restored pointed PNG".into());
    }
    snapshot(&desktop.window, output, "00-missing-pointed-icon-restored", screenshots)?;
    checks.push(json!({"workflow": "missing requested pointed icon and native reload", "item": missing.name,
        "removed_sandbox_asset": missing.path.file_name().and_then(|name| name.to_str()),
        "window_icon": "23937.png retained", "blank_image_size": [0, 0], "blank_geometry": blank_geometry,
        "restored_image_size": [restored.icon.size().width, restored.icon.size().height],
        "recovery": "restore original bytes, then invoke normal reload-data action"}));
    scroll_content(desktop, 10000.0).await?;
    Ok(())
}

async fn panel_element(desktop: &Desktop, panel: &ElementHandle, label: &str, role: AccessibleRole) -> Result<ElementHandle> {
    for attempt in 0..25 {
        if let Some(control) = panel.query_descendants().find_all().into_iter()
            .find(|control| control.accessible_label().is_some_and(|text| text == label) && control.accessible_role() == Some(role)) {
            if role == AccessibleRole::List || fully_visible(desktop, &control)? { return Ok(control); }
        }
        scroll_content(desktop, if attempt == 0 { 10000.0 } else { -160.0 }).await?;
    }
    Err(format!("Panel control {label:?} is unreachable through the native outer wheel").into())
}

async fn record_gear_geometry(desktop: &Desktop, source: &str, marker: &str, slots: &slint::ModelRc<GearSlotRow>, label_only: bool,
    baseline: &mut HorizontalBaseline, rectangles: &mut Vec<Value>) -> Result<()> {
    visible_element(desktop, marker, AccessibleRole::Button).await?;
    let panel = ElementHandle::find_by_element_type_name(&desktop.window, "GearPanel")
        .find(|panel| panel.query_descendants().find_all().iter()
            .any(|child| child.accessible_label().is_some_and(|label| label == marker)))
        .ok_or_else(|| format!("{source} gear panel is not visible"))?;
    record_horizontal(desktop, &format!("{source}/panel"), &panel, baseline, rectangles)?;
    let normal = slots.iter().find(|slot| label_only || slot.name != "Empty").ok_or("No occupied gear card to measure")?;
    let empty = if label_only { None } else { Some(slots.iter().find(|slot| slot.name == "Empty").ok_or("No empty gear card to measure")?) };
    let modes = [(if label_only { "label-only" } else { "normal" }, Some(&normal)), ("empty", empty.as_ref())];
    let mut equipped = None;
    for (mode, slot) in modes {
        let Some(slot) = slot else { continue; };
        let label = format!("{}: {}", slot.label, slot.name);
        let button = panel_element(desktop, &panel, &label, AccessibleRole::Button).await?;
        let card = panel.query_descendants().match_inherits("GearCard").find_all().into_iter()
            .find(|card| card.query_descendants().match_inherits("Button").find_all().iter()
                .any(|child| child.accessible_label().is_some_and(|text| text == label)))
            .ok_or_else(|| format!("{source}/{mode} has no debug GearCard"))?;
        if (card.size().width - 48.0).abs() > 1.0 || (card.size().height - 48.0).abs() > 1.0 {
            return Err(format!("{source}/{mode} card is not a fixed 48px square: {}", rectangle(&card)).into());
        }
        let images = card.query_descendants().match_type_name("Image").find_all();
        let canvas = if label_only {
            if !images.is_empty() { return Err("Label-only cards must not retain an invisible image layout row".into()); }
            card.query_descendants().match_id("GearCard::slot-label").find_first()
                .ok_or("Label-only card lost its visible slot-label canvas")?
        } else {
            if images.len() != 1 { return Err(format!("{source}/{mode} must expose exactly one rendered Image canvas, got {}", images.len()).into()); }
            let image = images[0].clone();
            if (image.size().width - 28.0).abs() > 1.0 || (image.size().height - 28.0).abs() > 1.0 {
                return Err(format!("{source}/{mode} image canvas changed size: {}", rectangle(&image)).into());
            }
            image
        };
        centered_canvas(&canvas, &card, &format!("{source}/{mode} canvas"))?;
        centered_canvas(&canvas, &button, &format!("{source}/{mode} canvas"))?;
        for (part, control) in [("card", &card), ("button", &button), ("canvas", &canvas)] {
            record_horizontal(desktop, &format!("{source}/{mode}/{part}"), control, baseline, rectangles)?;
        }
        rectangles.push(json!({"gear_mode": mode, "source": source, "slot": slot.label.to_string(),
            "canvas_type": canvas.type_name().map(|name| name.to_string()), "center_tolerance": 1.0,
            "card": rectangle(&card), "button": rectangle(&button), "canvas": rectangle(&canvas)}));
        if equipped.is_none() { equipped = Some(button); }
    }
    let choices_label = if label_only { "Gear candidates" } else { "Available equipment" };
    let choices = panel_element(desktop, &panel, choices_label, AccessibleRole::List).await?;
    let equipped = equipped.ok_or("No equipped gear card was measured")?;
    if choices.absolute_position().x <= equipped.absolute_position().x + equipped.size().width {
        return Err(format!("{source} candidates moved below the slot grid").into());
    }
    record_horizontal(desktop, &format!("{source}/candidate list"), &choices, baseline, rectangles)?;
    let row_type = if label_only { "GearCheckRow" } else { "GearRadioRow" };
    let candidate = choices.query_descendants().match_inherits(row_type).find_first()
        .ok_or_else(|| format!("{source} candidate list has no rendered row"))?;
    record_horizontal(desktop, &format!("{source}/candidate row"), &candidate, baseline, rectangles)?;
    if candidate.absolute_position().x < choices.absolute_position().x - 1.0 ||
        candidate.absolute_position().x + candidate.size().width > choices.absolute_position().x + choices.size().width + 1.0 {
        return Err(format!("{source} candidate row clips its list viewport").into());
    }
    for _ in 0..24 {
        if let Some(inspector) = panel.query_descendants().match_inherits("GroupBox").find_all().into_iter()
            .find(|group| group.absolute_position().x >= choices.absolute_position().x - 1.0 &&
                group.absolute_position().y >= choices.absolute_position().y + choices.size().height - 1.0) {
            if fully_visible(desktop, &inspector)? {
                record_horizontal(desktop, &format!("{source}/item details"), &inspector, baseline, rectangles)?;
                return Ok(());
            }
        }
        scroll_content(desktop, -160.0).await?;
    }
    Err(format!("{source} item details are unreachable through the native outer wheel").into())
}

async fn record_stats_geometry(desktop: &Desktop, baseline: &mut HorizontalBaseline, rectangles: &mut Vec<Value>) -> Result<()> {
    let groups = [desktop.window.get_stats_base(), desktop.window.get_stats_physical(), desktop.window.get_stats_magical(),
        desktop.window.get_stats_multi(), desktop.window.get_stats_haste(), desktop.window.get_stats_defensive(), desktop.window.get_stats_other()];
    let mut boxes = Vec::new();
    for (index, rows) in groups.into_iter().enumerate() {
        let first = rows.row_data(0).ok_or("Stats group has no first row")?;
        let longest = rows.iter().max_by_key(|row| row.label.len()).ok_or("Stats group has no labels")?;
        let second = (longest.label != first.label).then_some(longest);
        for stat in [Some(first), second].into_iter().flatten() {
            let row_label = format!("{}: {}", stat.label, stat.value);
            let row = geometry_element(desktop, &row_label, AccessibleRole::ListItem).await?;
            let value = row.query_descendants().match_type_name("Text").find_all().into_iter()
                .find(|text| text.accessible_label().is_some_and(|label| label == stat.label) &&
                    text.accessible_value().is_some_and(|value| value == stat.value)).ok_or("Stats value is not displayed")?;
            let label = row.query_descendants().match_type_name("Text").find_all().into_iter()
                .find(|text| text.accessible_label().is_some_and(|label| label == stat.label) && text.accessible_value().is_none())
                .ok_or("Stats label is not displayed")?;
            if value.size().width < (if index == 0 { 48.0 } else { 64.0 }) ||
                label.size().width < 20.0 || label.absolute_position().x + label.size().width + 5.0 > value.absolute_position().x {
                return Err(format!("{} label/value overlap or lack readable value space", stat.label).into());
            }
            for (part, control) in [("row", &row), ("label", &label), ("value", &value)] {
                record_horizontal(desktop, &format!("Stats/{}/{part}", stat.label), control, baseline, rectangles)?;
            }
            if boxes.len() == index {
                let group = ElementHandle::find_by_element_type_name(&desktop.window, "StatGroup")
                    .find(|group| group.query_descendants().find_all().iter()
                        .any(|child| child.accessible_label().is_some_and(|label| label == row_label)))
                    .ok_or("Stats row has no visible group")?;
                record_horizontal(desktop, &format!("Stats/group/{index}"), &group, baseline, rectangles)?;
                boxes.push(group);
            }
        }
    }
    for (upper, lower) in [(&boxes[0..3], &boxes[3..5]), (&boxes[3..5], &boxes[5..7])] {
        let bottom = upper.iter().map(|group| group.absolute_position().y + group.size().height).fold(f32::NEG_INFINITY, f32::max);
        if lower.iter().any(|group| group.absolute_position().y < bottom - 1.0) {
            return Err("Stats group rows overlap after displaying their labels/values".into());
        }
    }
    Ok(())
}

async fn compact_layout(desktop: &Desktop, output: &Path, ninja: &FixtureValue, screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    fs::write(&desktop.controller.borrow().paths.defaults, serde_json::to_vec(&ninja["input"]["profile_document"])?)?;
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    pause(80).await;
    tab(desktop, 0).await?;
    scroll_content(desktop, 10000.0).await?;
    assert_input_spacing(desktop)?;
    let inputs = assert_columns(desktop, &["Master level", "Special Toggles inputs", "Enemy preset"])?;
    snapshot(&desktop.window, output, "11-compact-760x440-inputs", screenshots)?;
    let buff_group = element(&desktop.window, "Active Buffs", AccessibleRole::Groupbox)?;
    let outer = content_viewport(desktop)?;
    scroll_content(desktop, outer.absolute_position().y - buff_group.absolute_position().y).await?;
    let buffs = assert_columns(desktop, &["White Magic", "Bard Songs", "Corsair Rolls", "Geomancy Bubbles"])?;
    snapshot(&desktop.window, output, "11-compact-760x440-buffs", screenshots)?;
    let slot = desktop.window.get_quick_slots().row_data(0).ok_or("Main gear slot missing")?;
    let slot_label = format!("{}: {}", slot.label, slot.name);
    let equipped = visible_element(desktop, &slot_label, AccessibleRole::Button).await?;
    let choices = element(&desktop.window, "Available equipment", AccessibleRole::List)?;
    if choices.absolute_position().x <= equipped.absolute_position().x + equipped.size().width {
        return Err("Compact equipment choices moved below the slot grid".into());
    }
    snapshot(&desktop.window, output, "11-compact-760x440-equipment", screenshots)?;
    let previous = desktop.controller.borrow().completed_jobs;
    click(desktop, "Quicklook WS").await?;
    finished(desktop, previous).await?;
    assert_display(desktop, &ninja["expected"]["value"]["steps"][5]["state"]["displayed_analytical"])?;
    snapshot(&desktop.window, output, "11-compact-760x440-calculation", screenshots)?;
    let previous = desktop.controller.borrow().completed_jobs;
    click(desktop, "Quicklook TP").await?;
    finished(desktop, previous).await?;
    assert_display(desktop, &ninja["expected"]["value"]["steps"][4]["state"]["displayed_analytical"])?;
    if desktop.window.get_time_to_ws().is_empty() { return Err("Completed Quicklook TP omitted its displayed time".into()); }

    tab(desktop, 1).await?;
    scroll_content(desktop, 10000.0).await?;
    let slot = desktop.window.get_optimize_slots().row_data(0).ok_or("Optimize main slot missing")?;
    let equipped = element(&desktop.window, &format!("{}: {}", slot.label, slot.name), AccessibleRole::Button)?;
    let candidates = element(&desktop.window, "Gear candidates", AccessibleRole::List)?;
    if candidates.absolute_position().x <= equipped.absolute_position().x + equipped.size().width {
        return Err("Compact optimizer candidates moved below the slot grid".into());
    }
    scroll_content(desktop, -10000.0).await?;
    visible_element(desktop, "Optimize WS", AccessibleRole::Button).await?;
    snapshot(&desktop.window, output, "11-compact-760x440-optimize-options", screenshots)?;

    tab(desktop, 2).await?;
    scroll_content(desktop, 10000.0).await?;
    let slot = desktop.window.get_tp_slots().row_data(0).ok_or("TP main slot missing")?;
    let equipped = element(&desktop.window, &format!("{}: {}", slot.label, slot.name), AccessibleRole::Button)?;
    let choices = element(&desktop.window, "Available equipment", AccessibleRole::List)?;
    if choices.absolute_position().x <= equipped.absolute_position().x + equipped.size().width {
        return Err("Compact TP choices moved below the slot grid".into());
    }
    scroll_content(desktop, -10000.0).await?;
    visible_element(desktop, "Run DPS simulations", AccessibleRole::Button).await?;
    visible_element(desktop, "Create WS damage distribution plot", AccessibleRole::Button).await?;
    visible_element(desktop, "Compare TP & WS stats", AccessibleRole::Button).await?;
    snapshot(&desktop.window, output, "11-compact-760x440-simulate-actions", screenshots)?;

    tab(desktop, 3).await?;
    scroll_content(desktop, 10000.0).await?;
    let stats = assert_columns(desktop, &["STR", "Accuracy1", "Magic Accuracy"])?;
    snapshot(&desktop.window, output, "11-compact-760x440-stats-columns", screenshots)?;
    scroll_content(desktop, -10000.0).await?;
    snapshot(&desktop.window, output, "11-compact-760x440-stats-bottom", screenshots)?;
    checks.push(json!({"workflow": "760x440 compact reference layout", "input_columns": inputs,
        "buff_columns": buffs, "stats_columns": stats,
        "gear": "slot grid beside virtual candidate lists", "calculation": "native Quicklook WS and TP displays equal frozen Ninja results",
        "scrolling": "outer pointer wheel reaches calculation, optimization and simulation actions"}));
    Ok(())
}

async fn resize_geometry(desktop: &Desktop, output: &Path, width: f32, height: f32, baseline: &mut HorizontalBaseline,
    screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    let mut rectangles = Vec::new();
    let prefix = format!("11-fixed-{}x{}", width as u32, height as u32);
    tab(desktop, 0).await?;
    scroll_content(desktop, 10000.0).await?;
    record_horizontal(desktop, "Quicklook/viewport", &content_viewport(desktop)?, baseline, &mut rectangles)?;
    quick_input_geometry(desktop, baseline, &mut rectangles).await?;
    for model in [desktop.window.get_whm_controls(), desktop.window.get_food_controls(), desktop.window.get_brd_controls(),
        desktop.window.get_cor_controls(), desktop.window.get_geo_controls()] {
        for row in model.iter().filter(|row| row.visible && (row.control_kind == InputWidgetKind::Entry || matches!(row.label.as_str(),
            "White Magic" | "Dia" | "Food" | "Bard Songs" | "Songs +" | "Corsair Rolls" | "Roll 1 potency" | "Roll 1 name" | "Geomancy Bubbles" | "Indi-"))) {
            record_input(desktop, "Quicklook", &row, row.control_kind == InputWidgetKind::Entry, baseline, &mut rectangles).await?;
        }
    }
    snapshot(&desktop.window, output, &format!("{prefix}-buffs"), screenshots)?;
    record_gear_geometry(desktop, "Quicklook/gear", "Copy to TP Set", &desktop.window.get_quick_slots(),
        false, baseline, &mut rectangles).await?;
    snapshot(&desktop.window, output, &format!("{prefix}-quicklook-equipment"), screenshots)?;
    for label in ["Copy to TP Set", "Copy to WS Set", "Copy Quicklook to Clipboard", "Quicklook TP", "Quicklook WS", "Quicklook Spell", "Simulate WS"] {
        let button = geometry_element(desktop, label, AccessibleRole::Button).await?;
        record_horizontal(desktop, &format!("Quicklook/action/{label}"), &button, baseline, &mut rectangles)?;
    }
    let mut previous: Option<ElementHandle> = None;
    for (label, expected) in [("Average damage", desktop.window.get_damage()), ("Average TP", desktop.window.get_tp_return()),
        ("Time per WS", desktop.window.get_time_to_ws())] {
        let field = geometry_element(desktop, label, AccessibleRole::Text).await?;
        if field.accessible_value() != Some(expected) {
            return Err(format!("Completed Quicklook result {label:?} is not the displayed value").into());
        }
        if let Some(previous) = previous.as_ref() {
            if field.absolute_position().y < previous.absolute_position().y + previous.size().height - 1.0 {
                return Err("Completed Quicklook TP result/time fields overlap".into());
            }
        }
        record_horizontal(desktop, &format!("Quicklook/result/{label}"), &field, baseline, &mut rectangles)?;
        previous = Some(field);
    }
    snapshot(&desktop.window, output, &format!("{prefix}-quicklook-results"), screenshots)?;

    tab(desktop, 1).await?;
    scroll_content(desktop, 10000.0).await?;
    record_horizontal(desktop, "Optimize/viewport", &content_viewport(desktop)?, baseline, &mut rectangles)?;
    record_gear_geometry(desktop, "Optimize/gear", "Select all candidates in current slot",
        &desktop.window.get_optimize_slots(), true, baseline, &mut rectangles).await?;
    snapshot(&desktop.window, output, &format!("{prefix}-optimize-equipment"), screenshots)?;
    for row in desktop.window.get_optimize_controls().iter().filter(|row| row.visible) {
        record_input(desktop, "Optimize", &row, true, baseline, &mut rectangles).await?;
    }
    for label in ["Select all candidates in current slot", "Unselect all candidates in current slot", "Select all candidates in all slots",
        "Select candidates from Windower inventory file", "Optimize WS", "Optimize Spell", "Optimize TP", "Equip best set"] {
        let button = geometry_element(desktop, label, AccessibleRole::Button).await?;
        record_horizontal(desktop, &format!("Optimize/action/{label}"), &button, baseline, &mut rectangles)?;
    }
    snapshot(&desktop.window, output, &format!("{prefix}-optimize-actions"), screenshots)?;

    tab(desktop, 2).await?;
    scroll_content(desktop, 10000.0).await?;
    record_horizontal(desktop, "Simulate/viewport", &content_viewport(desktop)?, baseline, &mut rectangles)?;
    for (source, marker, slots) in [("Simulate/TP gear", "Copy TP set to Quicklook", desktop.window.get_tp_slots()),
        ("Simulate/WS gear", "Copy WS set to Quicklook", desktop.window.get_ws_slots())] {
        record_gear_geometry(desktop, source, marker, &slots, false, baseline, &mut rectangles).await?;
        snapshot(&desktop.window, output, &format!("{prefix}-{}", if source.contains("TP") { "tp-equipment" } else { "ws-equipment" }), screenshots)?;
    }
    for row in desktop.window.get_simulate_controls().iter().filter(|row| row.visible) {
        record_input(desktop, "Simulate", &row, true, baseline, &mut rectangles).await?;
    }
    for label in ["Copy TP set to Quicklook", "Copy TP set to clipboard", "Copy WS set to Quicklook", "Copy WS set to clipboard",
        "Run DPS simulations", "Create WS damage distribution plot", "Compare TP & WS stats"] {
        let button = geometry_element(desktop, label, AccessibleRole::Button).await?;
        record_horizontal(desktop, &format!("Simulate/action/{label}"), &button, baseline, &mut rectangles)?;
    }
    snapshot(&desktop.window, output, &format!("{prefix}-simulate-actions"), screenshots)?;

    tab(desktop, 3).await?;
    scroll_content(desktop, 10000.0).await?;
    record_horizontal(desktop, "Stats/viewport", &content_viewport(desktop)?, baseline, &mut rectangles)?;
    for label in ["Quicklook Gear Stats", "TP Gear Stats", "WS Gear Stats"] {
        let button = geometry_element(desktop, label, AccessibleRole::Button).await?;
        record_horizontal(desktop, &format!("Stats/action/{label}"), &button, baseline, &mut rectangles)?;
    }
    record_stats_geometry(desktop, baseline, &mut rectangles).await?;
    snapshot(&desktop.window, output, &format!("{prefix}-stats-lower-groups"), screenshots)?;
    checks.push(json!({"workflow": "fixed horizontal geometry under native resize", "requested_logical_size": [width, height],
        "actual_client_pixels": [desktop.window.window().size().width, desktop.window.window().size().height],
        "scale_factor": desktop.window.window().scale_factor(), "baseline_logical_size": [760, 440], "geometry": rectangles,
        "scrolling": "Ctrl+1..4 and outer pointer wheel reach all tab controls, TP/WS cards/candidates and lower actions/results",
        "icons": "debug Image canvas centers, not asset silhouettes; normal, empty and label-only cards"}));
    Ok(())
}

fn assert_stats(actual: &Stats, expected: &FixtureValue, label: &str) -> Result<()> {
    let expected = expected.as_object().ok_or("Frozen stat sheet is missing")?;
    if actual.len() != expected.len() { return Err(format!("{label}: stat presence differs").into()); }
    for ((stat, actual), (name, expected)) in actual.ordered().zip(expected) {
        let expected = expected.as_f64().ok_or("Frozen buff number is invalid")?;
        let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
        if stat.name() != name || !actual.is_finite() || (actual - expected).abs() > tolerance {
            return Err(format!("{label}/{name}: expected {expected}, actual {} {actual}", stat.name()).into());
        }
    }
    Ok(())
}

fn apply_buff_step(desktop: &Desktop, step: &FixtureValue) -> Result<()> {
    let key = step["key"].as_str().ok_or("Buff key is missing")?;
    match step["action"].as_str().ok_or("Buff action is missing")? {
        "activate" => desktop.window.invoke_text_edited(key.into(), step["value"].as_str().ok_or("Buff combo value is missing")?.into()),
        "check" => desktop.window.invoke_toggled(key.into(), step["value"].as_bool().ok_or("Buff checkbox value is missing")?),
        "restore_widget" => {
            // The oracle's programmatic widget restore does not activate caches.
            let mut controller = desktop.controller.borrow_mut();
            controller.ui.set_raw(key, &serde_json::to_value(&step["value"])?)?;
            let tp = controller.ui.controls["defaults_tp_entry"].value.clone();
            controller.edit("defaults_tp_entry", tp)?;
        },
        action => return Err(format!("Unsupported captured buff action {action}").into()),
    }
    Ok(())
}

async fn buff_workflows(desktop: &Desktop, output: &Path, fixtures: &Path, index: &FixtureValue,
    screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    for (id, name) in [
        ("ui.buff.song.00.honor-march", "brd"), ("ui.buff.roll.00.chaos", "cor"),
        ("ui.buff.bubble.00.fury", "geo"), ("ui.buff.white-magic.dia", "dia"),
        ("ui.buff.white-magic.haste", "haste"), ("ui.buff.white-magic.boost", "boost"),
        ("ui.buff.white-magic.storm", "storm"), ("ui.buff.white-magic.gain-and-skill-reset", "gain"),
        ("ui.buff.shell-independent", "shell"), ("ui.buff.food.00.grape-daifuku--1", "food"),
    ] {
        let mut paths = desktop.controller.borrow().paths.clone();
        paths.defaults = paths.root.join("absent-buff-profile.json");
        let buff = Desktop::with_random(load_resources_at(paths)?, RandomSources::from_seed(11))?;
        buff.window.show()?;
        let document = fixture(fixtures, index, id)?;
        for step in document["input"]["setup"].as_array().ok_or("Buff setup is missing")? {
            apply_buff_step(&buff, step)?;
        }
        let steps = document["input"]["steps"].as_array().ok_or("Buff steps are missing")?;
        let rows = document["expected"]["value"]["steps"].as_array().ok_or("Buff reference is missing")?;
        let mut captured = false;
        for (step, row) in steps.iter().zip(rows) {
            apply_buff_step(&buff, step)?;
            let controller = buff.controller.borrow();
            let actual = aggregate_buffs(&controller.ui.buff_inputs(), &controller.ui.engine.catalog);
            match actual {
                Ok(effects) => {
                    let expected = &row["state"]["aggregate_buffs"];
                    if effects.buckets.len() != expected["active_buffs"].as_object().ok_or("Frozen buffs missing")?.len() {
                        return Err(format!("{id}: bucket domain differs").into());
                    }
                    for bucket in &effects.buckets {
                        assert_stats(&bucket.stats, &expected["active_buffs"][bucket.name.as_ref()], id)?;
                    }
                    assert_stats(&effects.enemy_debuffs, &expected["combined_debuffs"], id)?;
                    if let Some(enemy) = row["state"].get("normalized_enemy") {
                        let input = controller.ui.calculation_inputs(GearSource::Quicklook)?;
                        assert_stats(&input.enemy.stats, enemy, id)?;
                    }
                },
                Err(CalcError::Legacy { category, .. }) if row["state"]["aggregate_buffs_error"]["class"] == category => {},
                Err(error) => return Err(format!("{id}: unexpected buff error: {error}").into()),
            }
            if let Some(gathered) = row["state"].get("gather_player_inputs") {
                let abilities = controller.ui.abilities()?;
                if abilities.storm_name.as_deref() != gathered["special_toggles"]["Storm spell"].as_str() {
                    return Err(format!("{id}: effective storm cache differs").into());
                }
            }
            drop(controller);
            if !captured && row["state"]["aggregate_buffs"].is_object() {
                pause(20).await;
                snapshot(&buff.window, output, &format!("00-buff-{name}"), screenshots)?;
                captured = true;
            }
        }
        buff.controller.borrow_mut().shutdown();
        checks.push(json!({"workflow": "effective buff controls", "reference": id, "transitions": steps.len(),
            "checks": "ordered buff buckets/debuffs and normalized enemy"}));
    }
    Ok(())
}

async fn generated_variants(desktop: &Desktop, output: &Path, screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    for (job, slot, name, str_text) in [
        ("Ninja", Slot::Back, "Andartia's Mantle STR Weapon Skill Damage", "STR:30"),
        ("Bard", Slot::Ranged, "Linos STR Weapon Skill Damage", "STR:8"),
    ] {
        desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), job.into());
        pause(50).await;
        let viewport = element(&desktop.window, "Quicklook content", AccessibleRole::List)?;
        let position = viewport.absolute_position();
        for _ in 0..4 {
            desktop.window.window().dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(position.x + 2.0, position.y + viewport.size().height * 0.5),
                delta_x: 0.0, delta_y: -700.0,
            });
            pause(25).await;
        }
        let previous = desktop.controller.borrow().ui.gear.equipped;
        let old_name = desktop.controller.borrow().ui.engine.catalog.piece(previous[0][slot.index()]).name2.clone();
        click(desktop, &format!("{}: {old_name}", slot.name())).await?;
        let candidates = element(&desktop.window, "Available equipment", AccessibleRole::List)?;
        let mut choice = element(&desktop.window, name, AccessibleRole::RadioButton);
        for _ in 0..40 {
            if choice.is_ok() { break; }
            candidates.scroll(0.0, -240.0);
            pause(25).await;
            choice = element(&desktop.window, name, AccessibleRole::RadioButton);
        }
        let choice = choice?;
        let tooltip = choice.accessible_description().ok_or("Generated variant has no tooltip")?;
        if !tooltip.contains(str_text) { return Err(format!("{name}: tooltip lost variant stat {str_text}").into()); }
        choice.invoke_accessible_default_action();
        pause(40).await;
        let state = desktop.controller.borrow();
        let actual = state.ui.gear.equipped;
        if state.ui.engine.catalog.piece(actual[0][slot.index()]).name2.as_ref() != name ||
            actual[1..] != previous[1..] {
            return Err(format!("{name}: native variant selection crossed gear sources").into());
        }
        drop(state);
        snapshot(&desktop.window, output, &format!("01-generated-{}", slot.name()), screenshots)?;
        checks.push(json!({"workflow": "generated variant and tooltip", "job": job, "slot": slot.name(), "variant": name, "tooltip_stat": str_text}));
    }
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    let viewport = element(&desktop.window, "Quicklook content", AccessibleRole::List)?;
    let position = viewport.absolute_position();
    desktop.window.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(position.x + 2.0, position.y + viewport.size().height * 0.5),
        delta_x: 0.0, delta_y: 10000.0,
    });
    pause(60).await;
    Ok(())
}

fn report_clipboard_sentinel(clipboard: &mut arboard::Clipboard, stage: &str) -> Result<()> {
    let sentinel = "native-scrollbar-selection-sentinel";
    clipboard.set_text(sentinel)?;
    // The X11 setter only flushes. This read-back acknowledges ownership on
    // arboard's connection before Slint's separate clipboard client copies.
    let established = clipboard.get_text().map_err(|error| format!(
        "Native report clipboard sentinel read-back failed {stage}: {error}"))?;
    if established != sentinel {
        return Err(format!("Native report clipboard sentinel ownership was not established {stage}: copied_prefix={:?}",
            established.chars().take(160).collect::<String>()).into());
    }
    Ok(())
}

async fn report_selection(report: &ResultWindow) -> Result<Value> {
    let content = ElementHandle::find_by_element_type_name(report, "ScrollableTextEdit").next()
        .ok_or("Wrapped native report editor is absent")?;
    let text = content.query_descendants().match_id("TextEditBase::text-input").find_first()
        .ok_or("Native report TextInput is absent")?;
    let state = scroll_state(&content, "Computation result vertical scrollbar", ScrollSurface::Report)?;
    let protected = clear_scrollbar_lane(&text, &state, "report rendered TextInput")?;
    let expected = report.get_report_text();
    let accessible = element(report, "Computation result", AccessibleRole::TextInput)?;
    if accessible.accessible_value() != Some(expected.clone()) { return Err("Native report lost its accessible text value".into()); }
    let position = state.viewport.absolute_position();
    pointer_click(report.window(), slint::LogicalPosition::new(position.x + 20.0, position.y + 12.0)).await?;
    control_key(report.window(), "a");
    pause(40).await;
    let mut clipboard = arboard::Clipboard::new()?;
    report_clipboard_sentinel(&mut clipboard, "before initial selection/copy baseline")?;
    control_key(report.window(), "c");
    pause(80).await;
    let before_scroll = clipboard.get_text().map_err(|error| format!(
        "Native report clipboard read failed BEFORE scrollbar interaction after initial Ctrl+A/Ctrl+C: {error}; title={:?}, visible={}, client={:?}, expected={}bytes, content={}, TextInput={}, viewport={}",
        report.get_report_title(), report.window().is_visible(), report.window().size(), expected.len(),
        rectangle(&content), rectangle(&text), rectangle(&state.viewport)))?;
    if before_scroll != expected.as_str() {
        return Err(format!("Native report selection/copy failed BEFORE scrollbar interaction: expected {}bytes, copied {}bytes, sentinel={}, copied_prefix={:?}, content={}, TextInput={}, viewport={}",
            expected.len(), before_scroll.len(), before_scroll == "native-scrollbar-selection-sentinel",
            before_scroll.chars().take(160).collect::<String>(), rectangle(&content), rectangle(&text), rectangle(&state.viewport)).into());
    }
    report_clipboard_sentinel(&mut clipboard, "before scrollbar track/drag/wheel")?;
    // Pointer scrolling must retain native TextEdit selection AND focus.
    // Do not refocus/select again before copying, or rely on previous clipboard text.
    let events = exercise_scrollbar(report.window(), &content, "Computation result vertical scrollbar", ScrollSurface::Report, None).await?;
    control_key(report.window(), "c");
    pause(80).await;
    let copied = clipboard.get_text().map_err(|error| format!(
        "Native report clipboard read failed AFTER scrollbar track/drag/wheel and Ctrl+C, with pre-scroll exact-copy verified: {error}; title={:?}, visible={}, client={:?}, expected={}bytes, content={}, TextInput={}, viewport={}",
        report.get_report_title(), report.window().is_visible(), report.window().size(), expected.len(),
        rectangle(&content), rectangle(&text), rectangle(&state.viewport)))?;
    if copied != expected.as_str() {
        return Err(format!("Native scrollbar track/drag lost previously verified report selection or editor focus before copy: expected {}bytes, copied {}bytes, sentinel={}, copied_prefix={:?}, content={}, TextInput={}, viewport={}",
            expected.len(), copied.len(), copied == "native-scrollbar-selection-sentinel",
            copied.chars().take(160).collect::<String>(), rectangle(&content), rectangle(&text), rectangle(&state.viewport)).into());
    }
    Ok(json!({"events": events, "actual_rendered_text_before_gutter": protected,
        "pre_scroll_copy_bytes": before_scroll.len(), "copied_bytes": copied.len(),
        "selection": "pointer focus, Ctrl+A, sentinel roundtrip/verified baseline Ctrl+C, reset sentinel with server read-back, actual track/drag/wheel, Ctrl+C without refocus/reselection, exact native clipboard match"}))
}

async fn actual_report_selection(desktop: &Desktop, output: &Path,
    screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    let report = desktop.controller.borrow().result_windows().last().ok_or("Completed comparison report is absent")?.clone_strong();
    let original_size = report.window().size();
    let scale = report.window().scale_factor();
    report.window().set_size(slint::PhysicalSize::new((760.0 * scale).round() as u32, (440.0 * scale).round() as u32));
    pause(100).await;
    let proof = report_selection(&report).await?;
    snapshot(&report, output, "09-actual-comparison-report-selection-after-scroll", screenshots)?;
    report.window().set_size(original_size);
    pause(80).await;
    checks.push(json!({"workflow": "actual completed comparison report selection survives scrollbar interaction",
        "report_text": "unchanged actual controller result", "proof": proof}));
    Ok(())
}

fn rendered_candidate(content: &ElementHandle, name: &str, role: AccessibleRole) -> Result<ElementHandle> {
    let viewport = content.query_descendants().match_inherits("Flickable").find_first().ok_or("Candidate viewport is absent")?;
    let controls = content.query_descendants().find_all();
    let row = controls.iter()
        .find(|row| row.accessible_label().is_some_and(|label| label == name) && row.accessible_role() == Some(role));
    if let Some(row) = row {
        if row.absolute_position().y >= viewport.absolute_position().y - 1.0 &&
            row.absolute_position().y + row.size().height <= viewport.absolute_position().y + viewport.size().height + 1.0 {
            return Ok(row.clone());
        }
    }
    let bar = controls.iter().find(|control| control.accessible_role() == Some(AccessibleRole::Slider));
    Err(format!("Keyboard did not bring candidate {name:?} fully into its real viewport: role={role:?}, row={}, inner_viewport={}, container={}, container_valid={}, container_description={:?}, item_count={:?}, scrollbar={}",
        row.map(rectangle).unwrap_or(Value::Null), rectangle(&viewport), rectangle(content), content.is_valid(), content.accessible_description(), content.accessible_item_count(),
        bar.map(|bar| json!({"rectangle": rectangle(bar), "offset": bar.accessible_value().map(|value| value.to_string()),
            "maximum": bar.accessible_value_maximum()})).unwrap_or(Value::Null)).into())
}

async fn center_scroll_surface(desktop: &Desktop, content: &ElementHandle, label: &str) -> Result<Value> {
    let outer = content_viewport(desktop)?;
    // A fixed gear list may be taller than the compact outer viewport. Center
    // its real bounds so clipping is balanced; only interactive leaves must fit.
    let desired_y = outer.absolute_position().y + (outer.size().height - content.size().height) / 2.0;
    let delta = desired_y - content.absolute_position().y;
    if delta.abs() > 1.0 { scroll_content(desktop, delta).await?; }
    let outer = content_viewport(desktop)?;
    let clip = PointerClip::new(desktop.window.window(), Some(&outer));
    clip.check(slint::LogicalPosition::new(content.absolute_position().x + 2.0,
        content.absolute_position().y + content.size().height / 2.0), label)?;
    Ok(json!({"content": rectangle(content), "outer_viewport": rectangle(&outer), "pointer_clip": clip.geometry()}))
}

async fn radio_keyboard(desktop: &Desktop) -> Result<Value> {
    tab(desktop, 0).await?;
    let slot = desktop.window.get_quick_slots().row_data(Slot::Main.index()).ok_or("Main slot is absent")?;
    let slot_button = geometry_element(desktop, &format!("{}: {}", slot.label, slot.name), AccessibleRole::Button).await?;
    slot_button.single_click(PointerEventButton::Left).await;
    pause(60).await;
    let rows = desktop.window.get_quick_items().iter().collect::<Vec<_>>();
    if rows.len() < 3 { return Err("Native radio keyboard probe needs at least three actual candidates".into()); }
    let choices = element(&desktop.window, "Available equipment", AccessibleRole::List)?;
    let alignment = center_scroll_surface(desktop, &choices, "Available equipment").await?;
    native_wheel(desktop.window.window(), &choices, "Available equipment vertical scrollbar", ScrollSurface::Standard, 1_000_000.0).await?;
    geometry_element(desktop, rows[0].name.as_str(), AccessibleRole::RadioButton).await?;
    let first = rendered_candidate(&choices, rows[0].name.as_str(), AccessibleRole::RadioButton)?;
    let outer = content_viewport(desktop)?;
    PointerClip::new(desktop.window.window(), Some(&outer)).check(slint::LogicalPosition::new(
        first.absolute_position().x + first.size().width / 2.0, first.absolute_position().y + first.size().height / 2.0),
        "first native radio candidate")?;
    first.single_click(PointerEventButton::Left).await;
    pause(60).await;
    let mut transitions = Vec::new();
    for (pressed, key_name, expected) in [(Key::DownArrow, "DownArrow", 1), (Key::UpArrow, "UpArrow", 0),
        (Key::End, "End", rows.len() - 1), (Key::Home, "Home", 0)] {
        key(desktop.window.window(), pressed);
        pause(60).await;
        let state = desktop.controller.borrow();
        let selected_id = state.ui.gear.equipped[GearSource::Quicklook.index()][Slot::Main.index()].index() as i32;
        if selected_id != rows[expected].id {
            return Err(format!("Native radio {key_name} did not equip the actual candidate {}", rows[expected].name).into());
        }
        drop(state);
        geometry_element(desktop, rows[expected].name.as_str(), AccessibleRole::RadioButton).await?;
        let outer = content_viewport(desktop)?;
        let row = rendered_candidate(&choices, rows[expected].name.as_str(), AccessibleRole::RadioButton)
            .map_err(|error| format!("{error}; key={key_name}, selected_id={selected_id}, expected_id={}, outer_viewport={}",
                rows[expected].id, rectangle(&outer)))?;
        transitions.push(json!({"key": key_name, "candidate": rows[expected].name.to_string(), "row": rectangle(&row),
            "outer_viewport": rectangle(&content_viewport(desktop)?)}));
    }
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    pause(80).await;
    Ok(json!({"source": "real controller-backed radio candidates", "rows": rows.len(), "container_alignment": alignment, "transitions": transitions}))
}

async fn native_scrollbars(desktop: &Desktop, output: &Path, width: f32, height: f32,
    screenshots: &mut Vec<Value>, checks: &mut Vec<Value>) -> Result<()> {
    // Length/virtualization stress lives in a disposable native Desktop, never
    // in the controller/fixtures used by the comprehensive calculation workflow.
    let paths = desktop.controller.borrow().paths.clone();
    let proof = Desktop::with_random(load_resources_at(paths)?, RandomSources::from_seed(11))?;
    proof.window.show()?;
    // Newly shown Winit windows publish their monitor scale during native
    // mapping. Wait for that real event before converting logical dimensions.
    pause(100).await;
    let scale = proof.window.window().scale_factor();
    proof.window.window().set_size(slint::PhysicalSize::new((width * scale).round() as u32, (height * scale).round() as u32));
    proof.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    pause(100).await;
    assert_native_logical_size(proof.window.window(), width, height, "Disposable scrollbar Main")?;
    let prefix = format!("12-scrollbars-{}x{}", width as u32, height as u32);
    let mut surfaces = Vec::new();
    for (index, label) in [(0, "Quicklook content"), (1, "Optimize content"), (2, "Simulate content"), (3, "Stats content")] {
        tab(&proof, index).await?;
        let content = element(&proof.window, label, AccessibleRole::List)?;
        let events = exercise_scrollbar(proof.window.window(), &content, &format!("{label} vertical scrollbar"), ScrollSurface::Standard, None).await?;
        let mut protected = Vec::new();
        if index == 0 {
            let state = scroll_state(&content, "Quicklook content vertical scrollbar", ScrollSurface::Standard)?;
            for label in ["Enemy preset", "enemy Magic Evasion", "Resistance rank"] {
                protected.push(clear_scrollbar_lane(&element(&proof.window, label, if label == "enemy Magic Evasion" {
                    AccessibleRole::TextInput
                } else { AccessibleRole::Combobox })?, &state, label)?);
            }
        }
        snapshot(&proof.window, output, &format!("{prefix}-outer-{index}"), screenshots)?;
        surfaces.push(json!({"surface": label, "events": events, "rightmost_controls": protected}));
    }
    let radio = radio_keyboard(&proof).await?;
    tab(&proof, 0).await?;
    scroll_content(&proof, 1_000_000.0).await?;
    let scalar_rows = proof.window.get_ability_controls().iter().filter(|row| row.visible).collect::<Vec<_>>();
    let first_scalar = scalar_rows.first().ok_or("No visible scalar rows for the native nested-scroll probe")?.clone();
    proof.window.set_ability_controls(slint::ModelRc::new(slint::VecModel::from(
        (0..48).map(|index| scalar_rows[index % scalar_rows.len()].clone()).collect::<Vec<_>>())));
    pause(80).await;
    let scalar = element(&proof.window, "Special Toggles inputs", AccessibleRole::List)?;
    let scalar_outer = content_viewport(&proof)?;
    let scalar_events = exercise_scrollbar(proof.window.window(), &scalar, "Special Toggles inputs vertical scrollbar", ScrollSurface::Standard, Some(&scalar_outer)).await?;
    let scalar_state = scroll_state(&scalar, "Special Toggles inputs vertical scrollbar", ScrollSurface::Standard)?;
    let editor = scalar.query_descendants().match_inherits("ControlEditor").find_first().ok_or("Nested scalar lost its editor row")?;
    let scalar_protected = clear_scrollbar_lane(&editor, &scalar_state, "nested scalar editor")?;
    snapshot(&proof.window, output, &format!("{prefix}-nested-scalar"), screenshots)?;
    proof.window.set_ability_controls(slint::ModelRc::new(slint::VecModel::from(vec![first_scalar])));
    pause(80).await;
    let fitting_scalar = scroll_state(&scalar, "Special Toggles inputs vertical scrollbar", ScrollSurface::Standard)?;
    if fitting_scalar.bar.is_some() { return Err("A fitting scalar list retained a visible handle".into()); }
    snapshot(&proof.window, output, &format!("{prefix}-fitting-scalar"), screenshots)?;
    surfaces.push(json!({"surface": "nested scalar", "rows": 48, "events": scalar_events,
        "rightmost_editor": scalar_protected, "fitting": fitting_scalar.geometry}));

    tab(&proof, 1).await?;
    scroll_content(&proof, 1_000_000.0).await?;
    let candidates = element(&proof.window, "Gear candidates", AccessibleRole::List)?;
    center_scroll_surface(&proof, &candidates, "Gear candidates").await?;
    let first_choice = proof.window.get_optimize_items().row_data(0).ok_or("No actual checkbox candidate to focus")?;
    geometry_element(&proof, first_choice.name.as_str(), AccessibleRole::Checkbox).await?;
    let first = rendered_candidate(&candidates, first_choice.name.as_str(), AccessibleRole::Checkbox)?;
    let outer = content_viewport(&proof)?;
    PointerClip::new(proof.window.window(), Some(&outer)).check(slint::LogicalPosition::new(
        first.absolute_position().x + first.size().width / 2.0, first.absolute_position().y + first.size().height / 2.0),
        "first native checkbox candidate")?;
    first.single_click(PointerEventButton::Left).await;
    pause(60).await;
    let actual = proof.window.get_optimize_items().iter().collect::<Vec<_>>();
    if actual.is_empty() { return Err("No actual candidates for the large native virtual list".into()); }
    let large_rows = (0..2048).map(|index| {
        let mut row = actual[index % actual.len()].clone();
        row.name = format!("Native candidate {index:04}: {}", row.name).into();
        row.tooltip = format!("{}\n{}", row.tooltip, "Native inspector wrapping and scrolling retains the actual item details.\n".repeat(24)).into();
        row
    }).collect::<Vec<_>>();
    let first_name = large_rows[0].name.clone();
    let last_name = large_rows.last().ok_or("Large candidate population is absent")?.name.clone();
    let pools = proof.controller.borrow().ui.gear.candidate_pools();
    proof.window.set_optimize_items(slint::ModelRc::new(slint::VecModel::from(large_rows)));
    pause(80).await;
    let mut keyboard = Vec::new();
    for (pressed, key_name, name) in [(Key::End, "End", last_name), (Key::Home, "Home", first_name)] {
        key(proof.window.window(), pressed);
        pause(80).await;
        geometry_element(&proof, name.as_str(), AccessibleRole::Checkbox).await?;
        let row = rendered_candidate(&candidates, name.as_str(), AccessibleRole::Checkbox)?;
        keyboard.push(json!({"key": key_name, "candidate": name.to_string(), "rectangle": rectangle(&row),
            "outer_viewport": rectangle(&content_viewport(&proof)?)}));
    }
    key(proof.window.window(), Key::PageDown);
    pause(80).await;
    let paged = scroll_state(&candidates, "Gear candidates vertical scrollbar", ScrollSurface::Standard)?;
    if paged.offset <= 0.5 || proof.controller.borrow().ui.gear.candidate_pools() != pools {
        return Err("Large candidate keyboard paging failed or changed selection without activation".into());
    }
    key(proof.window.window(), Key::Home);
    pause(80).await;
    let realized = candidates.query_descendants().match_inherits("GearCheckRow").find_all().len();
    if realized == 0 || realized >= 256 { return Err(format!("The2048-row candidate list lost native virtualization: {realized} delegates").into()); }
    let candidate_alignment = center_scroll_surface(&proof, &candidates, "Gear candidates").await?;
    let candidate_outer = content_viewport(&proof)?;
    let candidate_events = exercise_scrollbar(proof.window.window(), &candidates, "Gear candidates vertical scrollbar", ScrollSurface::Standard, Some(&candidate_outer)).await?;
    let candidate_state = scroll_state(&candidates, "Gear candidates vertical scrollbar", ScrollSurface::Standard)?;
    let candidate_row = candidates.query_descendants().match_inherits("GearCheckRow").find_first().ok_or("Native candidate row disappeared")?;
    let candidate_protected = clear_scrollbar_lane(&candidate_row, &candidate_state, "candidate rightmost canvas")?;
    snapshot(&proof.window, output, &format!("{prefix}-virtual-candidates"), screenshots)?;
    // Metadata queries exclude fully clipped items: reveal the real inspector
    // before requiring its source-declared GearPanel::details-scroll handle.
    let mut details = ElementHandle::find_by_element_id(&proof.window, "GearPanel::details-scroll").next();
    for _ in 0..24 {
        if let Some(details) = &details {
            if fully_visible(&proof, details)? { break; }
        }
        scroll_content(&proof, -80.0).await?;
        details = ElementHandle::find_by_element_id(&proof.window, "GearPanel::details-scroll").next();
    }
    let details = details.ok_or("Native inspector scroll surface is absent after actual outer scrolling")?;
    if !fully_visible(&proof, &details)? { return Err("Native inspector cannot be fully reached through outer scrolling".into()); }
    let inspector_outer = content_viewport(&proof)?;
    let inspector_events = exercise_scrollbar(proof.window.window(), &details, "Item details vertical scrollbar", ScrollSurface::Standard, Some(&inspector_outer)).await?;
    let inspector_state = scroll_state(&details, "Item details vertical scrollbar", ScrollSurface::Standard)?;
    if inspector_state.bar.is_none() { return Err("Long native inspector details did not expose an overflow handle".into()); }
    let inspector_protected = details.query_descendants().match_type_name("Text").find_all().iter()
        .map(|text| clear_scrollbar_lane(text, &inspector_state, "inspector wrapped text")).collect::<Result<Vec<_>>>()?;
    snapshot(&proof.window, output, &format!("{prefix}-inspector"), screenshots)?;
    proof.window.set_optimize_items(slint::ModelRc::new(slint::VecModel::from(Vec::<wsdist_app::GearChoiceRow>::new())));
    pause(80).await;
    let empty_candidates = scroll_state(&candidates, "Gear candidates vertical scrollbar", ScrollSurface::Standard)?;
    if empty_candidates.bar.is_some() { return Err("Empty candidates retained a visible handle".into()); }
    snapshot(&proof.window, output, &format!("{prefix}-empty-candidates"), screenshots)?;
    surfaces.push(json!({"surface": "native virtual candidates and inspector", "rows": 2048, "realized_delegates": realized,
        "keyboard": keyboard, "keyboard_page_down": paged.geometry, "navigation": "selection retained without activation",
        "candidate_alignment": candidate_alignment, "candidate_events": candidate_events, "candidate_rightmost_canvas": candidate_protected,
        "inspector_events": inspector_events, "inspector_wrapped_text": inspector_protected, "empty": empty_candidates.geometry}));
    proof.controller.borrow_mut().shutdown();
    proof.window.hide()?;

    let progress = ProgressWindow::new()?;
    let lines = (0..1024).map(|index| if index < 256 {
        format!("Progress {index:04}: short estimate").into()
    } else {
        format!("Progress {index:04}: {}", "variable-height estimated range ".repeat(8 + index % 24)).into()
    }).collect::<Vec<SharedString>>();
    progress.set_lines(slint::ModelRc::new(slint::VecModel::from(lines)));
    progress.show()?;
    pause(100).await;
    let progress_scale = progress.window().scale_factor();
    progress.window().set_size(slint::PhysicalSize::new((width * progress_scale).round() as u32, (height * progress_scale).round() as u32));
    pause(100).await;
    assert_native_logical_size(progress.window(), width, height, "Native progress scrollbar")?;
    let progress_content = element(&progress, "Optimizer progress content", AccessibleRole::List)?;
    let progress_events = exercise_scrollbar(progress.window(), &progress_content, "Optimizer progress content vertical scrollbar", ScrollSurface::Standard, None).await?;
    let estimates = progress_events["thumb_down_pointer_events"].as_array().ok_or("Variable-height progress did not exercise native thumb events")?
        .iter().map(|sample| sample["state"]["maximum"].as_f64().ok_or("Progress range sample is absent")).collect::<Result<Vec<_>, _>>()?;
    if !estimates.iter().any(|value| (value - estimates[0]).abs() > 1.0) {
        return Err("Variable-height native progress drag did not visit a changed estimated range".into());
    }
    let progress_realized = progress_content.query_descendants().match_type_name("Text").find_all().len();
    if progress_realized == 0 || progress_realized >= 256 { return Err(format!("Progress list lost virtualization: {progress_realized} delegates").into()); }
    snapshot(&progress, output, &format!("{prefix}-variable-progress"), screenshots)?;
    progress.set_lines(slint::ModelRc::new(slint::VecModel::<SharedString>::default()));
    pause(80).await;
    let empty_progress = scroll_state(&progress_content, "Optimizer progress content vertical scrollbar", ScrollSurface::Standard)?;
    progress.set_lines(slint::ModelRc::new(slint::VecModel::from(vec![SharedString::from("One fitting progress line")])));
    pause(80).await;
    let fitting_progress = scroll_state(&progress_content, "Optimizer progress content vertical scrollbar", ScrollSurface::Standard)?;
    if empty_progress.bar.is_some() || fitting_progress.bar.is_some() { return Err("Empty/fitting progress retained a handle".into()); }
    snapshot(&progress, output, &format!("{prefix}-fitting-progress"), screenshots)?;
    surfaces.push(json!({"surface": "native variable-height progress", "rows": 1024, "realized_delegates": progress_realized,
        "estimated_ranges_changed_during_drag": estimates, "events": progress_events,
        "empty": empty_progress.geometry, "fitting": fitting_progress.geometry}));
    progress.hide()?;

    let actual_report = desktop.controller.borrow().result_windows().iter().rev()
        .find(|report| report.get_report_title().as_str() == "TP & WS stat comparison")
        .ok_or("Completed TP & WS stat comparison report is absent")?.get_report_text();
    let report_text: SharedString = actual_report.as_str().repeat(4).into();
    let report = ResultWindow::new()?;
    report.set_report_title("Native report scrollbar and selection".into());
    report.set_report_text(report_text);
    report.show()?;
    pause(100).await;
    let report_scale = report.window().scale_factor();
    report.window().set_size(slint::PhysicalSize::new((width * report_scale).round() as u32, (height * report_scale).round() as u32));
    pause(100).await;
    assert_native_logical_size(report.window(), width, height, "Native report scrollbar")?;
    let report_content = ElementHandle::find_by_element_type_name(&report, "ScrollableTextEdit").next().ok_or("Wrapped native report editor is absent")?;
    if scroll_state(&report_content, "Computation result vertical scrollbar", ScrollSurface::Report)?.bar.is_none() {
        return Err("Real comparison report repeated4x must overflow before native scrollbar selection/copy proof".into());
    }
    let selection_proof = report_selection(&report).await?;
    snapshot(&report, output, &format!("{prefix}-report-selection"), screenshots)?;
    report.set_report_text("".into());
    pause(80).await;
    let empty_report = scroll_state(&report_content, "Computation result vertical scrollbar", ScrollSurface::Report)?;
    if empty_report.bar.is_some() { return Err("Empty report retained a visible handle".into()); }
    snapshot(&report, output, &format!("{prefix}-empty-report"), screenshots)?;
    report.set_report_text("One fitting report line".into());
    pause(80).await;
    let fitting_report = scroll_state(&report_content, "Computation result vertical scrollbar", ScrollSurface::Report)?;
    if fitting_report.bar.is_some() { return Err("Fitting report retained a visible handle".into()); }
    snapshot(&report, output, &format!("{prefix}-fitting-report"), screenshots)?;
    surfaces.push(json!({"surface": "native report TextEdit selection/copy", "source": "completed TP/WS comparison repeated4x to overflow",
        "proof": selection_proof,
        "empty": empty_report.geometry, "fitting": fitting_report.geometry}));
    report.hide()?;
    checks.push(json!({"workflow": "native explicit vertical scrollbar interactions", "requested_logical_size": [width, height],
        "scale_factor": scale, "bar_width": 18, "thumb_width": 14, "minimum_thumb_length": 32,
        "native_radio_keyboard": radio, "surfaces": surfaces}));
    Ok(())
}

async fn workflow(desktop: &Desktop, output: &Path, fixtures: &Path, index: &FixtureValue, ninja: &FixtureValue,
    missing_icon: &MissingIcon) -> Result<()> {
    let mut screenshots = Vec::new();
    let mut checks = Vec::new();
    pause(180).await;
    compact_dropdowns(desktop, output, &mut screenshots, &mut checks).await?;
    missing_icon_workflow(desktop, missing_icon, output, &mut screenshots, &mut checks).await?;
    let default = fixture(fixtures, index, "ui.profile.22.default")?;
    assert_sets(desktop, &default["expected"]["value"]["initial_state"]["equipped_sets"])?;
    snapshot(&desktop.window, output, "00-quicklook-initial", &mut screenshots)?;
    assert_input_spacing(desktop)?;
    for (i, name) in [(0, "quicklook"), (1, "optimize"), (2, "simulate"), (3, "stats")] {
        tab(desktop, i).await?;
        snapshot(&desktop.window, output, &format!("01-tab-{name}"), &mut screenshots)?;
    }
    checks.push(json!({"workflow": "four tabs", "shortcuts": "Ctrl+1..4", "backend": "native Winit/software"}));
    tab(desktop, 0).await?;

    // Profile switches use the real controller callback, including legacy restoration order.
    let mut profile_baseline = HorizontalBaseline::new();
    let mut profile_geometry = Vec::new();
    let mut initial_rectangles = Vec::new();
    quick_input_geometry(desktop, &mut profile_baseline, &mut initial_rectangles).await?;
    profile_geometry.push(json!({"profile": "default", "geometry": initial_rectangles}));
    for entry in index["cases"].as_array().ok_or("Cases missing")?.iter().filter(|entry| entry["id"].as_str().is_some_and(|id| id.starts_with("ui.profile."))) {
        let document = wsdist_fixtures::read_value(&fixtures.join(entry["path"].as_str().ok_or("Profile fixture path")?), Some(entry["content_xxh3_128"].as_str().ok_or("Profile fixture XXH3-128 content hash")?))
            .map_err(|error| -> Box<dyn Error> { error })?;
        let job = document["input"]["profile_name"].as_str().ok_or("Profile name")?;
        if job == "default" { continue; } // Default was compared at startup.
        desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), job.into());
        pause(16).await;
        let expected = &document["expected"]["value"]["steps"][0]["state"];
        assert_sets(desktop, &expected["equipped_sets"])?;
        if desktop.controller.borrow().ui.text("defaults_mainjob_combobox") != job { return Err(format!("Profile switch did not retain {job}").into()); }
        assert_input_spacing(desktop)?;
        let mut rectangles = Vec::new();
        quick_input_geometry(desktop, &mut profile_baseline, &mut rectangles).await?;
        profile_geometry.push(json!({"profile": job, "geometry": rectangles}));
    }
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    pause(80).await;
    let frozen = &ninja["expected"]["value"];
    assert_sets(desktop, &frozen["steps"][0]["state"]["equipped_sets"])?;
    checks.push(json!({"workflow": "all shipped profiles", "effective_sets": "all 16 slots in Quicklook, TP and WS compared to frozen states",
        "fixed_input_geometry": profile_geometry}));

    tab(desktop, 3).await?;
    let mut stats_baseline = HorizontalBaseline::new();
    let mut stats_geometry = Vec::new();
    for (step, label) in [(1, "Quicklook Gear Stats"), (2, "TP Gear Stats"), (3, "WS Gear Stats")] {
        compute(desktop, label).await?;
        let expected = &frozen["steps"][step]["state"]["displayed_stats"];
        if FixtureValue::Object(displayed_stats(desktop)) != *expected { return Err(format!("Frozen {label} display differs").into()); }
        snapshot(&desktop.window, output, &format!("02-stats-source-{step}"), &mut screenshots)?;
        let mut rectangles = Vec::new();
        record_stats_geometry(desktop, &mut stats_baseline, &mut rectangles).await?;
        stats_geometry.push(json!({"source": label, "geometry": rectangles}));
    }
    checks.push(json!({"workflow": "displayed Stats content transitions retain fixed labels/values", "sources": stats_geometry}));
    tab(desktop, 0).await?;
    for (step, label) in [(4, "Quicklook TP"), (5, "Quicklook WS"), (6, "Quicklook Spell")] {
        compute(desktop, label).await?;
        assert_display(desktop, &frozen["steps"][step]["state"]["displayed_analytical"])?;
        snapshot(&desktop.window, output, &format!("03-analytic-{step}"), &mut screenshots)?;
    }
    snapshot_result(desktop, output, "03-analytic-report", &mut screenshots)?;
    compute(desktop, "Simulate WS").await?;
    {
        let state = desktop.controller.borrow();
        match state.last_result.as_ref() {
            Some(WorkResult::SingleWs { events, .. }) if events.iter().any(|event| event.attempt > 0) => {},
            _ => return Err("Single WS did not return its actual attempted-hit trace".into()),
        }
    }
    snapshot_result(desktop, output, "04-single-ws-trace", &mut screenshots)?;
    checks.push(json!({"workflow": "analytical and Stats", "reference": "ui.profile.10.ninja steps1..6", "single_ws": "sampled attempted-hit trace"}));
    buff_workflows(desktop, output, fixtures, index, &mut screenshots, &mut checks).await?;
    generated_variants(desktop, output, &mut screenshots, &mut checks).await?;

    // Native entry keyboard focus, q protection, Return normalization and wheel.
    let prior_display = (desktop.window.get_damage(), desktop.window.get_tp_return(), displayed_stats(desktop));
    let prior_jobs = desktop.controller.borrow().completed_jobs;
    element(&desktop.window, "Job, spell, weapon skill & TP inputs", AccessibleRole::List)?.scroll(0.0, -1000.0);
    pause(80).await;
    let tp = element(&desktop.window, "TP", AccessibleRole::TextInput)?;
    snapshot(&desktop.window, output, "04-tp-editor", &mut screenshots)?;
    tp.single_click(PointerEventButton::Left).await;
    pause(30).await;
    control_key(desktop.window.window(), "a");
    key(desktop.window.window(), "q");
    pause(30).await;
    if !desktop.window.window().is_visible() || desktop.controller.borrow().ui.text("defaults_tp_entry") != "q" {
        return Err("Typing q in a focused entry closed the GUI or failed to edit".into());
    }
    key(desktop.window.window(), Key::Return);
    pause(30).await;
    if desktop.controller.borrow().ui.text("defaults_tp_entry") != "1000" { return Err("TP Return normalization failed".into()); }
    tp.scroll(0.0, 120.0);
    pause(30).await;
    if desktop.controller.borrow().ui.text("defaults_tp_entry") != "1100" { return Err("TP wheel did not add100".into()); }
    if (desktop.window.get_damage(), desktop.window.get_tp_return(), displayed_stats(desktop)) != prior_display ||
        desktop.controller.borrow().completed_jobs != prior_jobs { return Err("Editing inputs automatically recomputed displayed results or Stats".into()); }
    desktop.window.invoke_text_edited("defaults_tp_entry".into(), "1800".into());
    checks.push(json!({"workflow": "native TP keyboard and wheel", "q": "retained as entry text", "invalid_commit": 1000, "wheel": 1100}));

    // Explicit failed numeric input: no successful/partial result, visible details.
    let previous = desktop.controller.borrow().completed_jobs;
    desktop.window.invoke_text_edited("defaults_enemy_Defense_entry".into(), "invalid".into());
    click(desktop, "Quicklook WS").await?;
    while desktop.controller.borrow().is_busy() { pause(16).await; }
    if desktop.controller.borrow().completed_jobs != previous || desktop.window.get_error_message().is_empty() { return Err("Invalid numeric input was not visibly rejected".into()); }
    snapshot(&desktop.window, output, "05-visible-numeric-error", &mut screenshots)?;
    click(desktop, "Error details").await?;
    snapshot_result(desktop, output, "05-error-details", &mut screenshots)?;
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    pause(40).await;

    // Distinct sets, all direct-copy routes, and retained native clipboard ownership.
    let mut exports = Vec::new();
    for (source, action) in [(GearSource::Quicklook, "export-quicklook"), (GearSource::Tp, "export-tp"), (GearSource::Ws, "export-ws")] {
        desktop.window.invoke_action(action.into());
        pause(50).await;
        let text = arboard::Clipboard::new()?.get_text()?;
        let expected = wsdist_app::exchange::export_lua(&desktop.controller.borrow().ui.engine.catalog, &desktop.controller.borrow().ui.gear.equipped, source);
        if text != expected { return Err(format!("Clipboard did not serve {source} export").into()); }
        exports.push(text);
    }
    let copied_display = (desktop.window.get_damage(), displayed_stats(desktop));
    let original_sets = desktop.controller.borrow().ui.gear.equipped;
    for (source, destination) in [("quicklook", "tp"), ("quicklook", "ws"), ("tp", "quicklook"), ("ws", "quicklook")] {
        desktop.window.invoke_copy(source.into(), destination.into());
        let state = desktop.controller.borrow();
        let source = match source { "quicklook" =>0, "tp" =>1, _=>2 };
        let destination = match destination { "quicklook" =>0, "tp" =>1, _=>2 };
        if state.ui.gear.equipped[source] != state.ui.gear.equipped[destination] { return Err("Copy route used the wrong set".into()); }
        if (desktop.window.get_damage(), displayed_stats(desktop)) != copied_display { return Err("Direct copy refreshed stale displayed results or Stats".into()); }
    }
    desktop.controller.borrow_mut().ui.gear.equipped = original_sets;
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    checks.push(json!({"workflow": "copy and native clipboard", "routes": 4, "clipboard_exports": exports}));

    // Transactional inventory cancel/read failure and the real matching importer.
    let before = desktop.controller.borrow().ui.gear.candidate_pools();
    if desktop.controller.borrow_mut().import_inventory(None)? != ImportOutcome::Cancelled { return Err("Inventory cancellation outcome".into()); }
    let missing = desktop.controller.borrow().paths.root.join("not-present-inventory.lua");
    if desktop.controller.borrow_mut().import_inventory(Some(&missing)).is_ok() { return Err("Missing inventory was accepted".into()); }
    if desktop.controller.borrow().ui.gear.candidate_pools() != before { return Err("Inventory error changed selection".into()); }
    let inventory = desktop.controller.borrow().paths.root.join("inventory.lua");
    fs::write(&inventory, "return {\nmain=\"Heishi Shorinken\",\nhead=\"Hattori Zukin +2\"\n}\n")?;
    desktop.controller.borrow_mut().import_inventory(Some(&inventory))?;
    let imported_main = desktop.controller.borrow().ui.engine.catalog.by_name2("Heishi Shorinken R15").ok_or("Imported main alias missing")?;
    if !desktop.controller.borrow().ui.gear.candidate_pools()[Slot::Main.index()].contains(&imported_main) { return Err("Inventory did not select the base-name/CSV alias".into()); }
    let imported_head = desktop.controller.borrow().ui.engine.catalog.by_name2("Hattori Zukin +2").ok_or("Imported head alias missing")?;
    if !desktop.controller.borrow().ui.gear.candidate_pools()[Slot::Head.index()].contains(&imported_head) { return Err("Inventory did not select the second quoted line".into()); }
    checks.push(json!({"workflow": "inventory", "cancel_and_read_failure": "retain candidates", "matching": "Heishi Shorinken/Hattori Zukin+2"}));

    // Persist all three sets, relaunch a native component, then reload transactionally.
    desktop.window.invoke_action("save-defaults".into());
    if !desktop.window.get_error_message().is_empty() { return Err("Save Defaults failed".into()); }
    let paths = desktop.controller.borrow().paths.clone();
    let saved = desktop.controller.borrow().ui.effective_profile();
    let relaunched = Desktop::with_random(load_resources_at(paths.clone())?, RandomSources::from_seed(11))?;
    if relaunched.controller.borrow().ui.effective_profile() != saved { return Err("Save/relaunch lost effective widget/gear state".into()); }
    drop(relaunched);
    let (gear_path, mut edited) = resource_support::piece_fragment(&paths.data.join("gear"), "Heishi Shorinken R15")?;
    let original = fs::read(&gear_path)?;
    let old_catalog = desktop.controller.borrow().ui.engine.catalog.clone();
    fs::write(&gear_path, b"{broken")?;
    desktop.window.invoke_action("reload-data".into());
    if !std::sync::Arc::ptr_eq(&old_catalog, &desktop.controller.borrow().ui.engine.catalog) { return Err("Failed reload changed catalog".into()); }
    fs::write(&gear_path, &original)?;
    desktop.window.invoke_action("reload-data".into());
    if std::sync::Arc::ptr_eq(&old_catalog, &desktop.controller.borrow().ui.engine.catalog) { return Err("Successful reload failed to replace catalog".into()); }
    // Edit one existing item's inert stat record; the actual Stats action must
    // observe the change, then restore the original catalog before simulation.
    let piece = edited["pieces"].as_array_mut().ok_or("Gear records are missing")?.iter_mut()
        .find(|piece| resource_support::record_name2(piece) == Some("Heishi Shorinken R15")
            && piece["slots"].as_array().is_some_and(|slots| !slots.is_empty()))
        .ok_or("Editable main weapon is missing")?;
    let original_str = piece["stats"]["STR"].as_i64().unwrap_or(0);
    piece["stats"]["STR"] = json!(original_str + 10);
    // JSON is a JSON5 subset and preserves float literals such as 1.0 and -0.0.
    fs::write(&gear_path, serde_json::to_vec(&edited)?)?;
    desktop.window.invoke_action("reload-data".into());
    tab(desktop, 3).await?;
    compute(desktop, "Quicklook Gear Stats").await?;
    let expected_str = frozen["steps"][1]["state"]["displayed_stats"]["STR"].as_str().ok_or("Frozen STR is missing")?.parse::<i64>()? + 10;
    if displayed_stats(desktop).get("STR") != Some(&FixtureValue::from(expected_str.to_string())) {
        return Err(format!("Edited main-weapon STR did not reach the displayed result {expected_str}").into());
    }
    snapshot(&desktop.window, output, "05-edited-data-stats", &mut screenshots)?;
    fs::write(&gear_path, &original)?;
    desktop.window.invoke_action("reload-data".into());
    tab(desktop, 0).await?;
    checks.push(json!({"workflow": "existing-item JSON5 edit", "item": "Heishi Shorinken R15",
        "edit": "STR +10", "expected_displayed_STR": expected_str}));
    checks.push(json!({"workflow": "atomic save/relaunch and reload", "invalid_reload": "old catalog retained", "valid_reload": "new catalog committed"}));

    tab(desktop, 1).await?;
    for (key, value) in [("defaults_optimizer_seed_entry","11"), ("defaults_optimizer_iterations_entry","1"),
        ("defaults_optimizer_restart_count_entry","1"), ("defaults_optimizer_workers_entry","1"), ("defaults_optimizer_swap_slots_combobox","1")] {
        desktop.window.invoke_text_edited(key.into(), value.into());
    }
    {
        let mut state = desktop.controller.borrow_mut();
        let mut candidates = std::array::from_fn(|_| Vec::new());
        candidates[Slot::Head.index()] = ["Empty", "Malignance Chapeau", "Hattori Zukin +2", "Ryuo Somen +1 A"].iter()
            .map(|name| state.ui.engine.catalog.by_name2(name).ok_or_else(|| format!("Missing candidate {name}"))).collect::<std::result::Result<_,_>>()?;
        state.ui.gear.replace_candidates(&candidates);
    }
    let previous = desktop.controller.borrow().completed_jobs;
    click(desktop, "Optimize WS").await?;
    if let Some(progress) = desktop.controller.borrow().progress_window() { snapshot(progress, output, "06-optimizer-progress", &mut screenshots)?; }
    finished(desktop, previous).await?;
    let best = desktop.controller.borrow().best_set().ok_or("Optimizer has no real best player")?;
    let prior_stats = displayed_stats(desktop);
    click(desktop, "Equip best set").await?;
    if desktop.controller.borrow().ui.gear.equipped[0] != best { return Err("Equip best set did not install actual result".into()); }
    if displayed_stats(desktop) != prior_stats { return Err("Equip best set recomputed stale displayed Stats".into()); }
    snapshot(&desktop.window, output, "06-equipped-best", &mut screenshots)?;
    checks.push(json!({"workflow": "seed11 small optimizer", "equip_best": "actual returned player set"}));

    tab(desktop, 2).await?;
    desktop.window.invoke_toggled("simulate_tab.plot_dps_checkbox".into(), true);
    compute(desktop, "Run DPS simulations").await?;
    {
        let state = desktop.controller.borrow();
        match state.last_result.as_ref() {
            Some(WorkResult::Dps { result, plot: true, .. }) => {
                let combined = result.tp_damage + result.ws_damage;
                let tolerance = 1e-9 + 1e-12 * result.damage.abs().max(combined.abs());
                if result.final_time < 7200.0 || (result.damage - combined).abs() > tolerance ||
                    result.total_series.last() != Some(&[result.final_time, result.damage]) ||
                    result.tp_series.last().map(|point| point[1]) != Some(result.tp_damage) ||
                    result.ws_series.last().map(|point| point[1]) != Some(result.ws_damage) {
                    return Err(format!("Full DPS final cycle/series mismatch: time {}, total {}, TP {}, WS {}", result.final_time, result.damage, result.tp_damage, result.ws_damage).into());
                }
            },
            _ => return Err("Full DPS calculation returned the wrong operation or omitted the requested plot".into()),
        }
        snapshot(state.chart_windows().last().ok_or("No DPS chart")?, output, "07-dps-three-curves", &mut screenshots)?;
    }
    snapshot_result(desktop, output, "07-dps-summary", &mut screenshots)?;
    compute(desktop, "Create WS damage distribution plot").await?;
    {
        let state = desktop.controller.borrow();
        match state.last_result.as_ref() {
            Some(WorkResult::Distribution { result, presentation, .. }) if result.histogram.counts.iter().sum::<u64>() ==20000 && presentation.annotations.len()==13 => {},
            _=> return Err("20000-sample histogram/presentation failed".into()),
        }
        snapshot(state.chart_windows().last().ok_or("No histogram chart")?, output, "08-ws-density", &mut screenshots)?;
    }
    snapshot_result(desktop, output, "08-distribution-summary", &mut screenshots)?;
    compute(desktop, "Compare TP & WS stats").await?;
    snapshot_result(desktop, output, "09-tp-ws-comparison", &mut screenshots)?;
    actual_report_selection(desktop, output, &mut screenshots, &mut checks).await?;
    checks.push(json!({"workflow": "full simulation and comparison", "dps_seconds": 7200, "distribution_samples": 20000, "histogram_bins": 300}));

    desktop.window.invoke_action("about-slint".into());
    pause(60).await;
    if let Some(about) = desktop.controller.borrow().about_window() { snapshot(about, output, "10-about-slint", &mut screenshots)?; }
    let mut resize_baseline = HorizontalBaseline::new();
    for (width,height) in [(760.0,440.0),(720.0,560.0),(1440.0,1080.0),(720.0,440.0)] {
        let scale = desktop.window.window().scale_factor();
        // Winit's LogicalSize uses the monitor DPI, which can differ from an
        // explicit SLINT_SCALE_FACTOR. Request exact rendered client pixels.
        desktop.window.window().set_size(slint::PhysicalSize::new((width * scale).round() as u32, (height * scale).round() as u32));
        pause(100).await;
        let actual = desktop.window.window().size();
        if (actual.width as f32 / scale - width).abs() > 1.0 || (actual.height as f32 / scale - height).abs() > 1.0 {
            return Err(format!("Native window refused {width}x{height}: actual {actual:?}, scale {scale}").into());
        }
        for (i,name) in [(0,"quicklook"),(1,"optimize"),(2,"simulate"),(3,"stats")] {
            tab(desktop,i).await?;
            scroll_content(desktop,10000.0).await?;
            snapshot(&desktop.window,output,&format!("11-resize-{}x{}-{name}",width as u32,height as u32),&mut screenshots)?;
        }
        if width == 760.0 { compact_layout(desktop, output, ninja, &mut screenshots, &mut checks).await?; }
        resize_geometry(desktop, output, width, height, &mut resize_baseline, &mut screenshots, &mut checks).await?;
        native_scrollbars(desktop, output, width, height, &mut screenshots, &mut checks).await?;
    }

    // Busy input/duplicate gating, cancellation recovery, then Close while active.
    tab(desktop,2).await?;
    let cancelled = desktop.controller.borrow().cancelled_jobs;
    desktop.window.invoke_action("distribution".into());
    if !desktop.window.get_busy() { return Err("Worker did not disable inputs".into()); }
    let tp_before = desktop.controller.borrow().ui.text("defaults_tp_entry").to_owned();
    desktop.window.invoke_text_edited("defaults_tp_entry".into(),"2500".into());
    desktop.window.invoke_action("distribution".into());
    if desktop.controller.borrow().ui.text("defaults_tp_entry") != tp_before { return Err("Busy input mutation was accepted".into()); }
    desktop.window.invoke_action("cancel".into());
    while desktop.controller.borrow().is_busy() { pause(16).await; }
    if desktop.controller.borrow().cancelled_jobs != cancelled +1 { return Err("Cancellation did not recover and join worker".into()); }
    desktop.window.invoke_action("distribution".into());
    let closed = Instant::now();
    desktop.window.window().dispatch_event(WindowEvent::CloseRequested);
    if desktop.controller.borrow().is_busy() || desktop.window.window().is_visible() { return Err("Close left native window or worker active".into()); }
    checks.push(json!({"workflow":"busy/cancel/close", "close_join_milliseconds":closed.elapsed().as_millis()}));
    fs::write(output.join("report.json"), serde_json::to_vec_pretty(&json!({
        "status":"passed", "platform": std::env::consts::OS, "scale_factor":desktop.window.window().scale_factor(),
        "screenshots":screenshots, "checks":checks,
        "manual_limits":["Native portal file chooser interaction requires a desktop portal session; cancel/read/matching import controller paths are exercised here."]
    }))?)?;
    println!("Native UI smoke passed");
    Ok(())
}
