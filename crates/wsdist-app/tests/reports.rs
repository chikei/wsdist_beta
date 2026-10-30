use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use wsdist_fixtures::Value;
use wsdist_app::exchange::GearSource;
use wsdist_app::reports::{self, ComparisonOutputs, StatDisplay, StatGroups};
use wsdist_core::Catalog;
use wsdist_core::model::{
    Abilities, ActionRequest, ActionResult, AttackKind, AttackProc, DistributionResult,
    Histogram, HitEvent, Job, Metric, ModelState, OptimizeOptions, OptimizeRequest,
    OptimizeResult, OptimizeTermination, Player, Slot, WsRequest, WsSample, WsType,
    WscContribution,
};
use wsdist_core::stats::{Stat, Stats};
use wsdist_core::weaponskills::resolve_name;

#[path = "../../wsdist-core/tests/legacy_player/support.rs"]
#[allow(dead_code)]
mod support;
use support::{document_json, number};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../wsdist-core/tests/fixtures/legacy")
}
fn frozen(index: &str, id: &str) -> Value {
    let root = root();
    let index = document_json(&root.join(index));
    assert_eq!(index["schema_version"], 3);
    let entry = index["cases"].as_array().unwrap().iter().find(|entry| entry["id"] == id).unwrap();
    support::fixture(&root, entry)
}
fn catalog() -> Arc<Catalog> {
    static CATALOG: LazyLock<Arc<Catalog>> = LazyLock::new(|| Arc::new(
        Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()));
    Arc::clone(&CATALOG)
}

// Reports receive captured engine results; expected numbers never come from a
// second call to build_player/evaluate_action/summarize_distribution.
fn captured_player(value: &Value, catalog: &Catalog) -> Player {
    let gearset = std::array::from_fn(|index| {
        let piece = &value["gearset"][Slot::ALL[index].name()];
        catalog.by_original_id(piece["id"].as_u64().unwrap() as u32).unwrap()
    });
    let mut player = Player::new(gearset);
    player.main_job = Job::parse_player_name(value["main_job"].as_str().unwrap()).unwrap();
    player.sub_job = Job::parse_player_name(value["sub_job"].as_str().unwrap());
    player.master_level = value["master_level"].as_i64().unwrap() as i32;
    player.main_job_level = value["main_job_level"].as_i64().unwrap() as i32;
    player.sub_job_level = value["sub_job_level"].as_i64().unwrap() as i32;
    player.stats = wsdist_fixtures::from_value(&value["stats"]).unwrap();
    player.wsc = value["wsc"].as_array().unwrap().iter().map(|row| WscContribution {
        stat: Stat::from_name(row[0].as_str().unwrap()).unwrap(), percent: number(&row[1]),
    }).collect();
    player.wyvern_bonus_attack = value["wyvern_bonus_attack"].as_bool().unwrap();
    player.abilities = wsdist_fixtures::from_value::<Abilities>(&value["abilities"]).unwrap();
    player
}

fn captured_action(value: &Value) -> ActionResult {
    ActionResult {
        damage: number(&value["damage"]), tp_return: number(&value["tp_return"]),
        seconds_per_round: value.get("seconds_per_round").map(number),
        time_to_ws: value.get("seconds_per_ws").map(number),
        metric: number(&value["metric"]), invert: value["invert"].as_i64().unwrap() as i8,
        physical_damage: number(&value["physical_damage"]), magical_damage: number(&value["magical_damage"]),
    }
}

fn comparison_outputs() -> ComparisonOutputs {
    let tp = frozen("combat.json", "combat.000359.ws.Savage_Blade.tp1000.Damage_dealt");
    let ws = frozen("combat.json", "combat.000364.ws.Savage_Blade.tp1500.Damage_dealt");
    let round = frozen("combat.json", "combat.018457.round.normal.Time_to_WS");
    let fields = |case: &Value| captured_action(&case["expected"]["value"]["compiled"]["fields"]);
    ComparisonOutputs {
        tp_ws: fields(&tp), ws_ws: fields(&ws), tp_ws_damage: 667.0, ws_ws_damage: 831.0,
        tp_round: fields(&round), ws_round: fields(&round), tp_spell: None, ws_spell: None,
    }
}

