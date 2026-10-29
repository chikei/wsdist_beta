#[path = "../tests/legacy_player/support.rs"]
#[allow(dead_code)]
mod player_support;
#[path = "../tests/numerical_boundaries/support.rs"]
#[allow(dead_code)]
mod kernel_support;
#[path = "../tests/combat_support/mod.rs"]
#[allow(dead_code)]
mod combat_support;
#[path = "../tests/ws_support/mod.rs"]
#[allow(dead_code)]
mod ws_support;
#[path = "../tests/buffs_support/mod.rs"]
#[allow(dead_code)]
mod buffs_support;
#[path = "../tests/search_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../tests/optimizer_support/mod.rs"]
#[allow(dead_code)]
mod optimizer_support;
#[path = "../tests/simulation_support/mod.rs"]
#[allow(dead_code)]
mod simulation_support;
#[path = "../tests/support/mod.rs"]
#[allow(dead_code)]
mod verification;
#[path = "../tests/native_support/mod.rs"]
#[allow(dead_code)]
mod native_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rayon::prelude::*;
use serde_json::{Value, json};
use wsdist_core::actions::sample_ws;
use wsdist_core::model::{EvaluationContext, Metric, WsRequest, WsType};
use wsdist_core::weaponskills::resolve_name;

use native_support::{CountingRandom, random_value};
use support::{FrozenCatalog, IndexedCase};

const USAGE: &str = "Usage: regenerate_native_goldens --legacy <legacy-root> --out <native-root>";
const COMBAT_ID: &str = "combat.019404.ws.Savage_Blade.sample.tp1000.seed11.singleTrue";

fn arguments() -> Result<Option<(PathBuf, PathBuf)>, String> {
    let mut arguments = std::env::args_os().skip(1);
    let mut legacy = None;
    let mut out = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--legacy") if legacy.is_none() => legacy = Some(PathBuf::from(arguments.next().ok_or("--legacy needs a directory")?)),
            Some("--out") if out.is_none() => out = Some(PathBuf::from(arguments.next().ok_or("--out needs a directory")?)),
            _ => return Err(format!("unexpected or repeated argument {argument:?}")),
        }
    }
    Ok(Some((legacy.ok_or("--legacy is required")?, out.ok_or("--out is required")?)))
}

fn observe_combat_ws(case: &Value, cat: &FrozenCatalog) -> Value {
    let mut context = EvaluationContext::new(cat.catalog.clone(), CountingRandom::seeded(11));
    let player = combat_support::build_boundary(case, &mut context).unwrap();
    let enemy = combat_support::boundary_enemy(case);
    let action = &case["input"]["action"];
    let request = WsRequest {
        skill: resolve_name(action["name"].as_str().unwrap()).unwrap(),
        ws_type: WsType::from_name(action["ws_type"].as_str().unwrap()).unwrap(),
        tp: support::number(&action["tp"]), metric: Metric::new(action["metric"].as_str().unwrap()),
    };
    context.action_trace = Some(Vec::new());
    let result = sample_ws(&player, &enemy, &request, true, &mut context).unwrap();
    json!({"result": result, "hits": context.action_trace.as_ref().unwrap(), "random": random_value(&context.random)})
}

fn observation(case: &Value, cat: &FrozenCatalog) -> Value {
    macro_rules! observe {
        ($module:ident, $run:ident, $assert:ident, $project:ident) => {{
            let run = $module::$run(case, cat);
            $module::$assert(case, &run, cat);
            $module::$project(&run, cat)
        }};
    }
    match case["operation"].as_str().unwrap() {
        "optimizer_neighborhood" => observe!(optimizer_support, run_neighborhood, assert_neighborhood_legacy, project_neighborhood),
        "optimizer_pass" => observe!(optimizer_support, run_pass, assert_pass_legacy, project_pass),
        "optimizer_build" => observe!(optimizer_support, run_build, assert_build_legacy, project_build),
        "optimizer_sequence" => observe!(optimizer_support, run_sequence, assert_sequence_legacy, project_sequence),
        "simulation_dps" => observe!(simulation_support, run_dps, assert_dps_legacy, project_dps),
        "simulation_distribution" => observe!(simulation_support, run_distribution, assert_distribution_legacy, project_distribution),
        "simulation_nonprogress" => observe!(simulation_support, run_nonprogress, assert_nonprogress_legacy, project_nonprogress),
        "combat.ws" => observe_combat_ws(case, cat),
        operation => panic!("unsupported native golden operation {operation}"),
    }
}

