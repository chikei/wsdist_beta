// Process-wide allocator for this verification-only, allocation-heavy corpus.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
#[path = "../tests/native_support/mod.rs"]
#[allow(dead_code)]
mod native_support;
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

use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const USAGE: &str = "Usage: verify_numerics --fixtures <legacy-root> --report <json-path>\n\nReplays every frozen core case, the captured UI numerical inputs, complete\noptimizer searches, 7200-second DPS trajectories and 20000-sample distributions.\nAlso runs independent diagnostics and 18 million-sample isolated sampler checks.\nExit codes: 0 all gates pass; 1 numerical/fixture failure; 2 argument/report I/O error.";

fn arguments() -> Result<Option<(PathBuf, PathBuf)>, String> {
    let mut arguments = std::env::args_os().skip(1);
    let mut fixtures = None;
    let mut report = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--fixtures") if fixtures.is_none() => fixtures = Some(PathBuf::from(arguments.next().ok_or("--fixtures needs a directory")?)),
            Some("--report") if report.is_none() => report = Some(PathBuf::from(arguments.next().ok_or("--report needs a file path")?)),
            _ => return Err(format!("unexpected or repeated argument {:?}", argument)),
        }
    }
    Ok(Some((fixtures.ok_or("--fixtures is required")?, report.ok_or("--report is required")?)))
}

fn toolchain() -> Value {
    match Command::new("rustc").args(["--version", "--verbose"]).output() {
        Ok(output) if output.status.success() => json!({"queried_command": "rustc --version --verbose", "output": String::from_utf8_lossy(&output.stdout)}),
        Ok(output) => json!({"queried_command": "rustc --version --verbose", "unavailable": String::from_utf8_lossy(&output.stderr), "exit_status": output.status.code()}),
        Err(error) => json!({"queried_command": "rustc --version --verbose", "unavailable": error.to_string()}),
    }
}

fn report_file(path: &std::path::Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let file = std::fs::File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|error| format!("{}: {error}", path.display()))?;
    writer.write_all(b"\n").and_then(|_| writer.flush()).map_err(|error| format!("{}: {error}", path.display()))
}