fn groups(groups: &StatGroups) -> [&[StatDisplay]; 7] {
    [&groups.base, &groups.physical, &groups.magical, &groups.multi,
        &groups.haste, &groups.defensive, &groups.other]
}
fn value<'a>(groups: &'a StatGroups, name: &str) -> &'a str {
    groups.base.iter().chain(&groups.physical).chain(&groups.magical).chain(&groups.multi)
        .chain(&groups.haste).chain(&groups.defensive).chain(&groups.other)
        .find(|row| row.label == name).unwrap_or_else(|| panic!("missing displayed stat {name}")).value.as_str()
}
fn raw_sheet(player: &Player) -> (Vec<(Stat, u64)>, Vec<WscContribution>) {
    (player.stats.ordered().map(|(stat, value)| (stat, value.to_bits())).collect(), player.wsc.clone())
}
fn report_values<'a>(text: &'a str, label: &str) -> Vec<&'a str> {
    let line = text.lines().find_map(|line| line.trim().strip_prefix(label)
        .filter(|rest| rest.starts_with(char::is_whitespace)))
        .unwrap_or_else(|| panic!("report is missing {label:?}: {text}"));
    line.split_whitespace().collect()
}

#[test]
fn every_source_stat_remains_in_its_original_group_with_its_display_units() {
    let catalog = catalog();
    let mut player = Player::new([catalog.empty; Slot::COUNT]);
    for (stat, number) in [
        (Stat::Str, 147.5), (Stat::Attack1, 1234.5), (Stat::Accuracy1, 826.6),
        (Stat::MagicAttack, 200.6), (Stat::MagicBurstDamage, 39.5),
        (Stat::Da, 27.5), (Stat::GearHaste, 0.5), (Stat::JaHaste, 0.5),
        (Stat::MagicHaste, 0.75), (Stat::DelayReduction, 0.9),
        (Stat::Pdt, -54.5), (Stat::Evasion, 899.5), (Stat::Regain, -0.0),
        (Stat::StoreTp, 50.5), (Stat::CritRate, 27.5), (Stat::TpBonus, 730.0),
    ] { player.stats.set(stat, number); }
    let before = raw_sheet(&player);
    let displayed = reports::player_stats(GearSource::Ws, &player, &catalog);
    let expected: [&[&str]; 7] = [
        &["STR", "DEX", "VIT", "AGI", "INT", "MND", "CHR"],
        &["Accuracy1", "Accuracy2", "Attack1", "Attack2", "Ranged Accuracy", "Ranged Attack"],
        &["Magic Accuracy", "Magic Attack", "Magic Damage", "Magic Burst Damage", "Magic Burst Damage II", "Magic Burst Damage Trait"],
        &["Daken", "Zanshin", "Kick Attacks", "DA", "TA", "QA", "Double Shot", "Triple Shot", "Quad Shot"],
        &["Dual Wield", "Martial Arts", "Gear Haste", "JA Haste", "Magic Haste", "Delay Reduction"],
        &["PDT", "MDT", "DT", "Evasion", "Magic Evasion", "Magic Defense", "Subtle Blow", "Subtle Blow II"],
        &["Regain", "Store TP", "Crit Rate", "Crit Damage", "Ranged Crit Damage", "Weapon Skill Damage", "Weapon Skill Damage Trait", "Skillchain Bonus", "PDL", "PDL Trait", "TP Bonus"],
    ];
    let mut labels = BTreeSet::new();
    for (actual, expected) in groups(&displayed).into_iter().zip(expected) {
        assert_eq!(actual.iter().map(|row| row.label.as_str()).collect::<Vec<_>>(), expected);
        for row in actual { assert!(labels.insert(row.label.as_str()), "stat is duplicated across source groups"); }
    }
    assert_eq!(labels.len(), 53, "every consumer-visible field is represented exactly once");
    for (name, expected) in [
        ("STR", "147.5"), ("Attack1", "1234"), ("Accuracy1", "827"),
        ("Magic Attack", "201"), ("Magic Burst Damage", "40%"), ("DA", "28%"),
        ("Gear Haste", "25.0%"), ("JA Haste", "25.0%"), ("Magic Haste", "43.8%"),
        ("Delay Reduction", "90.0%"), ("PDT", "-54%"), ("Evasion", "900"),
        ("Regain", "0"), ("Store TP", "50"), ("Crit Rate", "28%"), ("TP Bonus", "730"),
    ] { assert_eq!(value(&displayed, name), expected, "display units/precision for {name}"); }
    assert_eq!(raw_sheet(&player), before, "display caps, rounding, and Regain must not change the original sheet");
}

