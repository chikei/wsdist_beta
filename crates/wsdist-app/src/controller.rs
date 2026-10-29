use std::cell::RefCell;
use std::error::Error;
use std::path::Path;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};
use slint::{ComponentHandle, SharedString, Timer, TimerMode, VecModel};
use wsdist_core::buffs::parse_integer_text;
use wsdist_core::model::{CalcError, EvaluationContext, Gearset, Job, OptimizeOptions, Slot};
use wsdist_core::rng::RandomSources;

use crate::charts::ChartBuffer;
use crate::desktop_inputs::{EXTRA_INPUTS, INPUT_PRESENTATION};
use crate::desktop_models::{DesktopModels, ExtraControls, extra_controls};
use crate::exchange::{BulkAction, BulkFilters, ClipboardOwner, GearSource, IconProvider, ImportOutcome, import_inventory_file, select_bulk};
use crate::jobs::{AnalyticalKind, GuiAction, JobCompletion, WorkRequest, WorkResult, WorkerCoordinator, WorkerEvent};
use crate::profile_registry::InputKind;
use crate::profiles::{ProfileError, ProfileStore};
use crate::reports::{StatGroups, optimizer_progress, player_stats};
use crate::resources::{AppPaths, Resources};
use crate::state::UiState;
use crate::{AboutWindow, ChartWindow, MainWindow, ProgressWindow, ResultWindow, StatDisplayRow};

type AppResult<T, E = Box<dyn Error>> = Result<T, E>;

/// Event-loop ownership: Slint handles never enter a computation worker.
pub struct Desktop {
    pub window: MainWindow,
    pub controller: Rc<RefCell<DesktopController>>,
    timer: Timer,
}

pub struct DesktopController {
    pub ui: UiState,
    pub extras: ExtraControls,
    pub paths: AppPaths,
    pub profiles: Option<ProfileStore>,
    pub last_result: Option<WorkResult>,
    pub completed_jobs: usize,
    pub cancelled_jobs: usize,
    pub last_error: Option<String>,
    pub status: String,
    window: slint::Weak<MainWindow>,
    self_weak: Weak<RefCell<Self>>,
    models: DesktopModels,
    worker: WorkerCoordinator,
    // A single preallocated idle context replaces the leased context. It never
    // computes; completion swaps the real session back on all terminal paths.
    spare_context: Option<EvaluationContext<RandomSources>>,
    best: Option<Gearset>,
    clipboard: ClipboardOwner,
    results: Vec<ResultWindow>,
    charts: Vec<ChartWindow>,
    about: Option<AboutWindow>,
    progress_window: Option<ProgressWindow>,
    progress_lines: Rc<VecModel<SharedString>>,
    chart_buffer: Option<ChartBuffer>,
    pending_dialog: Option<slint::JoinHandle<()>>,
    closing: bool,
}

impl Desktop {
    pub fn new(resources: Resources) -> AppResult<Self> {
        Self::with_random(resources, RandomSources::from_entropy()?)
    }

    pub fn with_random(resources: Resources, random: RandomSources) -> AppResult<Self> {
        let window = MainWindow::new()?;
        let (profiles, initial_error, restore) = load_profiles(&resources.paths);
        let ui = UiState::new(Arc::clone(&resources.catalog), random, if restore { profiles.as_ref() } else { None });
        let extras = extra_controls();
        let models = DesktopModels::new(&ui, &extras, IconProvider::new(&resources.paths));
        models.bind(&window);
        let spare_context = Some(EvaluationContext::new(resources.catalog, RandomSources::from_seeds(0, 0)));
        let controller = Rc::new_cyclic(|weak| RefCell::new(DesktopController {
            ui, extras, paths: resources.paths, profiles, last_result: None,
            completed_jobs: 0, cancelled_jobs: 0, last_error: initial_error, status: "Ready — calculations run only when requested.".into(),
            window: window.as_weak(), self_weak: weak.clone(), models,
            worker: WorkerCoordinator::new(), spare_context, best: None, clipboard: ClipboardOwner::new(),
            results: Vec::new(), charts: Vec::new(), about: None, progress_window: None,
            progress_lines: Rc::new(VecModel::default()), chart_buffer: None, pending_dialog: None, closing: false,
        }));
        {
            let mut state = controller.borrow_mut();
            match slint::Image::load_from_path(&state.paths.icons.join("23937.png")) {
                Ok(icon) => window.set_app_icon(icon),
                Err(error) => {
                    let message = format!("Cannot load application icon {}: {error}", state.paths.icons.join("23937.png").display());
                    state.ui.messages.push(message);
                }
            }
            if let Some(player) = &state.ui.last_player { state.publish_stats(&player_stats(GearSource::Quicklook, player, &state.ui.engine.catalog)); }
            else { state.publish_stats(&crate::reports::uncomputed_stats()); }
            state.sync(true);
        }
        connect_callbacks(&window, &controller);
        let timer = Timer::default();
        let weak = Rc::downgrade(&controller);
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            if let Some(state) = weak.upgrade() { state.borrow_mut().drain(); }
        });
        Ok(Self { window, controller, timer })
    }

    pub fn run(&self) -> Result<(), slint::PlatformError> { self.window.run() }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.timer.stop();
        self.controller.borrow_mut().shutdown();
    }
}

