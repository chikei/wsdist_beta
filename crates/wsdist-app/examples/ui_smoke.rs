//! Development-only native Winit/software smoke. Never initializes the testing backend.
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::{Duration, Instant};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::{Value, json};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalSize, Model, SharedString};
use wsdist_app::controller::Desktop;
use wsdist_app::exchange::{GearSource, ImportOutcome};
use wsdist_app::jobs::WorkResult;
use wsdist_app::resources::{AppPaths, load_resources_at};
use wsdist_core::buffs::aggregate_buffs;
use wsdist_core::model::{CalcError, Slot};
use wsdist_core::rng::RandomSources;
use wsdist_core::stats::Stats;

#[path = "../../wsdist-core/tests/support/fixture_json.rs"]
mod fixture_json;

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
    let index: Value = serde_json::from_slice(&fs::read(fixtures.join("ui.json"))?)?;
    let ninja = fixture(&fixtures, &index, "ui.profile.10.ninja")?;
    // The original worktree is untouched, even when the workflow saves/reloads.
    fs::write(&paths.defaults, serde_json::to_vec(&ninja["input"]["profile_document"])?)?;
    let desktop = Rc::new(Desktop::with_random(load_resources_at(paths)?, RandomSources::from_seeds(11, 11))?);
    desktop.window.show()?;
    let outcome = Rc::new(RefCell::new(None));
    let completed = Rc::clone(&outcome);
    let app = Rc::clone(&desktop);
    let task = slint::spawn_local(async move {
        let result = workflow(&app, &output, &fixtures, &index, &ninja).await.map_err(|error| error.to_string());
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

fn fixture(root: &Path, index: &Value, id: &str) -> Result<Value> {
    let entry = index["cases"].as_array().ok_or("Invalid UI index")?.iter()
        .find(|entry| entry["id"] == id).ok_or_else(|| format!("Missing fixture {id}"))?;
    fixture_json::read::<Value>(&root.join(entry["path"].as_str().ok_or("Fixture path missing")?), Some(entry["sha256"].as_str().ok_or("Fixture SHA-256 missing")?))
}

fn copy_resources(source: &AppPaths, destination: &AppPaths) -> Result<()> {
    fs::create_dir(&destination.data)?;
    for name in ["gear.json", "buffs.json", "enemies.json", "pup-attachments.json"] {
        fs::copy(source.data.join(name), destination.data.join(name))?;
    }
    fs::copy(&source.item_list, &destination.item_list)?;
    fs::create_dir(&destination.icons)?;
    for entry in fs::read_dir(&source.icons)? {
        let entry = entry?;
        if entry.file_type()?.is_file() { fs::copy(entry.path(), destination.icons.join(entry.file_name()))?; }
    }
    Ok(())
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
    control_key(desktop.window.window(), &(index + 1).to_string());
    pause(80).await;
    if desktop.window.get_active_tab() != index { return Err(format!("Ctrl+{} did not select tab {index}", index + 1).into()); }
    Ok(())
}

fn element(component: &impl ComponentHandle, label: &str, role: AccessibleRole) -> Result<ElementHandle> {
    ElementHandle::find_by_accessible_label(component, label)
        .find(|element| element.accessible_role() == Some(role))
        .ok_or_else(|| format!("Visible accessible {role:?} {label:?} not found").into())
}

async fn click(desktop: &Desktop, label: &str) -> Result<()> {
    let mut found = element(&desktop.window, label, AccessibleRole::Button);
    if found.is_err() {
        // Wheel the tab's outer margin, not an inner candidate/input list.
        // This is a native pointer event; no test-only viewport mutation.
        let viewport_label = match desktop.window.get_active_tab() { 0 => "Quicklook content", 1 => "Optimize content", 2 => "Simulate content", _ => "Stats content" };
        let viewport = element(&desktop.window, viewport_label, AccessibleRole::List)?;
        let position = viewport.absolute_position();
        let size = viewport.size();
        for _ in 0..8 {
            desktop.window.window().dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(position.x + 2.0, position.y + size.height * 0.5),
                delta_x: 0.0, delta_y: -700.0,
            });
            pause(80).await;
            found = element(&desktop.window, label, AccessibleRole::Button);
            if found.is_ok() { break; }
        }
    }
    let button = found?;
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

fn displayed_stats(desktop: &Desktop) -> serde_json::Map<String, Value> {
    let mut values = serde_json::Map::new();
    for model in [desktop.window.get_stats_base(), desktop.window.get_stats_physical(), desktop.window.get_stats_magical(),
        desktop.window.get_stats_multi(), desktop.window.get_stats_haste(), desktop.window.get_stats_defensive(), desktop.window.get_stats_other()] {
        for row in model.iter() { values.insert(row.label.to_string(), Value::String(row.value.to_string())); }
    }
    values
}

fn assert_display(desktop: &Desktop, expected: &Value) -> Result<()> {
    let damage = format!("{} {}", expected["damage_label"].as_str().ok_or("Damage label absent")?, expected["damage"].as_str().ok_or("Damage value absent")?);
    let tp = format!("{} {}", expected["tp_label"].as_str().ok_or("TP label absent")?, expected["tp"].as_str().ok_or("TP value absent")?);
    if desktop.window.get_damage().as_str() != damage || desktop.window.get_tp_return().as_str() != tp {
        return Err(format!("Frozen displayed result mismatch: expected {damage:?} / {tp:?}, actual {:?} / {:?}", desktop.window.get_damage(), desktop.window.get_tp_return()).into());
    }
    Ok(())
}

fn assert_sets(desktop: &Desktop, expected: &Value) -> Result<()> {
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

fn assert_input_spacing(desktop: &Desktop) -> Result<()> {
    let first = element(&desktop.window, "Master level", AccessibleRole::Combobox)?;
    let second = element(&desktop.window, "Main job", AccessibleRole::Combobox)?;
    if first.size().height <= 0.0 || second.absolute_position().y < first.absolute_position().y + first.size().height {
        return Err("Input editors overlap or have no usable height".into());
    }
    Ok(())
}

fn assert_stats(actual: &Stats, expected: &Value, label: &str) -> Result<()> {
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

fn apply_buff_step(desktop: &Desktop, step: &Value) -> Result<()> {
    let key = step["key"].as_str().ok_or("Buff key is missing")?;
    match step["action"].as_str().ok_or("Buff action is missing")? {
        "activate" => desktop.window.invoke_text_edited(key.into(), step["value"].as_str().ok_or("Buff combo value is missing")?.into()),
        "check" => desktop.window.invoke_toggled(key.into(), step["value"].as_bool().ok_or("Buff checkbox value is missing")?),
        "restore_widget" => {
            // The oracle's programmatic widget restore does not activate caches.
            let mut controller = desktop.controller.borrow_mut();
            controller.ui.set_raw(key, &step["value"])?;
            let tp = controller.ui.controls["defaults_tp_entry"].value.clone();
            controller.edit("defaults_tp_entry", tp)?;
        },
        action => return Err(format!("Unsupported captured buff action {action}").into()),
    }
    Ok(())
}

async fn buff_workflows(desktop: &Desktop, output: &Path, fixtures: &Path, index: &Value,
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
        let buff = Desktop::with_random(load_resources_at(paths)?, RandomSources::from_seeds(11, 11))?;
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
        if !tooltip.contains(str_text) || !tooltip.starts_with(name) { return Err(format!("{name}: tooltip lost variant/stat identity").into()); }
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

async fn workflow(desktop: &Desktop, output: &Path, fixtures: &Path, index: &Value, ninja: &Value) -> Result<()> {
    let mut screenshots = Vec::new();
    let mut checks = Vec::new();
    pause(180).await;
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
    for entry in index["cases"].as_array().ok_or("Cases missing")?.iter().filter(|entry| entry["id"].as_str().is_some_and(|id| id.starts_with("ui.profile."))) {
        let document = fixture_json::read::<Value>(&fixtures.join(entry["path"].as_str().ok_or("Profile fixture path")?), Some(entry["sha256"].as_str().ok_or("Profile fixture SHA-256")?))?;
        let job = document["input"]["profile_name"].as_str().ok_or("Profile name")?;
        if job == "default" { continue; } // Default was compared at startup.
        desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), job.into());
        pause(16).await;
        let expected = &document["expected"]["value"]["steps"][0]["state"];
        assert_sets(desktop, &expected["equipped_sets"])?;
        if desktop.controller.borrow().ui.text("defaults_mainjob_combobox") != job { return Err(format!("Profile switch did not retain {job}").into()); }
        assert_input_spacing(desktop)?;
    }
    desktop.window.invoke_text_edited("defaults_mainjob_combobox".into(), "Ninja".into());
    pause(80).await;
    let frozen = &ninja["expected"]["value"];
    assert_sets(desktop, &frozen["steps"][0]["state"]["equipped_sets"])?;
    checks.push(json!({"workflow": "all shipped profiles", "effective_sets": "all 16 slots in Quicklook, TP and WS compared to frozen states"}));

    tab(desktop, 3).await?;
    for (step, label) in [(1, "Quicklook Gear Stats"), (2, "TP Gear Stats"), (3, "WS Gear Stats")] {
        compute(desktop, label).await?;
        let expected = &frozen["steps"][step]["state"]["displayed_stats"];
        if Value::Object(displayed_stats(desktop)) != *expected { return Err(format!("Frozen {label} display differs").into()); }
        snapshot(&desktop.window, output, &format!("02-stats-source-{step}"), &mut screenshots)?;
    }
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
    let relaunched = Desktop::with_random(load_resources_at(paths.clone())?, RandomSources::from_seeds(11,11))?;
    if relaunched.controller.borrow().ui.effective_profile() != saved { return Err("Save/relaunch lost effective widget/gear state".into()); }
    drop(relaunched);
    let original = fs::read(paths.data.join("gear.json"))?;
    let old_catalog = desktop.controller.borrow().ui.engine.catalog.clone();
    fs::write(paths.data.join("gear.json"), b"{broken")?;
    desktop.window.invoke_action("reload-data".into());
    if !std::sync::Arc::ptr_eq(&old_catalog, &desktop.controller.borrow().ui.engine.catalog) { return Err("Failed reload changed catalog".into()); }
    fs::write(paths.data.join("gear.json"), &original)?;
    desktop.window.invoke_action("reload-data".into());
    if std::sync::Arc::ptr_eq(&old_catalog, &desktop.controller.borrow().ui.engine.catalog) { return Err("Successful reload failed to replace catalog".into()); }
    // Edit one existing item's inert stat record; the actual Stats action must
    // observe the change, then restore the original catalog before simulation.
    let mut edited: Value = serde_json::from_slice(&original)?;
    let piece = edited["pieces"].as_array_mut().ok_or("Gear records are missing")?.iter_mut()
        .find(|piece| piece["name2"] == "Heishi Shorinken R15").ok_or("Editable main weapon is missing")?;
    let original_str = piece["stats"]["STR"].as_i64().unwrap_or(0);
    piece["stats"]["STR"] = json!(original_str + 10);
    fs::write(paths.data.join("gear.json"), serde_json::to_vec(&edited)?)?;
    desktop.window.invoke_action("reload-data".into());
    tab(desktop, 3).await?;
    compute(desktop, "Quicklook Gear Stats").await?;
    let expected_str = frozen["steps"][1]["state"]["displayed_stats"]["STR"].as_str().ok_or("Frozen STR is missing")?.parse::<i64>()? + 10;
    if displayed_stats(desktop).get("STR") != Some(&json!(expected_str.to_string())) {
        return Err(format!("Edited main-weapon STR did not reach the displayed result {expected_str}").into());
    }
    snapshot(&desktop.window, output, "05-edited-data-stats", &mut screenshots)?;
    fs::write(paths.data.join("gear.json"), &original)?;
    desktop.window.invoke_action("reload-data".into());
    tab(desktop, 0).await?;
    checks.push(json!({"workflow": "existing-item JSON edit", "item": "Heishi Shorinken R15",
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
    checks.push(json!({"workflow": "full simulation and comparison", "dps_seconds": 7200, "distribution_samples": 20000, "histogram_bins": 300}));

    desktop.window.invoke_action("about-slint".into());
    pause(60).await;
    if let Some(about) = desktop.controller.borrow().about_window() { snapshot(about, output, "10-about-slint", &mut screenshots)?; }
    for (width,height) in [(720.0,560.0),(1440.0,1080.0)] {
        desktop.window.window().set_size(LogicalSize::new(width,height));
        pause(100).await;
        for (i,name) in [(0,"quicklook"),(1,"optimize"),(2,"simulate"),(3,"stats")] {
            tab(desktop,i).await?;
            snapshot(&desktop.window,output,&format!("11-resize-{}x{}-{name}",width as u32,height as u32),&mut screenshots)?;
        }
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
