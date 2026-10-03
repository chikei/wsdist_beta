//! A single owned computation context, isolated from the Slint event-loop thread.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread::{self, JoinHandle};

use wsdist_core::actions::{evaluate_action, gui_spell_type, resolve_spell_name, sample_ws};
use wsdist_core::model::{
    ActionRequest, ActionResult, CalcError, CandidatePools, DistributionRequest, DistributionResult, DpsRequest,
    DpsResult, EvaluationContext, HitEvent, Metric, OptimizeOptions, OptimizeRequest, OptimizeResult, Player,
    PlayerInput, PlayerStage, Progress, SpellRequest, WsRequest, WsSample, build_player,
};
use wsdist_core::optimizer::optimize;
use wsdist_core::rng::RandomSources;
use wsdist_core::simulation::{sample_distribution, simulate_dps};
use wsdist_core::weaponskills::resolve_name;

use crate::exchange::GearSource;
use crate::reports::{
    self, AnalyticalDisplay, ComparisonOutputs, HistogramPresentation, StatGroups, TextReport,
};
use crate::state::CalculationInputs;

type Engine = EvaluationContext<RandomSources>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticalKind {
    Round,
    Ws,
    Spell,
}

pub enum GuiAction {
    Round,
    Ws,
    Spell,
    SingleWs,
    Optimize {
        kind: AnalyticalKind,
        metric: Arc<str>,
        candidates: CandidatePools,
        min_pdt: f64,
        min_mdt: f64,
        options: OptimizeOptions,
        similar_results: bool,
        similar_percent: f64,
        verbose_swaps: bool,
    },
    Dps { tp: PlayerInput, ws: PlayerInput, plot: bool },
    Distribution { ws: PlayerInput },
    Stats { source: GearSource, input: PlayerInput },
    Compare { tp: PlayerInput, ws: PlayerInput },
}

impl GuiAction {
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::Round => "Quicklook TP", Self::Ws => "Quicklook WS",
            Self::Spell => "Quicklook Spell", Self::SingleWs => "Simulate WS",
            Self::Optimize { .. } => "Optimize", Self::Dps { .. } => "DPS simulation",
            Self::Distribution { .. } => "WS distribution",
            Self::Stats { .. } => "Player Stats", Self::Compare { .. } => "Compare TP & WS stats",
        }
    }
}

/// One production path: every job retains the source Quicklook-build prelude.
pub struct WorkRequest {
    pub quicklook: CalculationInputs,
    pub action: GuiAction,
}

impl WorkRequest {
    pub const fn operation(&self) -> &'static str { self.action.operation() }
}

pub enum WorkResult {
    Analytical {
        kind: AnalyticalKind,
        player: Player,
        result: ActionResult,
        display: AnalyticalDisplay,
        report: TextReport,
    },
    SingleWs { player: Player, sample: WsSample, events: Vec<HitEvent>, report: TextReport },
    Optimize { result: OptimizeResult, report: TextReport },
    Dps { result: DpsResult, plot: bool, report: TextReport },
    Distribution { result: DistributionResult, presentation: HistogramPresentation, report: TextReport },
    Stats { source: GearSource, player: Player, groups: StatGroups, report: TextReport },
    Compare { tp_player: Player, ws_player: Player, outputs: ComparisonOutputs, report: TextReport },
}

