# Verification

## Commands and native prerequisites

Use the pinned Rust **1.92.0** toolchain, edition-2024 workspace and committed
`Cargo.lock`. Verification consumes immutable JSON evidence; no Python runtime,
fixture regeneration or golden acceptance is part of a build.

```sh
cargo test --locked -p wsdist-core
cargo test --locked -p wsdist-app -- --test-threads=1
cargo run --locked --release -p wsdist-core --example verify_numerics -- \
  --fixtures crates/wsdist-core/tests/fixtures/legacy \
  --report target/numerical-report.json
cargo build --locked --release -p wsdist-app --bin wsdist-gui
```

Linux needs a C/C++ linker, pkg-config, fontconfig/FreeType development files,
X11/Wayland/xkbcommon libraries and standard fonts. Displayless native proof
also needs Xvfb/xauth; archive automation needs Xlib/libpng development headers,
`cc`, `jq` and `dbus-run-session`. Interactive file selection requires an XDG
Desktop Portal backend such as GTK, GNOME or KDE. Windows requires the native
x64 MSVC Rust build environment and an interactive desktop session.

Current integrated Linux results: **109 core tests** and **60 app tests** passed.
The release verifier passed **30,192 exercised cases**, one explicitly inert
provenance case, and zero failed cases. Completed work includes 20 full DPS
trajectories, 12 full distributions / 240,000 samples, 30 complete optimizer
builds, three public seed-11 optimizer runs (workers 1/2/auto), 577 profile player
builds and 18 million isolated statistical samples. These are observed work
counts, not just fixture-index lengths. The JSON report records the native
platform, toolchain/provenance hashes, individual case paths, actual comparison
counts, maximum errors, exact/tape checks, statistics and failures.
Observed comparisons: **21,214,649** total, including 19,138,826 exact numeric,
2,075,259 continuous and 564 nonfinite checks. Maximum continuous absolute
error was `1.1368683772161603e-13`; maximum tolerance fraction was
`0.00007450451485247618`. Both maxima come from complete DPS trajectories.


Windows execution was explicitly waived for the local cutover. Windows build,
parity, real-backend UI and extracted-archive gates remain in CI; no local
Windows runtime result is claimed.

## Frozen reference and acceptance

The reviewed snapshot/capture artifacts live under `target/migration/` and are
not shipped or maintained as a second application. The canonical oracle used
CPython 3.13.6, NumPy 2.4.6, Numba 0.65.1, llvmlite 0.47.0, Matplotlib 3.10.9
and PySide6 6.11.1. The source/runtime manifests, immutable core corpus and
GUI subset are under the two crates' `tests/fixtures/legacy/` directories.
They record source hashes, complete inputs, intermediate/effective state,
ordered random calls, numerical outcomes and original exceptions.

After the required parity and Linux delivery gates passed, cutover removed all
36 superseded Python application/test modules, `.python-version`,
`pyrightconfig.json`, `requirements.txt`, `uv.lock` and `defaults.pkl`.
The source/data checksum guard verified all 41 removal candidates against the
reviewed snapshot before deletion. Existing `defaults.json`, CSV, all 1,231
icons, `.gitattributes` and `.venv/pyvenv.cfg` remained byte-identical; user
environment/editor directories and unrelated files were left in place.
Temporary chooser/isolation proof drivers were removed after proof; their JSON,
inventories, process maps and screenshots remain local evidence.

- Integer stages, identities, sparse presence/order, branch and proc decisions,
  exceptions, accepted optimizer candidates, final equipment and histogram
  counts must match exactly.
- Continuous binary64 fields use
  `abs(actual - expected) <= 1e-9 + 1e-12 * max(abs(actual), abs(expected))`.
  This does not permit a different truncation boundary, displayed rounding or
  winning set. Signed zero and nonfinite classes retain separate checks.
- Strict tapes check stream, operation, parameter bits, variate, order and full
  consumption; they never fall back to a generator.
- The report's maximum-error scope is the fields actually compared by adapters.
  Exact/nonfinite assertions and RNG/search coverage are recorded separately;
  an inert controller/provenance record is not claimed as numerical execution.

Do not widen tolerances, regenerate fixtures to pass a mismatch, or correct a
known legacy numerical discrepancy. Compatibility and independent internal
checks do not establish accuracy against live-game observations.

## Coverage

### Catalog, model, kernels and actions

Catalog checks compare complete materialized records, identity-based IDs,
ordered/repeated slot/food membership, duplicate-name resolution, optional
rank/WSC, literal numeric values, CSV aliases and asset hashes. The gear-stat
allowlist applies to final slot/food members as in the original validator;
declared-only records retain known internal identities without normalization.

Stat storage retains presence and insertion order across 294 known identities.
Player cases cover ordered base/gear/buff/final construction, every captured
job/subjob/ML and single-piece/profile build, plus shared-versus-isolated H2H
metadata transitions. Integer/float/bool Aftermath kinds remain distinct.
Python and NumPy one-decimal rounding have separate implementations; a
supplemental pinned-runtime probe matched both bit-for-bit for 20,470 finite
inputs across all binary64 exponents, signed zeros and scaled overflow.