#[test]
fn stats_caps_only_haste_above_its_display_ceiling_and_never_caps_comparison() {
    let catalog = catalog();
    let mut player = Player::new([catalog.empty; Slot::COUNT]);
    let outputs = comparison_outputs();
    for (gear, ja, magic, expected) in [
        (0.125, 0.125, 0.25, ["12.5%", "12.5%", "25.0%"]),
        (0.25, 0.25, 448.0 / 1024.0, ["25.0%", "25.0%", "43.8%"]),
        (0.5, 0.75, 0.875, ["25.0%", "25.0%", "43.8%"]),
        (-0.125, -0.125, -0.25, ["-12.5%", "-12.5%", "-25.0%"]),
    ] {
        for (stat, value) in [(Stat::GearHaste, gear), (Stat::JaHaste, ja), (Stat::MagicHaste, magic)] {
            player.stats.set(stat, value);
        }
        let before = raw_sheet(&player);
        let display = reports::player_stats(GearSource::Tp, &player, &catalog);
        for (name, expected) in ["Gear Haste", "JA Haste", "Magic Haste"].into_iter().zip(expected) {
            assert_eq!(value(&display, name), expected, "display boundary for {name}");
        }
        if gear == 0.5 {
            let comparison = reports::comparison_report(&player, &player, &catalog, "Savage Blade", "None", &outputs);
            assert_eq!(report_values(&comparison.text, "Gear Haste"), ["50.0%", "50.0%"]);
            assert_eq!(report_values(&comparison.text, "JA Haste"), ["75.0%", "75.0%"]);
            assert_eq!(report_values(&comparison.text, "Magic Haste"), ["87.5%", "87.5%"]);
        }
        assert_eq!(raw_sheet(&player), before);
    }
}

#[test]
fn gokotai_display_adds_dual_wield_to_sparse_regain_only_for_the_equipped_main() {
    let catalog = catalog();
    let fixture = frozen("search_simulation.json", "search-simulation/full7200-gokotai-tp-heishi-ws-host11-compiled20240101");
    let expected = &fixture["expected"]["value"]["players"];
    let tp = captured_player(&expected["tp_after_build"], &catalog);
    let ws = captured_player(&expected["ws_after_build"], &catalog);
    let before_tp = raw_sheet(&tp);
    let before_ws = raw_sheet(&ws);
    let tp_display = reports::player_stats(GearSource::Tp, &tp, &catalog);
    let ws_display = reports::player_stats(GearSource::Ws, &ws, &catalog);
    assert_eq!(value(&tp_display, "Regain"), "35", "Gokotai's displayed Regain includes its 35 Dual Wield");
    assert_eq!(value(&ws_display, "Regain"), "0", "the distinct Heishi WS main must not inherit Gokotai Regain");
    let report = reports::comparison_report(&tp, &ws, &catalog, "Blade: Shun", "None", &comparison_outputs());
    assert_eq!(report_values(&report.text, "Regain"), ["35", "0"]);
    assert_eq!(raw_sheet(&tp), before_tp, "Regain is a derived display field, not an inserted or mutated stat");
    assert_eq!(raw_sheet(&ws), before_ws);

    let mut buffed = tp.clone();
    buffed.stats.set(Stat::Regain, 5.5);
    let before = raw_sheet(&buffed);
    let display = reports::player_stats(GearSource::Tp, &buffed, &catalog);
    assert_eq!(value(&display, "Regain"), "40.5", "Gokotai adds Dual Wield to existing Regain rather than replacing it");
    let report = reports::comparison_report(&buffed, &ws, &catalog, "Blade: Shun", "None", &comparison_outputs());
    assert_eq!(report_values(&report.text, "Regain"), ["40.5", "0"]);
    assert_eq!(raw_sheet(&buffed), before);
}