impl DesktopController {
    pub fn is_busy(&self) -> bool { self.worker.is_busy() || self.pending_dialog.is_some() }
    pub fn best_set(&self) -> Option<Gearset> { self.best }
    pub fn result_windows(&self) -> &[ResultWindow] { &self.results }
    pub fn chart_windows(&self) -> &[ChartWindow] { &self.charts }
    pub fn progress_window(&self) -> Option<&ProgressWindow> { self.progress_window.as_ref() }
    pub fn about_window(&self) -> Option<&AboutWindow> { self.about.as_ref() }

    pub fn edit(&mut self, key: &str, value: Value) -> AppResult<()> {
        self.require_idle("input")?;
        self.clear_error();
        let mut profile_error = None;
        let mut restore_job = true;
        if key == "defaults_mainjob_combobox" {
            // Source job activation re-reads JSON. Failed reads retain the last
            // good document while allowing manual, non-restoring job selection.
            let (profiles, error, exists) = load_profiles(&self.paths);
            restore_job = exists && error.is_none();
            profile_error = error;
            if restore_job { self.profiles = profiles; }
        }
        if let Some(control) = self.extras.get_mut(key) {
            match control.kind {
                InputKind::Checkbox => control.value = Value::Bool(wsdist_core::model::python_truth(&value)),
                InputKind::Combo => {
                    let text = crate::profiles::legacy_text(&value);
                    if !control.choices.contains(&text) { control.choices.push(text.clone()); }
                    control.value = Value::String(text);
                }
                InputKind::Entry | InputKind::Label => control.value = Value::String(crate::profiles::legacy_text(&value)),
            }
            if key == "simulate_tab.plot_dps_checkbox" { self.ui.plot_dps = wsdist_core::model::python_truth(&control.value); }
        } else {
            let profiles = if restore_job { self.profiles.as_ref() } else { None };
            let updates_stats = key == "defaults_mainjob_combobox" &&
                profiles.and_then(|store| store.profile(value.as_str().unwrap_or(""))).is_some_and(|profile| !profile.is_empty());
            self.ui.activate(key, &value, profiles)?;
            if updates_stats && let Some(player) = &self.ui.last_player {
                self.publish_stats(&player_stats(GearSource::Quicklook, player, &self.ui.engine.catalog));
            }
        }
        self.sync(true);
        if let Some(error) = profile_error { return Err(error.into()); }
        Ok(())
    }

    pub fn commit_entry(&mut self, key: &str, text: &str) -> AppResult<()> {
        self.require_idle("input.commit")?;
        if key != "defaults_tp_entry" { return Ok(()); }
        if self.ui.text(key) != text { self.ui.set_raw(key, &Value::String(text.into()))?; }
        self.ui.commit_tp_entry();
        self.sync(false);
        Ok(())
    }

    pub fn wheel(&mut self, key: &str, direction: i32) -> AppResult<()> {
        self.require_idle("input.wheel")?;
        if key == "defaults_tp_entry" {
            self.ui.wheel_tp(direction);
            self.sync(false);
            return Ok(());
        }
        let presentation = INPUT_PRESENTATION.iter().chain(EXTRA_INPUTS.iter().map(|spec| &spec.presentation)).find(|spec| spec.key == key)
            .ok_or_else(|| input_error("key", format!("Unknown wheel input {key:?}")))?;
        let Some((low, high, step)) = presentation.wheel else { return Ok(()); };
        let text = self.control_text(key);
        // WheelIntLineEdit.value() treats invalid intermediate text as zero.
        let current = parse_integer_text(text, presentation.key).unwrap_or(0.0);
        let value = (current + (direction.signum() as f64) * step as f64).clamp(low as f64, high as f64) as i64;
        self.edit(key, Value::String(value.to_string()))
    }

