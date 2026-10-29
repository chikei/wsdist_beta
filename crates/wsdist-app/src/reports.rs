//! Presentation of structured engine results, without changing their numeric fields.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, Write as _};

use wsdist_core::Catalog;
use wsdist_core::model::{
    ActionRequest, ActionResult, AttackKind, AttackProc, CalculationStep, DistributionResult,
    DpsResult, HitEvent, OptimizeRequest, OptimizeResult, OptimizeTermination, PieceKind,
    Player, PlayerInput, Progress, Slot, WsSample,
};
use wsdist_core::stats::{Stat, Stats, STAT_COUNT};

use crate::exchange::GearSource;

#[derive(Clone, Debug)]
pub struct TextReport {
    pub title: String,
    pub text: String,
}

impl TextReport {
    /// The GUI remains the authoritative report surface when stdout is absent.
    /// A broken pipe is reported; a Windows GUI with no stdout simply uses Results.
    pub fn mirror_stdout(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let handle = io::stdout().as_raw_handle();
            if handle.is_null() || handle as isize == -1 { return Ok(()); }
        }
        let stdout = io::stdout();
        let mut output = stdout.lock();
        output.write_all(self.text.as_bytes())?;
        if !self.text.ends_with('\n') {
            output.write_all(b"\n")?;
        }
        output.flush()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatDisplay {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatGroups {
    pub source_label: String,
    pub base: Vec<StatDisplay>,
    pub physical: Vec<StatDisplay>,
    pub magical: Vec<StatDisplay>,
    pub multi: Vec<StatDisplay>,
    pub haste: Vec<StatDisplay>,
    pub defensive: Vec<StatDisplay>,
    pub other: Vec<StatDisplay>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyticalDisplay {
    pub damage_label: &'static str,
    pub tp_label: &'static str,
    pub damage: String,
    pub tp_return: String,
    pub time_to_ws: String,
}

#[derive(Clone, Debug)]
pub struct ComparisonOutputs {
    pub tp_ws: ActionResult,
    pub ws_ws: ActionResult,
    pub tp_ws_damage: f64,
    pub ws_ws_damage: f64,
    pub tp_round: ActionResult,
    pub ws_round: ActionResult,
    pub tp_spell: Option<ActionResult>,
    pub ws_spell: Option<ActionResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistogramPresentation {
    pub title: [String; 3],
    pub annotations: Vec<StatDisplay>,
    pub mean_label: String,
}

const STAT_GROUPS: [&[Stat]; 7] = [
    &[Stat::Str, Stat::Dex, Stat::Vit, Stat::Agi, Stat::Int, Stat::Mnd, Stat::Chr],
    &[Stat::Accuracy1, Stat::Accuracy2, Stat::Attack1, Stat::Attack2,
        Stat::RangedAccuracy, Stat::RangedAttack],
    &[Stat::MagicAccuracy, Stat::MagicAttack, Stat::MagicDamage, Stat::MagicBurstDamage,
        Stat::MagicBurstDamageIi, Stat::MagicBurstDamageTrait],
    &[Stat::Daken, Stat::Zanshin, Stat::KickAttacks, Stat::Da, Stat::Ta, Stat::Qa,
        Stat::DoubleShot, Stat::TripleShot, Stat::QuadShot],
    &[Stat::DualWield, Stat::MartialArts, Stat::GearHaste, Stat::JaHaste,
        Stat::MagicHaste, Stat::DelayReduction],
    &[Stat::Pdt, Stat::Mdt, Stat::Dt, Stat::Evasion, Stat::MagicEvasion,
        Stat::MagicDefense, Stat::SubtleBlow, Stat::SubtleBlowIi],
    &[Stat::Regain, Stat::StoreTp, Stat::CritRate, Stat::CritDamage, Stat::RangedCritDamage,
        Stat::WeaponSkillDamage, Stat::WeaponSkillDamageTrait, Stat::SkillchainBonus,
        Stat::Pdl, Stat::PdlTrait, Stat::TpBonus],
];

/// The legacy Stats labels are blank until an explicit calculation or profile restore.
pub fn uncomputed_stats() -> StatGroups {
    let mut groups = STAT_GROUPS.map(|stats| stats.iter().map(|stat| StatDisplay {
        label: stat.name().into(), value: String::new(),
    }).collect::<Vec<_>>());
    StatGroups {
        source_label: "Not yet computed".into(),
        base: std::mem::take(&mut groups[0]), physical: std::mem::take(&mut groups[1]),
        magical: std::mem::take(&mut groups[2]), multi: std::mem::take(&mut groups[3]),
        haste: std::mem::take(&mut groups[4]), defensive: std::mem::take(&mut groups[5]),
        other: std::mem::take(&mut groups[6]),
    }
}

fn fixed(value: f64, digits: usize) -> String {
    if value.is_nan() { "nan".into() }
    else if value == f64::INFINITY { "inf".into() }
    else if value == f64::NEG_INFINITY { "-inf".into() }
    else { format!("{value:.digits$}") }
}

fn scalar(value: f64) -> String {
    if value.is_nan() { "nan".into() }
    else if value == f64::INFINITY { "inf".into() }
    else if value == f64::NEG_INFINITY { "-inf".into() }
    else if value == 0.0 && value.is_sign_negative() { "-0.0".into() }
    else { value.to_string() }
}

fn percentage(value: f64, digits: usize) -> String {
    format!("{}%", fixed(value, digits))
}

fn contains_ascii(name: &str, pattern: &str) -> bool {
    name.as_bytes().windows(pattern.len()).any(|part| part.eq_ignore_ascii_case(pattern.as_bytes()))
}

fn percentage_stat(name: &str, comparison: bool) -> bool {
    contains_ascii(name, "crit")
        || (comparison && contains_ascii(name, "elemental bonus"))
        || ["zanshin", "zanshin oa2", "daken", "kick attacks", "pdt", "mdt", "dt", "da",
            "ta", "qa", "double shot", "triple shot", "quad shot"].iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate))
        || contains_ascii(name, "pdl")
        || contains_ascii(name, "magic burst")
        || name.contains('%')
        || contains_ascii(name, "weapon skill damage")
        || contains_ascii(name, "skillchain")
        || name.get(..2).is_some_and(|prefix| prefix.eq_ignore_ascii_case("oa"))
            && name[2..].split_once(' ').is_some_and(|(number, hand)|
                matches!(number, "2" | "3" | "4" | "5" | "6" | "7" | "8")
                    && (hand.eq_ignore_ascii_case("main") || hand.eq_ignore_ascii_case("sub")))
}

fn format_stat(stat: Stat, value: f64, comparison: bool) -> String {
    let name = stat.name();
    let name = if name.get(..4).is_some_and(|prefix| prefix.eq_ignore_ascii_case("pet:")) { &name[4..] } else { name };
    if contains_ascii(name, "haste") || name.eq_ignore_ascii_case("delay reduction") || (comparison && contains_ascii(name, "attack%")) {
        let value = if !comparison {
            let cap = match stat {
                Stat::GearHaste | Stat::JaHaste => Some(256.0 / 1024.0),
                Stat::MagicHaste => Some(448.0 / 1024.0),
                _ => None,
            };
            match cap { Some(cap) if value > cap => cap, _ => value }
        } else { value };
        percentage(value * 100.0, 1)
    } else if percentage_stat(name, comparison) {
        percentage(value, 0)
    } else if contains_ascii(name, "attack") || contains_ascii(name, "accuracy") || contains_ascii(name, "evasion") || name.eq_ignore_ascii_case("store tp") {
        fixed(value, 0)
    } else {
        scalar(value)
    }
}

fn displayed_regain(player: &Player, catalog: &Catalog) -> f64 {
    player.stats.get(Stat::Regain)
        + if catalog.piece(player.gearset[Slot::Main.index()]).name.as_ref() == "Gokotai" {
            player.stats.get(Stat::DualWield)
        } else { 0.0 }
}

/// All 53 source fields, in the original seven groups. No player build or RNG
/// access occurs here, so profile/equip refreshes can use their already-built player.
pub fn player_stats(source: GearSource, player: &Player, catalog: &Catalog) -> StatGroups {
    let mut groups = STAT_GROUPS.map(|stats| stats.iter().map(|&stat| {
        let value = if stat == Stat::Regain { displayed_regain(player, catalog) } else { player.stats.get(stat) };
        StatDisplay { label: stat.name().into(), value: format_stat(stat, value, false) }
    }).collect::<Vec<_>>());
    StatGroups {
        source_label: format!("{} Gear Stats", source.display_name()),
        base: std::mem::take(&mut groups[0]),
        physical: std::mem::take(&mut groups[1]),
        magical: std::mem::take(&mut groups[2]),
        multi: std::mem::take(&mut groups[3]),
        haste: std::mem::take(&mut groups[4]),
        defensive: std::mem::take(&mut groups[5]),
        other: std::mem::take(&mut groups[6]),
    }
}

pub fn stats_report(groups: &StatGroups) -> TextReport {
    let mut text = format!("{}\n", groups.source_label);
    for (title, rows) in [
        ("Base Parameters", &groups.base), ("Physical", &groups.physical),
        ("Magical", &groups.magical), ("Multi-Attack", &groups.multi),
        ("Attack Speed", &groups.haste), ("Defensive", &groups.defensive),
        ("Other Stats", &groups.other),
    ] {
        let _ = writeln!(text, "\n{title}");
        for row in rows { let _ = writeln!(text, "{:>26}  {}", row.label, row.value); }
    }
    TextReport { title: groups.source_label.clone(), text }
}

pub fn analytical_display(result: &ActionResult, round: bool) -> AnalyticalDisplay {
    let time = result.time_to_ws.map(|value| fixed(value, 3)).unwrap_or_default();
    AnalyticalDisplay {
        damage_label: if round { "Time per WS =" } else { "Average Damage =" },
        tp_label: if round { "TP/round =" } else { "Average TP =" },
        damage: if round { fixed(result.metric, 3) } else { fixed(result.damage, 0) },
        tp_return: fixed(result.tp_return, 1),
        time_to_ws: time,
    }
}

pub fn analytical_report(name: &str, display: &AnalyticalDisplay) -> TextReport {
    TextReport {
        title: format!("Quicklook {name}"),
        text: format!("Quicklook {name}\n{} {}\n{} {}\n", display.damage_label, display.damage,
            display.tp_label, display.tp_return),
    }
}

/// Render equations captured by the evaluator, without evaluating combat again.
pub fn append_calculation(text: &mut String, steps: &[CalculationStep], sampled: bool) {
    if steps.is_empty() { return; }
    text.push_str("\nCalculation process\n-------------------\n");
    text.push_str(if sampled {
        "Sampled result; observed hit outcomes are listed above.\n"
    } else {
        "Analytical result: expected values, not a particular random roll.\n"
    });
    text.push_str("[random ...] marks a random-number placeholder, not a fixed roll.\n\
        U(a,b) means a uniform random draw; trunc(x) discards the fractional part.\n\
        Equation values are rounded to at most 6 decimal places.\n");
    for (index, step) in steps.iter().enumerate() {
        let _ = writeln!(text, "\n{}. {}", index + 1, step.label);
        text.push_str("   ");
        let mut formula = step.formula;
        while let Some(open) = formula.find('{') {
            text.push_str(&formula[..open]);
            let rest = &formula[open + 1..];
            let close = rest.find('}').expect("calculation operand must have a closing brace");
            let name = &rest[..close];
            let value = step.values.iter().find(|(key, _)| *key == name)
                .expect("calculation operand must be captured by the evaluator").1;
            append_calculation_value(text, value);
            formula = &rest[close + 1..];
        }
        text.push_str(formula);
        text.push('\n');
    }
    text.push('\n');
}

fn append_calculation_value(text: &mut String, value: f64) {
    if !value.is_finite() || value == 0.0 || value.abs() < 1e-6 || value.abs() >= 1e9 {
        let _ = write!(text, "{}", scalar(value));
        return;
    }
    let _ = write!(text, "{value:.6}");
    while text.ends_with('0') { text.pop(); }
    if text.ends_with('.') { text.pop(); }
}

fn proc_label(proc: AttackProc) -> Option<Cow<'static, str>> {
    match proc {
        AttackProc::QuadAttack => Some("Quad. Attack".into()),
        AttackProc::TripleAttack => Some("Triple Attack".into()),
        AttackProc::DoubleAttack => Some("Double Attack".into()),
        AttackProc::Occasionally(number) => Some(match number {
            2 => "OA2".into(), 3 => "OA3".into(), 4 => "OA4".into(), 5 => "OA5".into(),
            6 => "OA6".into(), 7 => "OA7".into(), 8 => "OA8".into(),
            _ => format!("OA{number}").into(),
        }),
        AttackProc::FollowUp => Some("Follow-up".into()),
        _ => None,
    }
}

pub fn single_ws_report(player: &Player, name: &str, tp: f64, sample: &WsSample, events: &[HitEvent]) -> TextReport {
    let bonus = player.stats.get(Stat::TpBonus);
    // Same ordered max(1000, min(3000, tp + bonus)) as the source header,
    // including NaN selecting the first min argument, not a new input clamp.
    let effective = if tp + bonus < 3000.0 { tp + bonus } else { 3000.0 };
    let effective = if effective > 1000.0 { effective } else { 1000.0 };
    let mut text = format!("\n{name} at {} TP (+{} TP Bonus; Effective TP: {} TP)\n",
        fixed(tp, 1), fixed(bonus, 0), fixed(effective, 1));
    let mut previous_proc = None;
    let mut proc_hits_left = 0_u8;
    for event in events {
        if event.kind == AttackKind::TpRetention {
            let label = match event.proc {
                AttackProc::Fotia => "Fotia:",
                AttackProc::ConserveTp => "Conserve TP:",
                _ => "TP retention:",
            };
            let _ = writeln!(text, "    {label:<16}                         +{} TP", fixed(event.tp_return, 1));
            previous_proc = None;
            proc_hits_left = 0;
            continue;
        }
        let label = match event.kind {
            AttackKind::Main => "Main-hand:", AttackKind::Offhand => "Off-hand:",
            AttackKind::Ranged => "Ranged Hit:", AttackKind::Kick => "Kick:",
            AttackKind::Daken => "Daken:", AttackKind::Zanshin => "Zanshin:",
            AttackKind::Magical => if event.attempt == 0 { "Hybrid:" } else { "Magic Hit:" },
            AttackKind::TpRetention => unreachable!(),
        };
        let proc = proc_label(event.proc);
        let current_proc = proc.as_ref().map(|_| (event.kind, event.proc));
        if (current_proc != previous_proc || proc_hits_left == 0) && let Some(proc) = &proc {
            let hand = if event.kind == AttackKind::Offhand { "Off-hand" } else { "Main-hand" };
            let _ = writeln!(text, "    {hand} {proc}:");
            proc_hits_left = match event.proc {
                AttackProc::QuadAttack => 3, AttackProc::TripleAttack => 2,
                AttackProc::DoubleAttack | AttackProc::FollowUp => 1,
                AttackProc::Occasionally(number) => number.saturating_sub(1),
                _ => 0,
            };
        }
        previous_proc = current_proc;
        proc_hits_left = proc_hits_left.saturating_sub(1);
        let label = if proc.is_some() { "" } else { label };
        if !event.landed {
            let _ = writeln!(text, "    {:<10}        Missed.", label.trim_end_matches(':'));
        } else {
            let physical = format!("+{}", fixed(event.physical_damage, 0));
            let magical = format!("+{}", fixed(event.magical_damage, 0));
            let tp_return = format!("+{}", fixed(event.tp_return, 1));
            let _ = writeln!(text, "    {label:<10}  [{physical:>7} Phys.] [{magical:>7} Magic]  [{tp_return:>6} TP]  {}",
                if event.critical { "Critical Hit!" } else { "" });
        }
    }
    let _ = writeln!(text, "    =       Phys. Damage: {:>6}\n    =       Magic Damage: {:>6}\n    =        TP Returned: {:>6}\n",
        fixed(sample.physical_damage, 0), fixed(sample.magical_damage, 0), fixed(sample.tp_return, 1));
    TextReport { title: format!("{name} — single WS"), text }
}

pub fn optimizer_progress(progress: &Progress) -> TextReport {
    let mut text = format!("DT stage {}, current iteration: {}, scored: {}\nCurrent best score: {}   PDT:{}   MDT:{}\n",
        progress.stage, progress.pass, progress.scored, scalar(progress.best_metric), scalar(progress.pdt), scalar(progress.mdt));
    if let Some(message) = &progress.message { let _ = writeln!(text, "{message}"); }
    TextReport { title: "Optimizer progress".into(), text }
}

fn action_metric(action: &ActionRequest) -> &str {
    match action {
        ActionRequest::Round { metric, .. } => &metric.name,
        ActionRequest::Ws(request) => &request.metric.name,
        ActionRequest::Spell(request) => &request.metric.name,
    }
}

pub fn optimizer_report(request: &OptimizeRequest, result: &OptimizeResult, catalog: &Catalog) -> TextReport {
    let (header, decimals) = match &request.action {
        ActionRequest::Round { .. } => ("Melee TP set", 3),
        ActionRequest::Ws(_) => (request.ws_name.as_ref(), 1),
        ActionRequest::Spell(_) => (request.spell_name.as_ref(), 1),
    };
    let metric = action_metric(&request.action);
    let title = format!("Best   \"{metric}\"   \"{header}\"   set");
    let mut text = String::new();
    if !result.feasible {
        let _ = writeln!(text, "WARNING: Unsatisfied damage-taken requirements.\nRequired PDT:{} MDT:{}; achieved PDT:{} MDT:{}.",
            scalar(request.min_pdt), scalar(request.min_mdt), scalar(result.pdt), scalar(result.mdt));
    }
    for warning in &result.warnings { let _ = writeln!(text, "{warning}"); }
    if result.player.is_none() {
        let _ = writeln!(text, "No winning player returned. Legacy score: {}", scalar(result.metric));
    } else {
        let _ = writeln!(text, "==============================================================\n{title}\n==============================================================");
        for &slot in Slot::ALL {
            let piece = catalog.piece(result.gearset[slot.index()]);
            let _ = writeln!(text, "{:>10}  {:<50}", slot.name(), piece.name2);
        }
        text.push('\n');
        let output = |index: usize, digits| result.legacy_output.get(index).map(|&value| fixed(value, digits));
        match &request.action {
            ActionRequest::Round { .. } if metric == "Time to WS" => {
                let _ = writeln!(text, "Avg WS Time = {:<8} s", fixed(result.metric.powi(i32::from(result.invert)), decimals));
                if let Some(tp) = output(1, 1) { let _ = writeln!(text, "Avg TP per round = {tp:<5} TP"); }
            }
            ActionRequest::Round { .. } => {
                if let Some(damage) = output(0, decimals) { let _ = writeln!(text, "Avg Damage per round = {damage:<8} damage"); }
                if let Some(time) = output(2, 1) { let _ = writeln!(text, "Avg time per round = {time:<5} s"); }
                if let Some(tp) = output(1, 1) { let _ = writeln!(text, "Avg TP per round = {tp:<5} TP"); }
            }
            _ => {
                if let Some(damage) = output(0, decimals) { let _ = writeln!(text, "Avg Damage = {damage:<8} damage"); }
                if let Some(tp) = output(1, 1) { let _ = writeln!(text, "Avg TP return = {tp:<5} TP"); }
            }
        }
        text.push_str("==============================================================\n==============================================================\n");
        append_calculation(&mut text, &result.calculation, false);
    }
    let termination = match result.termination {
        OptimizeTermination::Converged => "Converged", OptimizeTermination::IterationLimit => "Iteration limit",
        OptimizeTermination::UnchangedDtStages => "Three unchanged DT stages",
        OptimizeTermination::NoWinner => "No winning improvement (legacy sentinel retained)",
        OptimizeTermination::UnsatisfiedDt => "Unsatisfied DT",
    };
    let _ = writeln!(text, "Termination: {termination}\nDT stages: {}; passes: {}; scored: {}; restarts: {}\nAchieved PDT:{} MDT:{}; feasible: {}\nAccepted improvements: {}",
        result.stages, result.passes, result.scored, result.restarts, scalar(result.pdt), scalar(result.mdt), result.feasible, result.accepted.len());
    if request.verbose_swaps || request.player.abilities.enabled(wsdist_core::model::Ability::VerboseSwaps) {
        for (index, accepted) in result.accepted.iter().enumerate() {
            let _ = writeln!(text, "\nAccepted set {}: metric {}; PDT:{} MDT:{}", index + 1,
                fixed(accepted.metric.powi(i32::from(result.invert)), decimals), scalar(accepted.pdt), scalar(accepted.mdt));
            for &slot in Slot::ALL {
                if index == 0 || accepted.gearset[slot.index()] != result.accepted[index - 1].gearset[slot.index()] {
                    let _ = writeln!(text, "{:>10}  {}", slot.name(), catalog.piece(accepted.gearset[slot.index()]).name2);
                }
            }
        }
    }
    if request.similar_results {
        let _ = writeln!(text, "\nList of potential swaps within {}% of the best set ({}):",
            scalar(request.similar_percent), fixed(result.metric.powi(i32::from(result.invert)), decimals));
        for swap in &result.similar {
            let _ = writeln!(text, "{:<6} {:<50} {:<8} {:>5}%", swap.slot.name(), catalog.piece(swap.gear).name2,
                fixed(swap.metric, decimals), fixed(swap.difference_percent, 1));
        }
    }
    TextReport { title, text }
}

pub fn dps_report(result: &DpsResult) -> TextReport {
    let text = format!("\n    =========================\n    Total time: {:>5} h\n    Time/Attack: {:>7} s\n    Average Dmg/Attack: {:>8}\n        Physical: {:>8} ({:>5}%)\n         Magical: {:>8} ({:>5}%)\n    Average Dmg/WS: {:>8}\n    Average TP/WS: {:>4} TP (+{} TP Bonus)\n    Total Damage: {:>5}M damage (Total DPS: {:>7})\n    TP Damage: {:>5}M damage (TP DPS: {:>7}; {:>5}%)\n    WS Damage: {:>5}M damage (WS DPS: {:>7}; {:>5}%)\n    =========================\n",
        fixed(result.final_time / 3600.0, 1), fixed(result.seconds_per_round, 3), fixed(result.average_tp_damage, 1),
        fixed(result.average_physical_round_damage, 1), fixed(result.average_physical_round_damage / result.average_tp_damage * 100.0, 1),
        fixed(result.average_magical_round_damage, 1), fixed(result.average_magical_round_damage / result.average_tp_damage * 100.0, 1),
        fixed(result.average_ws_damage, 1), fixed(result.average_pre_bonus_tp, 0), scalar(result.ws_tp_bonus),
        fixed(result.damage / 1e6, 1), fixed(result.damage / result.final_time, 1),
        fixed(result.tp_damage / 1e6, 1), fixed(result.tp_damage / result.final_time, 1), fixed(result.tp_damage / result.damage * 100.0, 1),
        fixed(result.ws_damage / 1e6, 1), fixed(result.ws_damage / result.final_time, 1), fixed(result.ws_damage / result.damage * 100.0, 1));
    TextReport { title: "DPS simulation".into(), text }
}

/// Unlike the Stats tab, comparison haste is not capped for display. The raw
/// sheets remain untouched; derived action fields are appended only to this report.
pub fn comparison_report(tp: &Player, ws: &Player, catalog: &Catalog, ws_name: &str, spell_name: &str,
    outputs: &ComparisonOutputs) -> TextReport {
    let mut present = [false; STAT_COUNT];
    present[Stat::Regain.index()] = true;
    let mut width = 0;
    for (stat, _) in tp.stats.ordered().chain(ws.stats.ordered()) {
        present[stat.index()] = true;
        width = width.max(stat.name().len());
    }
    let mut names = [Stat::Regain; STAT_COUNT];
    let mut count = 0;
    for stat in Stat::ALL {
        if present[stat.index()] { names[count] = stat; count += 1; }
    }
    names[..count].sort_unstable_by_key(|stat| stat.name());
    let mut text = String::from("================================================\n");
    for &stat in &names[..count] {
        let name = stat.name();
        let (tp_value, ws_value) = if stat == Stat::Regain {
            (displayed_regain(tp, catalog), displayed_regain(ws, catalog))
        } else { (tp.stats.get(stat), ws.stats.get(stat)) };
        let tp_value = format_stat(stat, tp_value, true);
        let ws_value = format_stat(stat, ws_value, true);
        let _ = writeln!(text, "{name:>width$}   {tp_value:>10}   {ws_value:<10}");
    }
    let tp_time = comparison_time(outputs.tp_round.metric);
    let ws_time = comparison_time(outputs.ws_round.metric);
    let _ = writeln!(text, "{:>width$}   {tp_time:>10}   {ws_time:<10}", "Time per WS");
    let damage_name = format!("{ws_name} Damage");
    let tp_damage = scalar(outputs.tp_ws_damage);
    let ws_damage = scalar(outputs.ws_ws_damage);
    let _ = writeln!(text, "{damage_name:>width$}   {tp_damage:>10}   {ws_damage:<10}");
    if let (Some(tp_spell), Some(ws_spell)) = (&outputs.tp_spell, &outputs.ws_spell) {
        let spell_label = format!("{spell_name} Damage");
        let tp_damage = scalar(tp_spell.metric);
        let ws_damage = scalar(ws_spell.metric);
        let _ = writeln!(text, "{spell_label:>width$}   {tp_damage:>10}   {ws_damage:<10}");
    }
    let tp_wsc = tp.wsc.first();
    let ws_wsc = ws.wsc.first();
    let wsc_name = tp_wsc.or(ws_wsc).map_or("", |wsc| wsc.stat.name());
    let label = format!("WSC:{wsc_name}");
    let tp_wsc = percentage(tp_wsc.map_or(0.0, |wsc| wsc.percent), 0);
    let ws_wsc = percentage(ws_wsc.map_or(0.0, |wsc| wsc.percent), 0);
    let _ = writeln!(text, "{label:>width$}   {tp_wsc:>10}   {ws_wsc:<10}");
    TextReport { title: "TP & WS stat comparison".into(), text }
}

fn comparison_time(value: f64) -> String {
    let mut text = fixed(value, 3);
    if value.is_finite() {
        while text.ends_with('0') { text.pop(); }
        if text.ends_with('.') { text.push('0'); }
    }
    text
}

/// The final stats snapshot is captured from the distribution's own player
/// build; producing plot annotations never rebuilds a player or consumes RNG.
pub fn histogram_presentation(input: &PlayerInput, stats: &Stats, catalog: &Catalog,
    tp: f64, ws_name: &str, result: &DistributionResult) -> HistogramPresentation {
    let dual_wield = catalog.piece(input.gearset[Slot::Sub.index()]).kind == PieceKind::Weapon;
    let title = [
        format!("ML{} {}/{}", input.master_level.clamp(0, 50), input.main_job.name().to_ascii_uppercase(),
            input.sub_job.map_or("NONE".into(), |job| job.name().to_ascii_uppercase())),
        format!("{:>15} {:>8} {:>8} {:>8} {:>8}", format!("TP={}", scalar(tp)), "Minimum", "Mean", "Median", "Maximum"),
        format!("{ws_name:>15} {:>8} {:>8} {:>8} {:>8}", fixed(result.minimum, 0),
            fixed(result.title_mean, 0), fixed(result.title_median, 0), fixed(result.maximum, 0)),
    ];
    let mut annotations = Vec::with_capacity(13);
    for &stat in STAT_GROUPS[0] {
        annotations.push(StatDisplay { label: stat.name().into(), value: fixed(stats.get(stat), 0) });
    }
    for (label, stat) in [("Accuracy1", Stat::Accuracy1), ("Accuracy2", Stat::Accuracy2),
        ("Attack1", Stat::Attack1), ("Attack2", Stat::Attack2),
        ("Ranged Acc.", Stat::RangedAccuracy), ("Ranged Atk.", Stat::RangedAttack)] {
        let value = if !dual_wield && matches!(stat, Stat::Accuracy2 | Stat::Attack2) { 0.0 } else { stats.get(stat) };
        annotations.push(StatDisplay { label: label.into(), value: fixed(value, 0) });
    }
    HistogramPresentation { title, annotations,
        mean_label: format!("Average = {} damage.", fixed(result.title_mean, 0)) }
}

pub fn distribution_report(result: &DistributionResult, presentation: &HistogramPresentation) -> TextReport {
    let mut text = presentation.title.join("\n");
    let _ = writeln!(text, "\n\n{} independent fixed-TP samples; {} density bins.\nRaw mean: {}\nRaw median: {}",
        result.damage.len(), result.histogram.counts.len(), scalar(result.mean), scalar(result.median));
    for row in &presentation.annotations { let _ = writeln!(text, "{:>14} = {:>4}", row.label, row.value); }
    TextReport { title: "WS damage distribution".into(), text }
}