#[test]
fn comparison_uses_alphabetical_sparse_union_and_only_the_first_wsc_pair() {
    let catalog = catalog();
    let mut tp = Player::new([catalog.empty; Slot::COUNT]);
    let mut ws = Player::new([catalog.empty; Slot::COUNT]);
    tp.stats.set(Stat::StoreTp, 50.5);
    tp.stats.set(Stat::Str, 147.0);
    tp.stats.set(Stat::AttackPct, 0.125);
    ws.stats.set(Stat::MagicDamage, 42.0);
    ws.stats.set(Stat::Str, 155.0);
    ws.stats.set(Stat::AttackPct, 0.5);
    tp.wsc = vec![WscContribution { stat: Stat::Str, percent: 10.0 },
        WscContribution { stat: Stat::Dex, percent: 99.0 }];
    ws.wsc = vec![WscContribution { stat: Stat::Int, percent: 20.0 },
        WscContribution { stat: Stat::Mnd, percent: 88.0 }];
    let before_tp = raw_sheet(&tp);
    let before_ws = raw_sheet(&ws);
    let outputs = comparison_outputs();
    let report = reports::comparison_report(&tp, &ws, &catalog, "Savage Blade", "None", &outputs);
    let labels: Vec<_> = report.text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('='))
        .map(|line| line.split("   ").next().unwrap().trim()).collect();
    assert_eq!(labels, ["Attack%", "Magic Damage", "Regain", "STR", "Store TP", "Time per WS", "Savage Blade Damage", "WSC:STR"]);
    assert_eq!(report_values(&report.text, "Attack%"), ["12.5%", "50.0%"]);
    assert_eq!(report_values(&report.text, "Magic Damage"), ["0", "42"]);
    assert_eq!(report_values(&report.text, "Store TP"), ["50", "0"]);
    assert_eq!(report_values(&report.text, "Time per WS"), ["200.501", "200.501"]);
    assert_eq!(report_values(&report.text, "Savage Blade Damage"), ["667", "831"],
        "comparison's integer conversion is not Quicklook's nearest integer formatting");
    assert_eq!(report_values(&report.text, "WSC:STR"), ["10%", "20%"],
        "do not combine WSC entries or look up the WS column by the TP column's stat");
    assert_eq!(raw_sheet(&tp), before_tp);
    assert_eq!(raw_sheet(&ws), before_ws);

    tp.wsc.clear();
    let fallback = reports::comparison_report(&tp, &ws, &catalog, "Savage Blade", "None", &outputs);
    assert_eq!(report_values(&fallback.text, "WSC:INT"), ["0%", "20%"], "empty TP WSC uses the first WS stat as its label");
    ws.wsc.clear();
    let empty = reports::comparison_report(&tp, &ws, &catalog, "Savage Blade", "None", &outputs);
    assert_eq!(report_values(&empty.text, "WSC:"), ["0%", "0%"]);
}