fn seed(case: &Value) -> Value {
    match case["operation"].as_str().unwrap() {
        "combat.ws" => case["input"]["random"]["numpy_host_seed"].clone(),
        "optimizer_sequence" => Value::Array(case["input"]["steps"].as_array().unwrap().iter()
            .map(|step| step["seeds"]["numpy_host"].clone()).collect()),
        _ => case["input"]["seeds"]["numpy_host"].clone(),
    }
}

fn compute(legacy: &Path, entry: &IndexedCase) -> Result<(Value, Vec<u8>), String> {
    let case = wsdist_fixtures::read_value(&legacy.join(&entry.path), Some(&entry.content_xxh3_128))
        .map_err(|error| error.to_string())?;
    assert_eq!(case["id"], entry.id);
    assert_eq!(case["operation"], entry.operation);
    let cat = FrozenCatalog::from_input(&case["input"]);
    let doc = json!({
        "id": entry.id, "operation": entry.operation,
        "legacy_case": {"path": entry.path, "sha256": entry.source_sha256},
        "seed": seed(&case), "observation": observation(&case, &cat),
    });
    let mut bytes = serde_json::to_vec(&doc).unwrap();
    bytes.push(b'\n');
    let ordinal = Path::new(&entry.path).file_name().unwrap().to_str().unwrap();
    let path = format!("{}/{ordinal}", entry.operation);
    let index_entry = json!({
        "id": entry.id, "operation": entry.operation, "path": path,
        "content_xxh3_128": format!("{:032x}", wsdist_fixtures::content_hash(&bytes)),
        "legacy_path": entry.path, "legacy_sha256": entry.source_sha256,
    });
    Ok((index_entry, bytes))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn run(legacy: PathBuf, out: PathBuf) -> Result<(), String> {
    if out.try_exists().map_err(|error| format!("{}: {error}", out.display()))? {
        return Err(format!("Refusing to overwrite existing native goldens: {}", out.display()));
    }
    let search: support::FixtureIndex = wsdist_fixtures::read(&legacy.join("search_simulation.json"), None)
        .map_err(|error| error.to_string())?;
    let mut cases: Vec<_> = search.cases.into_iter().filter(|entry| matches!(entry.operation.as_str(),
        "optimizer_neighborhood" | "optimizer_pass" | "optimizer_build" | "optimizer_sequence" |
        "simulation_dps" | "simulation_distribution" | "simulation_nonprogress")).collect();
    let combat: support::FixtureIndex = wsdist_fixtures::read(&legacy.join("combat.json"), None)
        .map_err(|error| error.to_string())?;
    cases.push(combat.cases.into_iter().find(|entry| entry.id == COMBAT_ID).ok_or("missing native combat WS source case")?);
    assert_eq!(cases.len(), 137, "native golden case selection");

    // Indexed parallel collection preserves legacy index order. Files are written
    // only after every execution and generator-independent Python check succeeds.
    let computed: Vec<_> = cases.par_iter().map(|entry| compute(&legacy, entry)).collect::<Result<_, _>>()?;
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::create_dir(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut entries = Vec::with_capacity(computed.len());
    for (entry, bytes) in computed {
        let path = out.join(entry["path"].as_str().unwrap());
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        write_file(&path, &bytes)?;
        println!("{} -> {} ({} bytes)", entry["id"].as_str().unwrap(), path.display(), bytes.len());
        entries.push(entry);
    }
    let index = json!({
        "schema_version": 2,
        "generator": "rand_chacha 0.9.0 ChaCha8Rng",
        "seed_layout": "u64 little-endian in key bytes 0..8, zero bytes 8..32; Primary stream 0, Pdif stream 1",
        "cases": entries,
    });
    let mut bytes = serde_json::to_vec(&index).unwrap();
    bytes.push(b'\n');
    write_file(&out.join("index.json"), &bytes)?;
    println!("Generated 137 native goldens in {}", out.display());
    Ok(())
}

fn main() -> ExitCode {
    match arguments() {
        Ok(None) => { println!("{USAGE}"); ExitCode::SUCCESS }
        Ok(Some((legacy, out))) => match run(legacy, out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => { eprintln!("{error}"); ExitCode::from(2) }
        },
        Err(error) => { eprintln!("{error}\n{USAGE}"); ExitCode::from(2) }
    }
}