Permanent suites include `legacy_player`, `legacy_ws`, `buff_effects`,
`numerical_boundaries`, `rng_compat`, `legacy_round`, `legacy_spell`,
`legacy_ws_action` and the complete `legacy_parity` driver. Action coverage
includes all 816 round, 9,565 spell and 12,812 WS cases and 112 strict scripted
counterparts. All 184 WS specifications and literal setup/hooks are checked.
GUI and optimizer spell dispatch remain intentionally distinct.

Independent NumPy-host and Numba-Python MT19937 states match 368 direct
operations, 12,547 tempered words, 12 expected errors and eight twist-crossing
runs. Singleton choice/rejection behavior and unused draws are preserved;
blank production seeds use OS entropy.

### Ordered optimizer and full simulations

`optimizer_behavior`, `simulation_behavior` and `simulation_statistics` replay
432 search/simulation cases: 399 optimizer cases, 20 complete DPS trajectories,
one nonprogress diagnostic and 12 complete distributions. Checks include
ordered combinations, full-value gear equality, legality, first strict
improvement, DT stages, restarts/ties, worker/coordinator contexts, every
captured draw and final stream continuation.

The complete seed-11 fixture selects Hattori Zukin +2, Hattori Tekko +2 and
Hattori Kyahan +2, with metric `3244.8594084927004` and five accepted swaps.
The helper-only sweep starts at another RNG position: Ryuo Somen +1 A,
`3071.314537892103`, six accepts. These are not interchangeable goldens.
Unsatisfied DT/no-winner results and zero-time inverse-metric errors remain
explicit, not fabricated feasible/infinite winners.

DPS finishes its final TP/WS cycle after the 7200-second horizon, retaining
unclamped TP, set-dependent timing and one-decimal Regain. Distributions retain
all 20000 ordered samples and 300 density bins. NumPy-compatible pairwise means,
edge correction, constant-data expansion, median and title truncation are
checked. A zero threshold remains WS-only with explicit NaN TP averages.
Cancellation publishes no partial result; only a proven nonpositive TP upper
bound triggers nonprogress.

### Independent mathematical diagnostics

Test-side integration/enumeration does not call production expectation
formulas: it conditions critical pDIF, integrates clipped uniforms, enumerates
four resist states and priority proc trees up to eight attempts, and applies
staged physical/Ninjutsu truncation. Expected discrepancies include:

- Capped Sword ratio 3.25: sampled expectation `3.23515625`, legacy analytical
  `3.33125`.
- Conserve TP: sampled integers 10…199 mean `104.5`, analytical `95`.
- Fotia at 1000 TP: sampled gains 10/20 for one/both pieces, analytical 0/0.1.
- Eight-attempt sampling versus the analytical expected-landed-hit approximation.

Each of 18 isolated legacy-sampler scenarios runs 40 batches of 25,000 samples.
Batch `i=0..39` uses host seed `20240101+i` and compiled-pDIF seed `40480202+i`.
Means must meet `max(1e-9, 6 * standard_error)`; categorical frequencies meet
`6 * sqrt(p * (1-p) / 1000000)`, with p=0/1 exact. The report retains sample
counts, seeds, batch variances, errors, expectations and pass/fail gates.
Full mixed-combat seeded/tape parity remains separately required.

### Profiles, exchange and controller state

`profile_workflows`, `ui_workflows` and the companion app suites use the actual
state/controller/engine. All 23 shipped profiles, 134-key registry types/order,
2420 job/subjob/ML availability rows and 1276 bulk-selection rows are covered.
Checks include effective cached buffs/inputs, three independent sets, hidden
selection retention, legality/pair swaps, missing keys/items and literal menus.

Saving atomically replaces same-directory JSON only after serialization,
flush and sync; independent unknown keys and other jobs survive. String `"0"`
retains Python truthiness; saved effective checkboxes are booleans. Corrupt
external profiles block saving rather than being overwritten. Job activation
re-reads the current profile file. Reload validates the complete JSON/CSV set
before committing, remaps by name2, reports removals and resets H2H overrides.
Cancelled/unreadable imports retain candidates; completed empty/unrecognized
imports clear them. Native clipboard ownership survives exports; all 16 typed
base-name/CSV lines come from the actual Quicklook, TP or WS source.

## Real-backend UI proof

```sh
WSDIST_DATA_DIR="$PWD" SLINT_BACKEND=winit-software SLINT_EMIT_DEBUG_INFO=1 \
  cargo run --locked -p wsdist-app --example ui_smoke -- --output target/ui-smoke-100
WSDIST_DATA_DIR="$PWD" SLINT_BACKEND=winit-software SLINT_EMIT_DEBUG_INFO=1 \
  SLINT_SCALE_FACTOR=1.5 target/debug/examples/ui_smoke --output target/ui-smoke-150
```

For displayless Linux prefix each command with `xvfb-run -a`; use a screen large
enough for scaled windows, for example `-s '-screen 0 2560x1800x24'`. Windows
PowerShell sets the corresponding environment variables and runs the same
Cargo command without the POSIX prefix. The driver uses the real Winit/software
backend, never the testing backend, and temporary resources for save/reload.