impl WorkResult {
    pub fn report(&self) -> &TextReport {
        match self {
            Self::Analytical { report, .. } | Self::SingleWs { report, .. }
            | Self::Optimize { report, .. } | Self::Dps { report, .. }
            | Self::Distribution { report, .. } | Self::Stats { report, .. }
            | Self::Compare { report, .. } => report,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("A computation is already running")]
    Busy,
    #[error("{operation}: unable to start worker: {message}")]
    Spawn { operation: &'static str, message: String },
    #[error(transparent)]
    Calculation(#[from] CalcError),
    #[error("{operation} cancelled")]
    Cancelled { operation: &'static str },
    #[error("{operation}: worker panicked: {message}")]
    Panic { operation: &'static str, message: String },
    #[error("{operation}: worker disconnected without returning a result")]
    Disconnected { operation: &'static str },
}

impl WorkerError {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled { .. } | Self::Calculation(CalcError::Interrupted { .. }))
    }
}

/// Starting never drops the caller's context, including thread-spawn failure.
pub struct StartFailure {
    pub context: Engine,
    pub error: WorkerError,
}

impl std::fmt::Debug for StartFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("StartFailure").field("error", &self.error).finish_non_exhaustive()
    }
}

pub struct JobCompletion {
    pub context: Engine,
    pub outcome: Result<WorkResult, WorkerError>,
}

pub enum WorkerEvent {
    Progress(Progress),
    Finished(JobCompletion),
}

enum Message {
    Progress(Progress),
    Finished(Result<WorkResult, WorkerError>),
}

struct ActiveJob {
    operation: &'static str,
    stop: Arc<AtomicBool>,
    context: Arc<Mutex<Engine>>,
    receiver: mpsc::Receiver<Message>,
    handle: JoinHandle<()>,
    outcome: Option<Result<WorkResult, WorkerError>>,
    disconnected: bool,
}

#[derive(Default)]
pub struct WorkerCoordinator {
    active: Option<ActiveJob>,
}

impl WorkerCoordinator {
    pub fn new() -> Self { Self::default() }

    pub fn is_busy(&self) -> bool { self.active.is_some() }

    pub fn start(&mut self, request: WorkRequest, context: Engine) -> Result<(), StartFailure> {
        if self.is_busy() {
            return Err(StartFailure { context, error: WorkerError::Busy });
        }
        let operation = request.operation();
        let context = Arc::new(Mutex::new(context));
        let worker_context = Arc::clone(&context);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (sender, receiver) = mpsc::channel();
        let handle = thread::Builder::new().name(format!("wsdist-{operation}")).spawn(move || {
            // The mutex is only a context-ownership mailbox. The UI never locks
            // it while the worker is active. Keeping the context in the guard
            // also recovers model/RNG state if the thread unwinds unexpectedly.
            let outcome = {
                let mut context = lock_context(&worker_context);
                let saved_trace = context.action_trace.take();
                let saved_stages = context.player_stages.take();
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    execute(request, &mut context, &worker_stop, &sender)
                }));
                context.action_trace = saved_trace;
                context.player_stages = saved_stages;
                match outcome {
                    Ok(Ok(result)) if worker_stop.load(Ordering::Acquire) => {
                        drop(result);
                        Err(WorkerError::Cancelled { operation })
                    }
                    Ok(outcome) => outcome,
                    Err(payload) => Err(WorkerError::Panic { operation, message: panic_message(payload) }),
                }
            };
            // An unbounded std channel send never waits for the UI. No weak
            // component, invoke_from_event_loop, or synchronous UI call exists.
            let _ = sender.send(Message::Finished(outcome));
        });
        match handle {
            Ok(handle) => {
                self.active = Some(ActiveJob { operation, stop, context, receiver, handle,
                    outcome: None, disconnected: false });
                Ok(())
            }
            Err(error) => Err(StartFailure { context: recover_context(context),
                error: WorkerError::Spawn { operation, message: error.to_string() } }),
        }
    }

    pub fn cancel(&self) {
        if let Some(active) = &self.active { active.stop.store(true, Ordering::Release); }
    }

    /// Return at most one message. Even a received terminal result waits for
    /// is_finished before joining, so the Slint timer never blocks on a worker.
    pub fn poll(&mut self) -> Option<WorkerEvent> {
        let active = self.active.as_mut()?;
        if active.outcome.is_none() && !active.disconnected {
            match active.receiver.try_recv() {
                Ok(Message::Progress(progress)) => return Some(WorkerEvent::Progress(progress)),
                Ok(Message::Finished(outcome)) => active.outcome = Some(outcome),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => active.disconnected = true,
            }
        }
        if !active.handle.is_finished() { return None; }
        let active = self.active.take().expect("active worker just observed");
        Some(WorkerEvent::Finished(finish(active)))
    }

    /// Actual app shutdown: signal the same safe-point token, then join. Core
    /// optimizer batches and simulation actions are bounded cancellation units;
    /// workers cannot be waiting for a synchronous call back into Slint.
    /// For a repaintable close transition, call cancel + poll until Finished.
    pub fn shutdown(&mut self) -> Option<JobCompletion> {
        self.cancel();
        self.active.take().map(finish)
    }
}