fn run(fixtures: PathBuf, report_path: PathBuf) -> ExitCode {
    let timer = Instant::now();
    let corpus = verification::verify_corpus(&fixtures);
    let mut failing = corpus.failing_case_paths.clone();
    let independent = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| verification::independent::diagnostics(&fixtures))) {
        Ok(rows) => json!({"passed": true, "completed": true, "checks": rows}),
        Err(error) => {
            let failure = verification::panic_message(error);
            failing.push(json!({"path": "independent.diagnostics", "diagnostic": failure}));
            json!({"passed": false, "completed": false, "failure": failure})
        }
    };
    let statistical = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| verification::independent::statistics(&fixtures))) {
        Ok(rows) => {
            let passed = rows.iter().all(|row| row["passed"] == true);
            for row in rows.iter().filter(|row| row["passed"] != true) {
                failing.push(json!({"path": format!("independent.statistics.{}", row["id"].as_str().unwrap()), "diagnostic": row}));
            }
            let observed_n = rows.iter().filter_map(|row| row["n"].as_u64()).sum::<u64>();
            json!({"passed": passed, "observed_samples": observed_n, "scenarios": rows,
                "seed_contract": {"generator": "ChaCha8Rng", "seed": "20240101+i", "primary_stream": 0, "pdif_stream": 1, "i": [0, 39]},
                "gate": "mean: max(1e-9,6*sample_stddev(batch_means)/sqrt(40)); categorical: 6*sqrt(p*(1-p)/1000000), endpoints exact"})
        }
        Err(error) => {
            let failure = verification::panic_message(error);
            failing.push(json!({"path": "independent.statistics", "diagnostic": failure}));
            json!({"passed": false, "completed": false, "failure": failure})
        }
    };
    let passed = corpus.passed && independent["passed"] == true && statistical["passed"] == true;
    let root = verification::resource_root();
    let delivery_hashes = ["Cargo.lock", "rust-toolchain.toml"].into_iter().map(|name| {
        let value = std::panic::catch_unwind(|| verification::catalog::hash_file(&root.join(name)))
            .map(|hash| json!({"sha256": hash})).unwrap_or_else(|error| json!({"unavailable": verification::panic_message(error)}));
        (name.to_owned(), value)
    }).collect::<serde_json::Map<_, _>>();
    let report = json!({
        "schema_version": 1,
        "passed": passed,
        "started_unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_secs_f64() - timer.elapsed().as_secs_f64()).ok(),
        "elapsed_seconds": timer.elapsed().as_secs_f64(),
        "platform": {"os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY, "pointer_width": usize::BITS,
            "endianness": if cfg!(target_endian = "little") { "little" } else { "big" }},
        "toolchain": {"active_rustc": toolchain(), "build_rustup_toolchain": option_env!("RUSTUP_TOOLCHAIN"),
            "declared_minimum_rust": env!("CARGO_PKG_RUST_VERSION"), "package_version": env!("CARGO_PKG_VERSION"),
            "note": "active_rustc is queried on this machine; only the platform above is exercised by this invocation."},
        "delivery_hashes": delivery_hashes,
        "fixtures": fixtures.display().to_string(),
        "provenance": corpus.provenance,
        "canonical_parity": corpus,
        "exact_and_tape_results": {"passed": corpus.passed,
            "recorded_actions": corpus.observed_work.recorded_actions,
            "scripted_actions": corpus.observed_work.scripted_actions,
            "strict_kernel_tape_rows": corpus.observed_work.strict_kernel_tape_rows,
            "note": "Legacy combat and kernel results replay frozen Python draws; native simulation and search adapters compare Rust fixed-seed goldens. Case statuses and diagnostics are in canonical_parity.cases."},
        "independent_discrepancies": independent,
        "statistical_results": statistical,
        "failing_case_paths": failing,
        "acceptance": {"continuous_absolute_tolerance": 1e-9, "continuous_relative_tolerance": 1e-12,
            "measured_error_scope": "Maximum errors cover finite continuous fields actually compared by the adapters; integer/exact and nonfinite failures remain separate assertions and counters. uniform_draws counts combat/simulation draws; kernel tapes and optimizer replay are separately covered.",
            "exact": ["integer stages", "identities", "presence and order", "exceptions", "random parameters and variates",
                "accepted optimizer candidates", "final gear", "attempt/proc decisions", "histogram counts", "signed zero and nonfinite classes"],
            "tape": "stream, operation, parameter bits, returned variate, order and complete consumption; no generator fallback"},
        "interpretation": "Legacy compatibility and independent internal diagnostics are not validation against live-game observations. Expected analytical/sampled discrepancies are reported, never repaired. UI/controller/native-platform proof belongs to the separate application gate."
    });
    if let Err(error) = report_file(&report_path, &report) {
        eprintln!("Cannot write numerical report: {error}");
        return ExitCode::from(2);
    }
    let parity = &report["canonical_parity"];
    eprintln!("{}: {} exercised cases; {} inert evidence cases; {} failed cases; {} DPS trajectories; {} distribution samples; statistical samples {}. Report: {}",
        if passed { "PASS" } else { "FAIL" }, parity["exercised_cases"], parity["evidence_only_cases"], parity["failed_cases"],
        parity["observed_work"]["dps_trajectories"], parity["observed_work"]["distribution_samples"], report["statistical_results"]["observed_samples"], report_path.display());
    if !passed {
        for failure in report["failing_case_paths"].as_array().unwrap() { eprintln!("{}: {}", failure["path"], failure["diagnostic"]); }
    }
    if passed { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn main() -> ExitCode {
    match arguments() {
        Ok(Some((fixtures, report))) => run(fixtures, report),
        Ok(None) => { println!("{USAGE}"); ExitCode::SUCCESS }
        Err(error) => { eprintln!("{error}\n\n{USAGE}"); ExitCode::from(2) }
    }
}