#[test]
fn analytical_precision_uses_raw_damage_tp_and_time_without_overwriting_nonfinite_or_signed_zero() {
    let ws = frozen("combat.json", "combat.000359.ws.Savage_Blade.tp1000.Damage_dealt");
    let round = frozen("combat.json", "combat.018457.round.normal.Time_to_WS");
    let ws = captured_action(&ws["expected"]["value"]["compiled"]["fields"]);
    let round = captured_action(&round["expected"]["value"]["compiled"]["fields"]);
    let display = reports::analytical_display(&ws, false);
    assert_eq!((display.damage.as_str(), display.tp_return.as_str(), display.time_to_ws.as_str()), ("668", "18.3", ""));
    let display = reports::analytical_display(&round, true);
    assert_eq!((display.damage.as_str(), display.tp_return.as_str(), display.time_to_ws.as_str()), ("200.501", "20.0", "200.501"));
    let mut boundary = ws;
    boundary.damage = -0.0;
    boundary.tp_return = -0.0;
    boundary.time_to_ws = Some(-0.0);
    let display = reports::analytical_display(&boundary, false);
    assert_eq!((display.damage.as_str(), display.tp_return.as_str(), display.time_to_ws.as_str()), ("-0", "-0.0", "-0.000"));
    boundary.damage = f64::NAN;
    boundary.tp_return = f64::INFINITY;
    boundary.time_to_ws = Some(f64::NEG_INFINITY);
    let display = reports::analytical_display(&boundary, false);
    assert_eq!((display.damage.as_str(), display.tp_return.as_str(), display.time_to_ws.as_str()), ("nan", "inf", "-inf"));
}

#[test]
fn sampled_trace_keeps_attempts_proc_misses_and_separate_physical_magical_totals() {
    let catalog = catalog();
    let fixture = frozen("combat.json", "combat.019404.ws.Savage_Blade.sample.tp1000.seed11.singleTrue");
    let expected = &fixture["expected"]["value"];
    let player = captured_player(&expected["player_at_action_boundary"], &catalog);
    let compiled = &expected["compiled"];
    let landed: Vec<_> = compiled["events"].as_array().unwrap().iter().filter(|event| event["kind"] == "landed_hit").collect();
    let mut events: Vec<_> = landed.into_iter().map(|event| HitEvent {
        kind: AttackKind::Main, proc: AttackProc::Normal,
        attempt: event["attempted_hits_at_output"].as_u64().unwrap() as u8,
        landed: true, critical: event["crit"].as_bool().unwrap(),
        physical_damage: number(&event["physical_damage"]), magical_damage: number(&event["magical_damage"]),
        tp_return: number(&event["tp_return"]),
    }).collect();
    events.push(HitEvent { kind: AttackKind::Main, proc: AttackProc::DoubleAttack,
        attempt: 3, landed: false, critical: false, physical_damage: 0.0, magical_damage: 0.0, tp_return: 0.0 });
    let fields = &compiled["fields"];
    let sample = WsSample { damage: number(&fields["damage"]), tp_return: number(&fields["tp_return"]),
        physical_damage: number(&fields["physical_damage"]), magical_damage: number(&fields["magical_damage"]) };
    let report = reports::single_ws_report(&player, "Savage Blade", 1000.0, &sample, &events);
    assert!(report.text.lines().any(|line| line.contains("Effective TP") && line.contains("1730.0")),
        "sample header includes the player's TP Bonus, not only the raw fixed-TP entry");
    let numeric_lines: Vec<_> = report.text.lines().filter(|line| line.contains("Phys.]")).collect();
    assert_eq!(numeric_lines.len(), 2, "a miss must not become a landed zero-damage attempt");
    assert!(numeric_lines[0].contains("+1919") && numeric_lines[0].contains("+75.0"));
    assert!(numeric_lines[1].contains("+301") && numeric_lines[1].contains("+10.0"));
    assert_eq!(report.text.matches("Double Attack").count(), 1);
    assert_eq!(report.text.matches("Missed").count(), 1);
    assert!(report.text.lines().any(|line| line.contains("Phys. Damage:") && line.ends_with("2220")));
    assert!(report.text.lines().any(|line| line.contains("Magic Damage:") && line.ends_with('0')));
    assert!(report.text.lines().any(|line| line.contains("TP Returned:") && line.ends_with("85.0")));
    assert!(!report.text.contains("Average Damage"), "sampled results must stay a trace, not analytical labels");
}