    pub fn select_slot(&mut self, source: &str, index: i32) -> AppResult<()> {
        self.require_idle("gear.slot")?;
        let slot = parse_slot(index)?;
        if source == "optimize" { self.ui.gear.optimizer_slot = slot; }
        else { self.ui.gear.visible_slots[parse_source(source)?.index()] = slot; }
        self.sync(true);
        Ok(())
    }

    pub fn select_item(&mut self, source: &str, index: i32) -> AppResult<()> {
        self.require_idle("gear.item")?;
        let source = parse_source(source)?;
        let id = self.ui.engine.catalog.pieces().get(usize::try_from(index).map_err(|_| input_error("gear", "Invalid gear identifier"))?)
            .map(|piece| piece.id).ok_or_else(|| input_error("gear", "Invalid gear identifier"))?;
        let slot = self.ui.gear.visible_slots[source.index()];
        self.ui.select_gear(source, slot, id)?;
        self.clear_error();
        self.sync(true);
        Ok(())
    }

    pub fn toggle_item(&mut self, index: i32, checked: bool) -> AppResult<()> {
        self.require_idle("gear.candidate")?;
        let id = self.ui.engine.catalog.pieces().get(usize::try_from(index).map_err(|_| input_error("gear", "Invalid candidate identifier"))?)
            .map(|piece| piece.id).ok_or_else(|| input_error("gear", "Invalid candidate identifier"))?;
        self.ui.gear.candidates[self.ui.gear.optimizer_slot.index()].set_checked(id, checked);
        self.sync(true);
        Ok(())
    }

    pub fn copy(&mut self, source: &str, destination: &str) -> AppResult<()> {
        self.require_idle("gear.copy")?;
        let source = parse_source(source)?;
        let destination = parse_source(destination)?;
        self.ui.copy_set(source, destination);
        if let Some(window) = self.window.upgrade() { window.set_active_tab(self.ui.active_tab as i32); }
        self.clear_error();
        self.status = format!("Copied {source} equipment to {destination}.");
        self.sync(true);
        Ok(())
    }

    pub fn setting(&mut self, key: &str, value: bool) -> AppResult<()> {
        self.require_idle("settings")?;
        match key {
            "damage-limit" => self.ui.damage_limit = value,
            "verbose-swaps" => self.ui.verbose_swaps = value,
            _ => return Err(input_error("setting", format!("Unknown setting {key:?}")).into()),
        }
        self.sync(false);
        Ok(())
    }