impl Drop for WorkerCoordinator {
    fn drop(&mut self) {
        // Also prevents orphaned workers on event-loop/backend failure.
        drop(self.shutdown());
    }
}

fn lock_context(context: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    match context.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn recover_context(context: Arc<Mutex<Engine>>) -> Engine {
    // start failure destroys the closure; completion always joins first. Only
    // this owner remains, so recovery neither clones the catalog nor RNG state.
    let context = match Arc::try_unwrap(context) {
        Ok(context) => context,
        Err(_) => unreachable!("worker context must have one owner after join"),
    };
    match context.into_inner() {
        Ok(context) => context,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn finish(mut active: ActiveJob) -> JobCompletion {
    let joined = active.handle.join();
    // On shutdown or a fast finish, progress and the terminal message may still
    // be queued. They are not published as completed/partial work while closing.
    for message in active.receiver.try_iter() {
        if let Message::Finished(outcome) = message { active.outcome = Some(outcome); }
    }
    let outcome = match joined {
        Err(payload) => Err(WorkerError::Panic { operation: active.operation, message: panic_message(payload) }),
        Ok(()) => match active.outcome {
            Some(Ok(result)) if active.stop.load(Ordering::Acquire) => {
                drop(result);
                Err(WorkerError::Cancelled { operation: active.operation })
            }
            Some(outcome) => outcome,
            None => Err(WorkerError::Disconnected { operation: active.operation }),
        },
    };
    JobCompletion { context: recover_context(active.context), outcome }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() { message.clone() }
    else if let Some(message) = payload.downcast_ref::<&str>() { (*message).into() }
    else { "non-string panic payload".into() }
}

fn check_cancel(stop: &AtomicBool, operation: &'static str) -> Result<(), WorkerError> {
    if stop.load(Ordering::Acquire) { Err(WorkerError::Cancelled { operation }) } else { Ok(()) }
}

fn player(input: &PlayerInput, context: &mut Engine, stop: &AtomicBool, operation: &'static str) -> Result<Player, WorkerError> {
    check_cancel(stop, operation)?;
    let mut player = Player::new(input.gearset);
    build_player(input, context, &mut player)?;
    check_cancel(stop, operation)?;
    Ok(player)
}

fn ws_request(inputs: &CalculationInputs) -> Result<WsRequest, CalcError> {
    Ok(WsRequest { skill: resolve_name(&inputs.ws_name)?, ws_type: inputs.ws_type,
        tp: inputs.tp, metric: Metric::new("Damage dealt") })
}

fn spell_request(name: &str) -> Result<SpellRequest, CalcError> {
    if name == "None" {
        return Err(CalcError::legacy("Quicklook Spell", "spell_name", "AssertionError", "No spell selected."));
    }
    let spell_type = gui_spell_type(name)?;
    Ok(SpellRequest { spell: resolve_spell_name(name)?, spell_type, metric: Metric::new("Damage dealt") })
}

fn round_request(threshold: f64) -> ActionRequest {
    ActionRequest::Round { metric: Metric::new("Time to WS"), starting_tp: 0.0, threshold }
}


fn analytical(inputs: CalculationInputs, kind: AnalyticalKind, player: Player, context: &mut Engine,
    stop: &AtomicBool, operation: &'static str) -> Result<WorkResult, WorkerError> {
    let action = match kind {
        AnalyticalKind::Round => round_request(inputs.tp),
        AnalyticalKind::Ws => ActionRequest::Ws(ws_request(&inputs)?),
        AnalyticalKind::Spell => ActionRequest::Spell(spell_request(&inputs.spell_name)?),
    };
    let result = evaluate_action(&player, &inputs.enemy, &action, context)?;
    check_cancel(stop, operation)?;
    let display = reports::analytical_display(&result, kind == AnalyticalKind::Round);
    let name = match kind {
        AnalyticalKind::Round => "TP", AnalyticalKind::Ws => inputs.ws_name.as_ref(),
        AnalyticalKind::Spell => inputs.spell_name.as_ref(),
    };
    let report = reports::analytical_report(name, &display);
    Ok(WorkResult::Analytical { kind, player, result, display, report })
}

fn single_ws(inputs: CalculationInputs, player: Player, context: &mut Engine, stop: &AtomicBool,
    operation: &'static str) -> Result<WorkResult, WorkerError> {
    let ws = ws_request(&inputs)?;
    context.action_trace = Some(Vec::with_capacity(11));
    let sample = sample_ws(&player, &inputs.enemy, &ws, true, context)?;
    check_cancel(stop, operation)?;
    let events = context.action_trace.take().expect("single WS trace installed");
    let report = reports::single_ws_report(&player, &inputs.ws_name, inputs.tp, &sample, &events);
    Ok(WorkResult::SingleWs { player, sample, events, report })
}

fn compare(inputs: &CalculationInputs, tp_player: Player, ws_player: Player, context: &mut Engine,
    stop: &AtomicBool, operation: &'static str) -> Result<WorkResult, WorkerError> {
    let ws_action = ActionRequest::Ws(ws_request(inputs)?);
    let tp_ws = evaluate_action(&tp_player, &inputs.enemy, &ws_action, context)?;
    // The source int conversion occurs between the two WS calls.
    let tp_ws_damage = wsdist_core::math::python_int(tp_ws.metric, "compare", "TP set WS damage")?;
    check_cancel(stop, operation)?;
    let ws_ws = evaluate_action(&ws_player, &inputs.enemy, &ws_action, context)?;
    let ws_ws_damage = wsdist_core::math::python_int(ws_ws.metric, "compare", "WS set WS damage")?;
    check_cancel(stop, operation)?;
    let round = round_request(inputs.tp);
    let tp_round = evaluate_action(&tp_player, &inputs.enemy, &round, context)?;
    check_cancel(stop, operation)?;
    let ws_round = evaluate_action(&ws_player, &inputs.enemy, &round, context)?;
    check_cancel(stop, operation)?;
    let (tp_spell, ws_spell) = if inputs.spell_name.as_ref() != "None" {
        let spell = ActionRequest::Spell(spell_request(&inputs.spell_name)?);
        let tp_spell = evaluate_action(&tp_player, &inputs.enemy, &spell, context)?;
        check_cancel(stop, operation)?;
        let ws_spell = evaluate_action(&ws_player, &inputs.enemy, &spell, context)?;
        check_cancel(stop, operation)?;
        (Some(tp_spell), Some(ws_spell))
    } else { (None, None) };
    let outputs = ComparisonOutputs { tp_ws, ws_ws, tp_ws_damage, ws_ws_damage,
        tp_round, ws_round, tp_spell, ws_spell };
    let report = reports::comparison_report(&tp_player, &ws_player, &context.catalog,
        &inputs.ws_name, &inputs.spell_name, &outputs);
    Ok(WorkResult::Compare { tp_player, ws_player, outputs, report })
}

fn execute(request: WorkRequest, context: &mut Engine, stop: &AtomicBool,
    sender: &mpsc::Sender<Message>) -> Result<WorkResult, WorkerError> {
    let WorkRequest { quicklook, action } = request;
    let operation = action.operation();
    // Source simulate_tab.py:208-209 is unconditional. This build is important
    // even for another source: H2H updates shared Empty-piece skill overrides.
    let quick_player = player(&quicklook.player, context, stop, operation)?;
    match action {
        GuiAction::Round => analytical(quicklook, AnalyticalKind::Round, quick_player, context, stop, operation),
        GuiAction::Ws => analytical(quicklook, AnalyticalKind::Ws, quick_player, context, stop, operation),
        GuiAction::Spell => analytical(quicklook, AnalyticalKind::Spell, quick_player, context, stop, operation),
        GuiAction::SingleWs => single_ws(quicklook, quick_player, context, stop, operation),
        GuiAction::Stats { source, input } => {
            let player = if source == GearSource::Quicklook { quick_player } else { player(&input, context, stop, operation)? };
            let groups = reports::player_stats(source, &player, &context.catalog);
            let report = reports::stats_report(&groups);
            Ok(WorkResult::Stats { source, player, groups, report })
        }
        GuiAction::Compare { tp, ws } => {
            let tp_player = player(&tp, context, stop, operation)?;
            let ws_player = player(&ws, context, stop, operation)?;
            compare(&quicklook, tp_player, ws_player, context, stop, operation)
        }
        GuiAction::Optimize { kind, metric, candidates, min_pdt, min_mdt, options,
            similar_results, similar_percent, verbose_swaps } => {
            if !candidates.iter().any(|pool| pool.len() > 1) {
                return Err(CalcError::legacy("Optimize", "candidate selection", "AssertionError",
                    "At least two items must be selected in at least one slot to find the best set.").into());
            }
            let action = match kind {
                AnalyticalKind::Round => ActionRequest::Round { metric: Metric::new(metric),
                    starting_tp: 0.0, threshold: quicklook.tp },
                AnalyticalKind::Ws => {
                    if quicklook.ws_name.is_empty() || quicklook.ws_name.as_ref() == "None" {
                        return Err(CalcError::Input { operation, field: "ws_name",
                            message: "No weapon skill selected.".into() }.into());
                    }
                    let mut request = ws_request(&quicklook)?;
                    request.metric = Metric::new(metric);
                    ActionRequest::Ws(request)
                }
                AnalyticalKind::Spell => {
                    if quicklook.spell_name.is_empty() || quicklook.spell_name.as_ref() == "None" {
                        return Err(CalcError::Input { operation, field: "spell_name",
                            message: "No spell selected.".into() }.into());
                    }
                    let spell_type = wsdist_core::actions::optimizer_spell_type(&quicklook.spell_name);
                    ActionRequest::Spell(SpellRequest { spell: resolve_spell_name(&quicklook.spell_name)?,
                        spell_type, metric: Metric::new(metric) })
                }
            };
            let request = OptimizeRequest { player: quicklook.player, enemy: quicklook.enemy, action,
                candidates, ws_name: quicklook.ws_name, spell_name: quicklook.spell_name, min_pdt, min_mdt,
                options, similar_results, similar_percent, verbose_swaps };
            let mut progress = |progress| { let _ = sender.send(Message::Progress(progress)); };
            let result = optimize(&request, context, stop, &mut progress)?;
            check_cancel(stop, operation)?;
            let report = reports::optimizer_report(&request, &result, &context.catalog);
            Ok(WorkResult::Optimize { result, report })
        }
        GuiAction::Dps { tp, ws, plot } => {
            let request = match ws_request(&quicklook) {
                Ok(request) => request,
                Err(error) => {
                    // Resolution is inert; expose its failure only after the
                    // original target builds, preserving their state/errors.
                    player(&tp, context, stop, operation)?;
                    player(&ws, context, stop, operation)?;
                    return Err(error.into());
                }
            };
            let request = DpsRequest { tp_player: tp, ws_player: ws, enemy: quicklook.enemy,
                ws: request, threshold: quicklook.tp, trace: false };
            let result = simulate_dps(&request, context, stop)?;
            check_cancel(stop, operation)?;
            let report = reports::dps_report(&result);
            Ok(WorkResult::Dps { result, plot, report })
        }
        GuiAction::Distribution { ws } => {
            let request = match ws_request(&quicklook) {
                Ok(request) => request,
                Err(error) => {
                    player(&ws, context, stop, operation)?;
                    return Err(error.into());
                }
            };
            let request = DistributionRequest { player: ws, enemy: quicklook.enemy, ws: request };
            // Capture the core's one target build for the chart annotations.
            context.player_stages = Some(Vec::with_capacity(5));
            let result = sample_distribution(&request, context, stop)?;
            check_cancel(stop, operation)?;
            let final_stats = context.player_stages.take().and_then(|stages| {
                stages.into_iter().find(|snapshot| snapshot.stage == PlayerStage::Final).map(|snapshot| snapshot.stats)
            }).ok_or_else(|| CalcError::Input { operation: "WS distribution", field: "player stats",
                message: "The engine did not return its final player construction snapshot".into() })?;
            let presentation = reports::histogram_presentation(&request.player, &final_stats, &context.catalog,
                request.ws.tp, request.ws.skill.name(), &result);
            let report = reports::distribution_report(&result, &presentation);
            Ok(WorkResult::Distribution { result, presentation, report })
        }
    }
}