The 100% and 150% Linux runs passed. Native screenshots were inspected for all
four tabs, generated variants/details, numerical errors, result windows, Stats,
DPS curves, histogram, AboutSlint and compact/expanded layouts. The driver checks
fixture-backed displayed analytical values and Stats, every buff group's
effective numbers, native TP click/typing/Return/wheel and q protection, all
copy/clipboard sources, inventory cancellation/read failure/matching, save and
native relaunch, actual-item JSON edit/reload, seeded optimization/Equip best,
full DPS and 20000-sample histogram, comparison, busy-input rejection and Close
joining active workers. Scalar groups use exact-height scrollable columns;
large equipment models remain virtualized.

The Slint 1.16 software snapshot path leaves its unused alpha channel at zero.
The driver sets alpha opaque while preserving every rendered RGB byte; this is
not numerical/image golden rewriting. Each `report.json` records screenshot
filenames, dimensions, actual scale and exercised workflows.

A separate native GTK portal proof passed using the production inventory action,
a painted chooser owned by the real portal process, and native XTest keyboard
events. Escape retained exact candidate IDs/order across all 16 slots; accepting
a real temporary inventory file returned both main-hand Heishi variants and
Hattori Zukin +2, with every other slot empty. Heishi is absent from the exported
sub-slot list. Evidence and chooser screenshots:
`target/native-chooser-reactive/report.json`. Its snapshots also verify the
details pane follows in-place model-row updates: bulk selection/cancellation
retains and displays 30 main candidates, then import displays two main and one
head candidate without stale copied inspector strings.

The permanent driver still leaves OS-dialog automation to a desktop checklist.
On other desktop backends/Windows, open **Optimize > Select all File**, verify
filters, cancel without losing checks, then accept an inventory export. A
missing portal/silent cancel is not proof of a working chooser.

## Native packaging and extracted launch

```sh
cargo +1.92.0 install --locked cargo-about --version 0.8.2
bash tools/package-linux.sh --output target/dist
xvfb-run -a -s '-screen 0 2560x1800x24' dbus-run-session -- \
  bash tools/smoke-linux-archive.sh \
  --archive target/dist/wsdist-linux-x86_64.tar.gz \
  --output target/archive-smoke/linux-100 --scale 1 \
  --ui-smoke target/debug/examples/ui_smoke
```

Repeat with `--scale 1.5` and `--output target/archive-smoke/linux-150`.
Packaging builds the explicit native target; `--skip-build` requires an already
built `target/x86_64-unknown-linux-gnu/release/wsdist-gui`. Windows uses
`tools/package-windows.ps1` and `tools/smoke-windows-archive.ps1`; static CRT
linkage is requested consistently by CI/packaging.

Archives contain only the executable, inert catalogs, profiles, CSV, icons and
third-party/provenance notices. Local application crates are unpublished and
excluded from third-party license resolution, not assigned an invented license.
The pinned notice tool scans locked target-filtered dependencies offline after
source caches are populated. Slint's full custom royalty-free license is
included explicitly because cargo-about cannot render its LicenseRef text;
original crate notices, asset/font licenses and Rust standard-library notices
are retained. No new license grant is asserted for repository code/data/icons.

Archive smoke extracts into a temporary directory, unsets the data override,
launches the actual release executable from an unrelated empty CWD, captures a
painted native client frame, rejects startup-error/blank surfaces and Python/Qt
loaded modules, closes through native window messaging and requires clean exit.
The optional development driver is copied beside resources only after archive
hygiene checks; it is not shipped. Its full workflows then run with the same
executable-relative resource boundary. Packaging alone is not execution proof.

CI runs both native targets, locked core/app tests, the full numerical verifier,
release GUI, real-backend 100%/150% UI proof, packaging and extracted-archive
smokes. Failed gates retain evidence but never publish a release archive.

A separate Linux proof ran the final archive's production GUI and complete UI
workflows at both 100% and 150% scale in an isolated filesystem with no Python or
Qt interpreter, library or source tree. Both processes exited zero at each
scale. The inventory recorded 2,082 staged files, 48 native ELF libraries and all
1,237 release resources; the production executable ran as
`/opt/wsdist/wsdist-gui` from `/unrelated working directory`, with
`WSDIST_DATA_DIR` absent. Only the UI driver received two read-only frozen JSON
fixture mounts; no broad repository or source mount was provided.
Archive SHA-256:
`6e0ae15d9ff8fb1214243f7c8a474e3b0841861f47e39c1176538c51c610f8f6`.
Evidence:
`target/migration/absent-runtime-reactive-100/report.json` and
`target/migration/absent-runtime-reactive-150/report.json`, with native screenshots,
process maps, mount inventories and executable/CWD observations beside them.

Post-cutover extracted-archive execution also passed with the permanent UI
driver: 23 workflow records and 45 native screenshots, production GUI launch
from an unrelated empty CWD, executable-relative resources and clean exit.
Evidence: `target/archive-smoke/post-cutover-linux-100/report.json` and
`target/archive-smoke/post-cutover-linux-100/ui-workflows/report.json`.