    pub fn action(&mut self, name: &str) -> AppResult<()> {
        match name {
            "cancel" => {
                self.worker.cancel();
                if let Some(dialog) = self.pending_dialog.take() { dialog.abort(); }
                self.status = "Cancellation requested; waiting for the current safe point.".into();
                self.sync(false);
                return Ok(());
            }
            "show-error" => {
                if let Some(text) = &self.last_error { self.show_report("Application error", text.as_str().into())?; }
                return Ok(());
            }
            "about-slint" => return self.show_about(),
            _ => self.require_idle("action")?,
        }
        self.clear_error();
        match name {
            "save-defaults" => {
                let job = self.ui.text("defaults_mainjob_combobox").to_owned();
                let profile = self.ui.effective_profile();
                match ProfileStore::load(&self.paths.defaults) {
                    Ok(mut store) => {
                        store.save(&self.paths.defaults, &job, &profile)?;
                        self.profiles = Some(store);
                    }
                    Err(ProfileError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                        if self.profiles.is_none() { self.profiles = load_profiles(&self.paths).0; }
                        self.profiles.as_mut().ok_or_else(|| input_error("defaults.json", "Unable to initialize an inert profile document."))?
                            .save(&self.paths.defaults, &job, &profile)?;
                    }
                    Err(error) => return Err(error.into()),
                }
                self.status = format!("Saved default and {job} profiles atomically to {}.", self.paths.defaults.display());
                self.sync(false);
            }
            "reload-data" => {
                let report = self.ui.reload_data(&self.paths)?;
                self.models.reset_catalog();
                self.best = None;
                self.last_result = None;
                let spare = self.spare_context.as_mut().expect("idle owns spare context");
                spare.catalog = Arc::clone(&self.ui.engine.catalog);
                spare.state.reset(spare.catalog.pieces().len());
                self.status = report.to_string();
                self.show_report("Reload Data", self.status.as_str().into())?;
                self.sync(true);
            }
            "bulk-all" | "bulk-slot" | "bulk-unselect" => {
                let action = match name { "bulk-all" => BulkAction::SelectAll, "bulk-slot" => BulkAction::SelectSlot(self.ui.gear.optimizer_slot), _ => BulkAction::UnselectSlot(self.ui.gear.optimizer_slot) };
                let selected = select_bulk(&self.ui.engine.catalog, &self.ui.gear.visible_candidates(), &self.ui.gear.candidate_pools(), &self.bulk_filters(), action);
                self.ui.gear.replace_candidates(&selected);
                self.status = "Candidate selection updated.".into();
                self.sync(true);
            }
            "inventory-import" => self.begin_import()?,
            "export-quicklook" | "export-tp" | "export-ws" => {
                let source = match name { "export-quicklook" => GearSource::Quicklook, "export-tp" => GearSource::Tp, _ => GearSource::Ws };
                self.clipboard.copy_export(&self.ui.engine.catalog, &self.ui.gear.equipped, source)?;
                self.status = format!("Copied the distinct {source} set to the clipboard; ownership retained.");
                self.sync(false);
            }
            "equip-best" => {
                let best = self.best.ok_or_else(|| input_error("best set", "No completed optimizer result contains an actual player."))?;
                self.ui.equip_best_set(best);
                if let Some(window) = self.window.upgrade() { window.set_active_tab(0); }
                self.status = "Equipped the completed best set into Quicklook.".into();
                self.sync(true);
            }
            "quick-round" | "quick-ws" | "quick-spell" | "single-ws" | "optimize-round" | "optimize-ws" | "optimize-spell" |
            "dps" | "distribution" | "compare" | "stats-quicklook" | "stats-tp" | "stats-ws" => {
                let request = self.prepare_request(name)?;
                if name.starts_with("optimize-") { self.open_progress()?; }
                let placeholder = self.spare_context.take().expect("idle owns spare context");
                let context = std::mem::replace(&mut self.ui.engine, placeholder);
                if let Err(failure) = self.worker.start(request, context) {
                    self.spare_context = Some(std::mem::replace(&mut self.ui.engine, failure.context));
                    return Err(failure.error.into());
                }
                self.status = format!("Running {name} — inputs frozen; tabs, results and Close remain available.");
                self.sync(false);
            }
            _ => return Err(input_error("action", format!("Unknown action {name:?}")).into()),
        }
        Ok(())
    }

    fn prepare_request(&self, name: &str) -> AppResult<WorkRequest> {
        let quicklook = self.ui.calculation_inputs(GearSource::Quicklook)?;
        let target = |source: GearSource| {
            let mut input = quicklook.player.clone();
            input.gearset = self.ui.gear.equipped[source.index()];
            input
        };
        let action = match name {
            "quick-round" => GuiAction::Round,
            "quick-ws" => GuiAction::Ws,
            "quick-spell" => GuiAction::Spell,
            "single-ws" => GuiAction::SingleWs,
            "optimize-round" | "optimize-ws" | "optimize-spell" => {
                let kind = match name { "optimize-round" => AnalyticalKind::Round, "optimize-ws" => AnalyticalKind::Ws, _ => AnalyticalKind::Spell };
                let metric_key = match kind { AnalyticalKind::Round => "optimize_tab.tp_metric_combobox", AnalyticalKind::Ws => "optimize_tab.ws_metric_combobox", AnalyticalKind::Spell => "optimize_tab.spell_metric_combobox" };
                GuiAction::Optimize {
                    kind, metric: Arc::from(self.control_text(metric_key)), candidates: self.ui.gear.candidate_pools(),
                    min_pdt: self.wheel_integer("optimize_tab.pdt_requirements_entry")?, min_mdt: self.wheel_integer("optimize_tab.mdt_requirements_entry")?,
                    options: self.optimizer_options()?, similar_results: self.extra_checked("optimize_tab.show_similar_results_checkbox"),
                    similar_percent: self.integer("optimize_tab.show_similar_results_entry")?, verbose_swaps: self.ui.verbose_swaps,
                }
            }
            "dps" => GuiAction::Dps { tp: target(GearSource::Tp), ws: target(GearSource::Ws), plot: self.ui.plot_dps },
            "distribution" => GuiAction::Distribution { ws: target(GearSource::Ws) },
            "compare" => GuiAction::Compare { tp: target(GearSource::Tp), ws: target(GearSource::Ws) },
            "stats-quicklook" | "stats-tp" | "stats-ws" => {
                let source = match name { "stats-quicklook" => GearSource::Quicklook, "stats-tp" => GearSource::Tp, _ => GearSource::Ws };
                GuiAction::Stats { source, input: target(source) }
            }
            _ => return Err(input_error("action", format!("Unknown computation {name:?}")).into()),
        };
        Ok(WorkRequest { quicklook, action })
    }

