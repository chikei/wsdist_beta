use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model};
use wsdist_app::controller::Desktop;
use wsdist_app::resources::{Resources, AppPaths};
use wsdist_core::rng::RandomSources;

mod ui_support;

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