#[test]
fn magical_sample_trace_does_not_present_its_total_as_physical_damage() {
    let catalog = catalog();
    let fixture = frozen("combat.json", "combat.019272.ws.Burning_Blade.sample.tp1000.seed11.singleTrue");
    let expected = &fixture["expected"]["value"];
    let player = captured_player(&expected["player_at_action_boundary"], &catalog);
    let event = expected["compiled"]["events"].as_array().unwrap().iter()
        .find(|event| event["kind"] == "landed_hit").unwrap();
    let trace = [HitEvent { kind: AttackKind::Magical, proc: AttackProc::Normal,
        attempt: 1, landed: true, critical: event["crit"].as_bool().unwrap(),
        physical_damage: number(&event["physical_damage"]), magical_damage: number(&event["magical_damage"]),
        tp_return: number(&event["tp_return"]) }];
    let fields = &expected["compiled"]["fields"];
    let sample = WsSample { damage: number(&fields["damage"]), tp_return: number(&fields["tp_return"]),
        physical_damage: number(&fields["physical_damage"]), magical_damage: number(&fields["magical_damage"]) };
    let report = reports::single_ws_report(&player, "Burning Blade", 1000.0, &sample, &trace);
    assert!(report.text.lines().any(|line| line.contains("Magic Hit:") && line.contains("+1087") && line.contains("+75.0")));
    assert!(report.text.lines().any(|line| line.contains("Phys. Damage:") && line.ends_with('0')));
    assert!(report.text.lines().any(|line| line.contains("Magic Damage:") && line.ends_with("1087")));
    assert!(!report.text.contains("Main-hand:"), "a purely magical WS has a distinct event kind");
}

#[test]
fn histogram_title_annotations_and_raw_mean_use_the_same_frozen_distribution() {
    let catalog = catalog();
    let fixture = frozen("search_simulation.json", "search-simulation/distribution20000-magical-constant-host11-compiled20240101");
    let expected = &fixture["expected"]["value"];
    let input = support::parse_input(&fixture["input"]["player"], &catalog,
        &ModelState::new(catalog.pieces().len()), "histogram input");
    let stats: Stats = wsdist_fixtures::from_value(&expected["player"]["stats"]).unwrap();
    let before: Vec<_> = stats.ordered().map(|(stat, value)| (stat, value.to_bits())).collect();
    let summary = &expected["summary"];
    let values = |value: &Value| value.as_array().unwrap().iter().map(number).collect::<Vec<_>>();
    let result = DistributionResult {
        damage: values(&expected["damage_samples"]), tp: values(&expected["tp_samples"]),
        histogram: Histogram {
            counts: wsdist_fixtures::from_value(&summary["histogram_counts"]).unwrap(),
            density: values(&summary["histogram_density"]), edges: values(&summary["histogram_edges"]),
        },
        minimum: number(&summary["minimum"]), mean: number(&summary["mean"]),
        median: number(&summary["median"]), maximum: number(&summary["maximum"]),
        title_mean: number(&summary["title_mean_integer"]), title_median: number(&summary["title_median_integer"]),
    };
    let presentation = reports::histogram_presentation(&input, &stats, &catalog, 1000.0, "Burning Blade", &result);
    let source_title = expected["plot"]["title_left"].as_str().unwrap();
    assert_eq!(presentation.title.join("\n").split_whitespace().collect::<Vec<_>>(), source_title.split_whitespace().collect::<Vec<_>>(),
        "title statistics must use the source's truncated mean/median and displayed job/TP");
    let source_annotations = expected["plot"]["annotations"].as_array().unwrap()[0].as_str().unwrap();
    let annotations: Vec<_> = source_annotations.lines().map(|line| {
        let (label, value) = line.split_once('=').unwrap();
        (label.trim(), value.trim())
    }).collect();
    assert_eq!(presentation.annotations.iter().map(|row| (row.label.as_str(), row.value.as_str())).collect::<Vec<_>>(), annotations,
        "all base parameters and both melee/ranged columns remain available to the plot");
    assert_eq!(presentation.mean_label, expected["plot"]["mean_marker"][0]["label"].as_str().unwrap());
    let report = reports::distribution_report(&result, &presentation);
    assert!(report.text.contains("20000") && report.text.contains("300"));
    assert!(report.text.contains("1435.2000000000003"), "the report cannot replace raw mean with its title integer");
    assert_eq!(stats.ordered().map(|(stat, value)| (stat, value.to_bits())).collect::<Vec<_>>(), before);
}