    fn optimizer_options(&self) -> AppResult<OptimizeOptions> {
        let integer = |key| to_i64(self.wheel_integer(key)?, key);
        let seed_text = self.control_text("defaults_optimizer_seed_entry").trim();
        let seed = if seed_text.is_empty() { None } else {
            let value = parse_integer_text(seed_text, "defaults_optimizer_seed_entry")?;
            if !(0.0..=2_000_000_000.0).contains(&value) { return Err(input_error("defaults_optimizer_seed_entry", "Random seed must be blank or 0–2000000000.").into()); }
            Some(value as u32)
        };
        Ok(OptimizeOptions {
            iterations: integer("defaults_optimizer_iterations_entry")?,
            max_swap_slots: to_i64(self.integer("defaults_optimizer_swap_slots_combobox")?, "defaults_optimizer_swap_slots_combobox")?,
            restart_count: integer("defaults_optimizer_restart_count_entry")?, seed,
            dt_step: self.wheel_integer("defaults_optimizer_dt_step_entry")?, workers: integer("defaults_optimizer_workers_entry")?,
        })
    }

    fn control_text(&self, key: &str) -> &str {
        self.extras.get(key).and_then(|control| control.value.as_str()).unwrap_or_else(|| self.ui.text(key))
    }
    fn extra_checked(&self, key: &str) -> bool { self.extras.get(key).is_some_and(|control| wsdist_core::model::python_truth(&control.value)) }
    fn integer(&self, key: &'static str) -> Result<f64, CalcError> {
        let text = self.control_text(key);
        parse_integer_text(if text.is_empty() { "0" } else { text }, key)
    }
    fn wheel_integer(&self, key: &'static str) -> Result<f64, CalcError> {
        match self.integer(key) {
            Err(CalcError::Legacy { category: "ValueError", .. }) => Ok(0.0),
            result => result,
        }
    }
    fn bulk_filters(&self) -> BulkFilters {
        BulkFilters {
            odyssey_rank: self.ui.text("defaults_ody_rank_combobox").into(), soa_ring: self.ui.text("defaults_soa_ring_combobox").into(),
            tvr_ring: self.ui.text("defaults_tvr_ring_combobox").into(), mastery_rank: self.ui.text("defaults_mastery_rank_combobox").into(),
            nyame_r25_b: self.ui.checked("defaults_nyame25_checkbox"),
        }
    }

    pub fn import_inventory(&mut self, path: Option<&Path>) -> AppResult<ImportOutcome> {
        self.require_idle("inventory.import")?;
        let mut selected = self.ui.gear.candidate_pools();
        let outcome = import_inventory_file(&self.ui.engine.catalog, &self.ui.gear.visible_candidates(), &self.bulk_filters(), &mut selected, path)?;
        if outcome == ImportOutcome::Completed { self.ui.gear.replace_candidates(&selected); self.status = "Imported inventory candidate selection.".into(); }
        else { self.status = "Inventory import cancelled; selections retained.".into(); }
        self.sync(true);
        Ok(outcome)
    }

    fn begin_import(&mut self) -> AppResult<()> {
        let weak = self.self_weak.clone();
        let root = self.paths.root.clone();
        let task = slint::spawn_local(async move {
            let chosen = rfd::AsyncFileDialog::new().set_title("Import inventory candidates").set_directory(root).add_filter("Inventory export", &["lua", "txt", "csv"]).pick_file().await;
            if let Some(state) = weak.upgrade() {
                let mut state = state.borrow_mut();
                state.pending_dialog = None;
                if state.closing { return; }
                if let Err(error) = state.import_inventory(chosen.as_ref().map(|file| file.path())) { state.record_error(error); }
            }
        })?;
        self.pending_dialog = Some(task);
        self.status = "Choose an inventory export — cancelling retains every check.".into();
        self.sync(false);
        Ok(())
    }

    pub fn drain(&mut self) {
        if self.closing { return; }
        if let Some(window) = self.window.upgrade() { self.ui.active_tab = window.get_active_tab().max(0) as usize; }
        let mut progress_changed = false;
        while let Some(event) = self.worker.poll() {
            match event {
                WorkerEvent::Progress(progress) => {
                    let report = optimizer_progress(&progress);
                    if let Err(error) = report.mirror_stdout() { self.record_error(error); }
                    self.status = report.text.clone();
                    self.progress_lines.push(report.text.into());
                    progress_changed = true;
                }
                WorkerEvent::Finished(completion) => self.complete(completion),
            }
        }
        self.results.retain(|window| window.window().is_visible());
        self.charts.retain(|window| window.window().is_visible());
        if progress_changed { self.sync(false); }
    }

