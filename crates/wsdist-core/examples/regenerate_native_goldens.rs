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
use sha2::{Digest, Sha256};
use wsdist_fixtures::{Value, to_value};
use wsdist_core::actions::sample_ws;
use wsdist_core::model::{EvaluationContext, Metric, WsRequest, WsType};
use wsdist_core::weaponskills::resolve_name;

use native_support::{CountingRandom, object, random_value};
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
    object([
        ("result", to_value(result).unwrap()),
        ("hits", to_value(context.action_trace.as_ref().unwrap()).unwrap()),
        ("random", random_value(&context.random)),
    ])
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

struct ComputedGolden<'a> {
    source: &'a IndexedCase,
    document: Value,
    path: String,
    source_path: String,
    source_sha256: String,
    source_content_xxh3_128: String,
}

fn compute<'a>(legacy: &Path, entry: &'a IndexedCase) -> Result<ComputedGolden<'a>, String> {
    let case = wsdist_fixtures::read_value(&legacy.join(&entry.path), Some(&entry.content_xxh3_128))
        .map_err(|error| error.to_string())?;
    assert_eq!(case["id"], entry.id);
    assert_eq!(case["operation"], entry.operation);
    let cat = FrozenCatalog::from_input(&case["input"]);
    let document = object([
        ("id", Value::from(entry.id.as_str())),
        ("operation", Value::from(entry.operation.as_str())),
        ("legacy_case", object([
            ("path", Value::from(entry.source_path.as_str())),
            ("sha256", Value::from(entry.source_sha256.as_str())),
        ])),
        ("seed", seed(&case)),
        ("observation", observation(&case, &cat)),
    ]);
    let mut source_bytes = serde_json::to_vec(&document).unwrap();
    source_bytes.push(b'\n');
    let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes));
    let source_content_xxh3_128 = format!("{:032x}", wsdist_fixtures::content_hash(&source_bytes));
    let ordinal = Path::new(&entry.source_path).file_stem().unwrap().to_str().unwrap();
    Ok(ComputedGolden {
        source: entry,
        document,
        path: format!("{}/{ordinal}.bitcode.zst", entry.operation),
        source_path: format!("{}/{ordinal}.json", entry.operation),
        source_sha256,
        source_content_xxh3_128,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn run(legacy: PathBuf, out: PathBuf) -> Result<(), String> {
    if out.try_exists().map_err(|error| format!("{}: {error}", out.display()))? {
        return Err(format!("Refusing to overwrite existing native goldens: {}", out.display()));
    }
    let search: support::FixtureIndex = wsdist_fixtures::read_json(&legacy.join("search_simulation.json"))
        .map_err(|error| error.to_string())?;
    assert_eq!(search.schema_version, 3, "search fixture storage schema");
    let mut cases: Vec<_> = search.cases.into_iter().filter(|entry| matches!(entry.operation.as_str(),
        "optimizer_neighborhood" | "optimizer_pass" | "optimizer_build" | "optimizer_sequence" |
        "simulation_dps" | "simulation_distribution" | "simulation_nonprogress")).collect();
    let combat: support::FixtureIndex = wsdist_fixtures::read_json(&legacy.join("combat.json"))
        .map_err(|error| error.to_string())?;
    assert_eq!(combat.schema_version, 3, "combat fixture storage schema");
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
    for computed in computed {
        let path = out.join(&computed.path);
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        let content_xxh3_128 = wsdist_fixtures::write_value(&path, &computed.document).map_err(|error| error.to_string())?;
        let bytes = fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?.len();
        println!("{} -> {} ({} bytes)", computed.source.id, path.display(), bytes);
        entries.push(object([
            ("id", Value::from(computed.source.id.as_str())),
            ("operation", Value::from(computed.source.operation.as_str())),
            ("path", Value::String(computed.path)),
            ("source_path", Value::String(computed.source_path)),
            ("source_sha256", Value::String(computed.source_sha256)),
            ("source_content_xxh3_128", Value::String(computed.source_content_xxh3_128)),
            ("content_xxh3_128", Value::String(content_xxh3_128)),
            ("legacy_path", Value::from(computed.source.source_path.as_str())),
            ("legacy_sha256", Value::from(computed.source.source_sha256.as_str())),
        ]));
    }
    let index = object([
        ("schema_version", Value::from(3_u32)),
        ("generator", Value::from("rand_chacha 0.9.0 ChaCha8Rng")),
        ("seed_layout", Value::from("u64 little-endian in key bytes 0..8, zero bytes 8..32; Primary stream 0, Pdif stream 1")),
        ("cases", Value::Array(entries)),
    ]);
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