#[test]
fn optimizer_feasibility_warning_precedes_the_set_and_no_winner_cannot_look_equippable() {
    let catalog = catalog();
    let fixture = frozen("search_simulation.json", "search-simulation/unsatisfied-dt-three-unchanged-stages");
    let input = &fixture["input"];
    let expected = &fixture["expected"]["value"]["result"];
    let player = captured_player(&expected["player"], &catalog);
    let action = ActionRequest::Ws(WsRequest { skill: resolve_name(input["request"]["ws_name"].as_str().unwrap()).unwrap(),
        ws_type: WsType::Melee, tp: number(&input["request"]["min_tp"]), metric: Metric::new("Damage dealt") });
    let request = OptimizeRequest {
        player: support::parse_input(&input["player"], &catalog, &ModelState::new(catalog.pieces().len()), "optimizer report"),
        enemy: wsdist_core::model::Enemy { name: Arc::from("Apex Toad"), location: Arc::from("captured"),
            stats: wsdist_fixtures::from_value(&input["enemy"]["normalized_stats"]).unwrap() },
        action, candidates: std::array::from_fn(|_| Vec::new()),
        ws_name: Arc::from(input["request"]["ws_name"].as_str().unwrap()), spell_name: Arc::from("None"),
        min_pdt: -100.0, min_mdt: -100.0, options: OptimizeOptions::default(),
        similar_results: false, similar_percent: 1.0, verbose_swaps: false,
    };
    let mut result = OptimizeResult {
        gearset: player.gearset, player: Some(player), metric: number(&expected["legacy_output"][0]),
        legacy_output: expected["legacy_output"].as_array().unwrap().iter().map(number).collect(),
        calculation: Vec::new(),
        pdt: -9.0, mdt: -9.0, feasible: false, termination: OptimizeTermination::UnchangedDtStages,
        accepted: Vec::new(), similar: Vec::new(), stages: 4, passes: 4, scored: 0, restarts: 1, invert: 1,
        warnings: Vec::new(),
    };
    let report = reports::optimizer_report(&request, &result, &catalog);
    let warning = report.text.to_ascii_lowercase().find("unsatisfied").expect("infeasible results need a visible warning");
    let gear = report.text.find("Heishi Shorinken").unwrap();
    assert!(warning < gear, "the constraint failure must be visible before an apparently successful set");
    assert!(report.text.contains("PDT:-100") && report.text.contains("MDT:-100"));
    assert!(report.text.contains("PDT:-9") && report.text.contains("MDT:-9"));
    assert!(report.text.contains("673.4") && report.text.contains("45.0"));

    result.player = None;
    result.termination = OptimizeTermination::NoWinner;
    result.metric = 0.0001;
    result.legacy_output.clear();
    let report = reports::optimizer_report(&request, &result, &catalog);
    assert!(report.text.to_ascii_lowercase().contains("no winning player"));
    assert!(report.text.contains("0.0001"), "the no-winner sentinel is not a fabricated zero-damage success");
    assert!(!report.text.contains("Heishi Shorinken"), "no winning player means no presented equippable set");
}