    fn complete(&mut self, completion: JobCompletion) {
        self.spare_context = Some(std::mem::replace(&mut self.ui.engine, completion.context));
        if let Some(progress) = self.progress_window.take() {
            if let Err(error) = progress.hide() { self.record_error(error); }
        }
        match completion.outcome {
            Ok(result) => {
                self.completed_jobs += 1;
                if let WorkResult::Analytical { display, .. } = &result && let Some(window) = self.window.upgrade() {
                    window.set_damage(format!("{} {}", display.damage_label, display.damage).into());
                    window.set_tp_return(format!("{} {}", display.tp_label, display.tp_return).into());
                    window.set_time_to_ws(if display.time_to_ws.is_empty() { "".into() } else { format!("Time to WS: {}", display.time_to_ws).into() });
                }
                if let WorkResult::Optimize { result, .. } = &result {
                    self.best = result.player.as_ref().map(|player| player.gearset);
                    if !result.feasible { self.last_error = Some("Optimizer returned no set satisfying the requested PDT/MDT constraints. Review the returned feasibility and warnings.".into()); }
                }
                if let WorkResult::Stats { groups, .. } = &result { self.publish_stats(groups); }
                let report = match &result {
                    WorkResult::Analytical { report, .. } | WorkResult::SingleWs { report, .. } | WorkResult::Optimize { report, .. } |
                    WorkResult::Dps { report, .. } | WorkResult::Distribution { report, .. } | WorkResult::Stats { report, .. } | WorkResult::Compare { report, .. } => report,
                };
                self.status = format!("Finished {}.", report.title);
                if let Err(error) = report.mirror_stdout() { self.record_error(error); }
                if let Err(error) = self.show_report(&report.title, report.text.as_str().into()) { self.record_error(error); }
                if let Err(error) = self.show_result_chart(&result) { self.record_error(error); }
                self.last_result = Some(result);
            }
            Err(error) if error.is_cancelled() => {
                self.cancelled_jobs += 1;
                self.status = "Computation cancelled and worker joined; no partial result was installed.".into();
            }
            Err(error) => {
                self.record_error(error);
                let text = self.last_error.as_deref().expect("worker error was recorded").into();
                if let Err(error) = self.show_report("Computation failed", text) { self.record_error(error); }
            }
        }
        self.sync(true);
    }

    fn show_result_chart(&mut self, result: &WorkResult) -> AppResult<()> {
        if !matches!(result, WorkResult::Dps { plot: true, .. } | WorkResult::Distribution { .. }) { return Ok(()); }
        // Old chart windows retain their raster. Allocate a fresh drawing
        // surface rather than COW-copying pixels that will be cleared anyway.
        if self.chart_buffer.is_none() || !self.charts.is_empty() { self.chart_buffer = Some(ChartBuffer::new(1280, 800)?); }
        let buffer = self.chart_buffer.as_mut().expect("chart buffer initialized");
        let (title, image) = match result {
            WorkResult::Dps { result, .. } => ("DPS simulation".to_owned(), buffer.render_dps(result)?),
            WorkResult::Distribution { result, presentation, .. } => (presentation.title.join("\n"), buffer.render_distribution(result, presentation)?),
            _ => return Ok(()),
        };
        let window = ChartWindow::new()?;
        window.set_chart_title(title.into());
        window.set_chart_image(image);
        hide_on_close(&window);
        window.show()?;
        self.charts.push(window);
        Ok(())
    }

    fn publish_stats(&self, groups: &StatGroups) {
        let Some(window) = self.window.upgrade() else { return; };
        let rows = |values: &[crate::reports::StatDisplay]| -> slint::ModelRc<StatDisplayRow> {
            Rc::new(VecModel::from(values.iter().map(|row| StatDisplayRow { label: row.label.as_str().into(), value: row.value.as_str().into() }).collect::<Vec<_>>())).into()
        };
        window.set_stats_source(groups.source_label.as_str().into());
        window.set_stats_base(rows(&groups.base)); window.set_stats_physical(rows(&groups.physical));
        window.set_stats_magical(rows(&groups.magical)); window.set_stats_multi(rows(&groups.multi));
        window.set_stats_haste(rows(&groups.haste)); window.set_stats_defensive(rows(&groups.defensive)); window.set_stats_other(rows(&groups.other));
    }

    fn show_report(&mut self, title: &str, text: SharedString) -> AppResult<()> {
        let window = ResultWindow::new()?;
        window.set_report_title(title.into());
        window.set_report_text(text);
        hide_on_close(&window);
        window.show()?;
        self.results.push(window);
        Ok(())
    }

    fn open_progress(&mut self) -> AppResult<()> {
        if let Some(old) = self.progress_window.take() { old.hide()?; }
        self.progress_lines = Rc::new(VecModel::default());
        let window = ProgressWindow::new()?;
        window.set_lines(self.progress_lines.clone().into());
        hide_on_close(&window);
        window.show()?;
        self.progress_window = Some(window);
        Ok(())
    }

    fn show_about(&mut self) -> AppResult<()> {
        if self.about.is_none() {
            let window = AboutWindow::new()?;
            hide_on_close(&window);
            self.about = Some(window);
        }
        self.about.as_ref().expect("About window initialized").show()?;
        Ok(())
    }

    pub fn record_error(&mut self, error: impl std::fmt::Display) {
        let message = error.to_string();
        eprintln!("{message}");
        self.status = "Action failed; current inputs and any previously completed results are retained.".into();
        self.last_error = Some(message);
        self.sync(false);
    }
    fn clear_error(&mut self) { self.last_error = None; }
    fn require_idle(&self, operation: &'static str) -> Result<(), CalcError> {
        if self.closing || self.is_busy() { return Err(CalcError::Input { operation, field: "busy", message: "A computation or inventory dialog is already active.".into() }); }
        Ok(())
    }

    fn sync(&mut self, gear: bool) {
        let Some(window) = self.window.upgrade() else { return; };
        let busy = self.is_busy();
        self.models.sync_controls(&self.ui, &self.extras, busy);
        if gear && !self.worker.is_busy() { self.models.sync_gear(&self.ui, &window); }
        let warnings = self.models.icons.take_warnings();
        self.ui.messages.extend(warnings.into_iter().map(|warning| warning.to_string()));
        if !self.ui.messages.is_empty() {
            let messages = std::mem::take(&mut self.ui.messages).join("\n");
            if let Some(old) = &mut self.last_error { old.push('\n'); old.push_str(&messages); } else { self.last_error = Some(messages); }
        }
        window.set_busy(busy);
        window.set_best_ready(self.best.is_some());
        window.set_damage_limit(self.ui.damage_limit);
        window.set_verbose_swaps(self.ui.verbose_swaps);
        window.set_sublevel(self.ui.sub_job_level.to_string().into());
        window.set_skill_label(self.ui.enhancing_skill_label.as_str().into());
        if window.get_status().as_str() != self.status { window.set_status(self.status.as_str().into()); }
        let error = self.last_error.as_deref().unwrap_or("");
        if window.get_error_message().as_str() != error { window.set_error_message(error.into()); }
    }

    pub fn shutdown(&mut self) {
        if self.closing { return; }
        self.closing = true;
        if let Some(dialog) = self.pending_dialog.take() { dialog.abort(); }
        if let Some(completion) = self.worker.shutdown() {
            self.spare_context = Some(std::mem::replace(&mut self.ui.engine, completion.context));
            if let Err(error) = completion.outcome && !error.is_cancelled() { eprintln!("Worker shutdown: {error}"); }
        }
        for window in &self.results { if let Err(error) = window.hide() { eprintln!("Close result: {error}"); } }
        for window in &self.charts { if let Err(error) = window.hide() { eprintln!("Close chart: {error}"); } }
        if let Some(window) = &self.about && let Err(error) = window.hide() { eprintln!("Close About: {error}"); }
        if let Some(window) = &self.progress_window && let Err(error) = window.hide() { eprintln!("Close progress: {error}"); }
        if let Some(window) = self.window.upgrade() && let Err(error) = window.hide() { eprintln!("Close main window: {error}"); }
    }
}

fn load_profiles(paths: &AppPaths) -> (Option<ProfileStore>, Option<String>, bool) {
    match std::fs::metadata(&paths.defaults) {
        Ok(_) => match ProfileStore::load(&paths.defaults) { Ok(store) => (Some(store), None, true), Err(error) => (None, Some(error.to_string()), false) },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut document = Map::new();
            document.insert("default".into(), Value::Object(Map::new()));
            for job in Job::ALL { document.insert(job.display_name().into(), Value::Object(Map::new())); }
            match ProfileStore::from_document(document) { Ok(store) => (Some(store), None, false), Err(error) => (None, Some(error.to_string()), false) }
        }
        Err(error) => (None, Some(format!("Cannot inspect profiles at {}: {error}", paths.defaults.display())), false),
    }
}

fn input_error(field: &'static str, message: impl Into<String>) -> CalcError { CalcError::Input { operation: "desktop", field, message: message.into() } }
fn parse_source(source: &str) -> Result<GearSource, CalcError> {
    match source { "quicklook" => Ok(GearSource::Quicklook), "tp" => Ok(GearSource::Tp), "ws" => Ok(GearSource::Ws), _ => Err(input_error("source", format!("Unknown gear source {source:?}"))) }
}
fn parse_slot(index: i32) -> Result<Slot, CalcError> { usize::try_from(index).ok().and_then(|index| Slot::ALL.get(index).copied()).ok_or_else(|| input_error("slot", "Invalid equipment slot")) }
fn to_i64(value: f64, field: &'static str) -> Result<i64, CalcError> {
    if !value.is_finite() || value < i64::MIN as f64 || value >= 9_223_372_036_854_775_808.0 { Err(input_error(field, "Integer is outside the supported range")) } else { Ok(value as i64) }
}

trait Closable: ComponentHandle { fn on_hide_requested(&self, callback: impl Fn() + 'static); }
impl Closable for ResultWindow { fn on_hide_requested(&self, callback: impl Fn() + 'static) { self.on_close_requested(callback); } }
impl Closable for ChartWindow { fn on_hide_requested(&self, callback: impl Fn() + 'static) { self.on_close_requested(callback); } }
impl Closable for AboutWindow { fn on_hide_requested(&self, callback: impl Fn() + 'static) { self.on_close_requested(callback); } }
impl Closable for ProgressWindow { fn on_hide_requested(&self, callback: impl Fn() + 'static) { self.on_close_requested(callback); } }
fn hide_on_close<T: Closable + 'static>(window: &T) {
    let weak = window.as_weak();
    window.on_hide_requested(move || { if let Some(window) = weak.upgrade() && let Err(error) = window.hide() { eprintln!("Close window: {error}"); } });
}

fn connect_callbacks(window: &MainWindow, controller: &Rc<RefCell<DesktopController>>) {
    let weak = Rc::downgrade(controller);
    window.on_text_edited(move |key, value| { dispatch(&weak, |state| state.edit(&key, Value::String(value.to_string()))); });
    let weak = Rc::downgrade(controller);
    window.on_editing_finished(move |key, value| { dispatch(&weak, |state| state.commit_entry(&key, &value)); });
    let weak = Rc::downgrade(controller);
    window.on_toggled(move |key, value| { dispatch(&weak, |state| state.edit(&key, Value::Bool(value))); });
    let weak = Rc::downgrade(controller);
    window.on_wheel(move |key, direction| { dispatch(&weak, |state| state.wheel(&key, direction)); });
    let weak = Rc::downgrade(controller);
    window.on_slot_selected(move |source, slot| { dispatch(&weak, |state| state.select_slot(&source, slot)); });
    let weak = Rc::downgrade(controller);
    window.on_item_selected(move |source, id| { dispatch(&weak, |state| state.select_item(&source, id)); });
    let weak = Rc::downgrade(controller);
    window.on_item_toggled(move |id, checked| { dispatch(&weak, |state| state.toggle_item(id, checked)); });
    let weak = Rc::downgrade(controller);
    window.on_copy(move |source, destination| { dispatch(&weak, |state| state.copy(&source, &destination)); });
    let weak = Rc::downgrade(controller);
    window.on_setting_changed(move |key, value| { dispatch(&weak, |state| state.setting(&key, value)); });
    let weak = Rc::downgrade(controller);
    window.on_action(move |name| { dispatch(&weak, |state| state.action(&name)); });
    let weak = Rc::downgrade(controller);
    window.on_close_requested(move || { if let Some(state) = weak.upgrade() { state.borrow_mut().shutdown(); } });
    let weak = Rc::downgrade(controller);
    window.window().on_close_requested(move || {
        if let Some(state) = weak.upgrade() { state.borrow_mut().shutdown(); }
        slint::CloseRequestResponse::HideWindow
    });
}
fn dispatch(controller: &Weak<RefCell<DesktopController>>, action: impl FnOnce(&mut DesktopController) -> AppResult<()>) {
    if let Some(state) = controller.upgrade() {
        let mut state = state.borrow_mut();
        if let Some(window) = state.window.upgrade() { state.ui.active_tab = window.get_active_tab().max(0) as usize; }
        if let Err(error) = action(&mut state) { state.record_error(error); }
    }
}
