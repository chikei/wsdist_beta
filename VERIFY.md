# Verification

## Commands and native prerequisites

Use the pinned Rust **1.92.0** toolchain, edition-2024 workspace and committed
`Cargo.lock`. Verification consumes immutable fixture evidence; no Python runtime,
fixture regeneration or golden acceptance is part of a build.

```sh
cargo test --locked -p wsdist-core --test formula_boundaries
cargo test --locked -p wsdist-core --test rng_protocol
cargo test --locked -p wsdist-core
env -u SLINT_SCALE_FACTOR cargo test --locked -p wsdist-app -- --test-threads=1
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

Original cutover Linux results: **109 core tests** and **61 app tests** passed.
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

### Native RNG migration proof

The ChaCha8 cutover passed **136 core tests** and **68 app tests** on Linux,
including `fixture_storage`, the bit-exact Python formula-boundary gate, native
protocol pins, worker invariance, cancellation and trace-on/off checks. The
release numerical verifier passed **30,184 exercised cases**, one inert
provenance case and zero failures, with all **18 statistical scenarios**
(18,000,000 samples) passing unchanged distribution gates.

`target/numerical-report.json` records 23,193 `recorded_actions`, 112 scripted
action replays and 10,053 strict kernel tape rows. It has no `direct_mt_*` or
`seeded_*` keys; `statistical_results.seed_contract.generator` is `ChaCha8Rng`.
All 137 native goldens were regenerated twice with no byte differences.
No production formula changes were needed to match the Python boundaries.

The complete real-backend UI smoke passed at 100% scale using Xvfb and the
native focus helper. `target/ui-smoke-native-rng-fixed/report.json` records
165 screenshots and the seed-11 optimizer, actual **Equip best set**, full
7200-second DPS and 20000-sample / 300-bin histogram workflows. Optimizer,
DPS and histogram surfaces were inspected.

The first smoke exposed a driver precondition: a Quicklook height proof left
the outer viewport at its bottom, hiding Master level from the next profile's
spacing check. The driver now restores the first row through the existing
native wheel path after that proof. All 23 profile records show offset 93 → 0;
no production UI behavior, geometry assertions or workflow coverage changed.

### UI-test geometry traversal

The compact-window regression in `crates/wsdist-app/tests/ui_workflows.rs`
reuses viewport handles within each stable layout pass and uses scoped,
short-circuit element queries. Each Stats pass scrolls from top to bottom once,
checking every row when fully exposed instead of resetting to the top for each
row. Handles and geometry are refreshed across profile/model/layout changes.
All 53 Stats rows remain checked in each of the twelve geometry passes, with
the same four sizes, two profiles, horizontal baselines, containment,
row/group nonoverlap, and eight oversized-value wrapping checks.

Local debug-test timing, with the pinned toolchain and serial test harness:

| Scenario | Before | After |
| --- | ---: | ---: |
| `compact_window_preserves_input_columns_and_visible_editors` | 320.37 s | 27.13 s |

The optimized scenario passed under `perf stat`; the complete app suite passed
all **68 tests** with
`env -u SLINT_SCALE_FACTOR cargo test --locked -p wsdist-app -- --test-threads=1`.
This is a test-harness optimization, not a production UI or test-profile change.
Evidence: `target/ui-geometry-optimization/comparison.json`,
`compact-profiled.log`, `compact-perf-stat.txt`, and `app-tests.log` in that directory.


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

Python formula evidence and native generator goldens have separate contracts.
The original integer/continuous tolerances below apply to replayed Python
evidence; native golden projections compare every JSON value exactly. The
formula-boundary suite additionally compares all scripted result fields and
kernel results bit-for-bit.

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

### Native RNG test-profile optimization

`Cargo.toml` sets `opt-level = 3` only for `rand_chacha` among RNG packages in
the test profile. `ppv-lite86`, `rand_core`, core/app calculation code and
parity/test targets remain at optimization level zero. The generator, stream
IDs, seed layout, draw order, sample counts and assertions are unchanged.
The override does not change ordinary development or release profiles.

Three interleaved trials per variant exercised the same 18-million-sample gate
with matching workspace features; these intrinsic test timers exclude builds:

| Test-profile RNG override | Median gate time |
| --- | ---: |
| None | 66.79 s |
| `rand_chacha` opt-level 3 | 21.38 s |
| `ppv-lite86` opt-level 3 | 64.00 s |
| Both opt-level 3 | 21.20 s |

Keep only the `rand_chacha` override: its 3.12x speedup captures the measured
benefit; the additional 0.17 s median improvement from `ppv-lite86` is within
the observed trial variation. Workspace features matter: app dependencies
enable `rand_chacha/std` and `ppv-lite86/std`; a core-only build is not the same
SIMD-dispatch control.

The candidate passed all **206 workspace tests**, including 12 RNG protocol
tests and formula/native-golden checks. Complete baseline/candidate numerical
reports matched exactly after excluding only `started_unix_seconds`,
`elapsed_seconds` and `canonical_parity.cases[*].elapsed_ms`. This preserves
all 18 scenarios, 720 batch records, category counts, seeds, means, variances
and tolerances, plus all 30,184 exercised corpus cases and comparison/work counts.
Summed test execution fell from 203.87 s to 154.50 s in that experiment.

The persisted profile then passed all **206 tests** without `--config`:
159.49 s warm wall time, 0.33 s compilation and 156.40 s summed test execution;
the statistical gate took 21.58 s. Compiler-artifact output confirms the scoped
optimization levels and matching workspace features.
Evidence: `target/rng-dependency-experiment-20261029/{findings,matrix,numerical-invariance}.json`
and `target/rng-profile-adoption-20261030/verification.json`.

### Fixture ingestion and content equality

Active legacy, formula-boundary and native case indexes use schema version **3**
and `content_xxh3_128`: XXH3-128 over the complete uncompressed bitcode payload,
not compressed bytes or original JSON. Digests are the numeric `u128` rendered as
exactly 32 lowercase hexadecimal digits, independent of host endianness.
This detects accidental content changes; it is not cryptographic authenticity.
Entries retain `source_path`, `source_sha256` and `source_content_xxh3_128` for
the original logical JSON evidence. Native `legacy_path`/`legacy_sha256` joins
still name that original capture, not the renamed binary storage. Source/runtime/
catalog, RNG-word, observation-projection and delivery SHA-256 contracts are unchanged.

During the earlier JSON-ingestion optimization, all **30,585** indexed documents
were checked against their original SHA-256; JSON and fragment bytes did not
change at that stage. The seven original indexes are preserved across the core/app
`tests/fixtures/original-indexes.zip` archives, with member paths relative to that fixtures
directory. Frozen capture manifests still describe that original evidence;
archives are not runtime fallback indexes.

`wsdist-fixtures` is a shared dev-dependency, not shipped application code.
Its non-generic `read_value` now keeps concrete bitcode/Zstandard decoding inside
the optimized helper crate and returns the final ordered native fixture value.
The fixture, codec, compression and hash packages are optimized alongside the
test-only `rand_chacha` override. Core/app and parity/test code stays at opt-level
zero. Explicit `read_json_value`/`read_json` readers handle metadata and resources;
binary payload readers never fall back to JSON or multipart storage.
Typed conversion borrows native values rather than cloning or materializing
`serde_json::Value`. Metadata is still fully validated before typed projection,
including invalid Unicode/UTF-8, overflowing numbers and excessive nesting in
discarded fields. UI workflows select their index entry before loading a case.

The fixture-ingestion change passed **206 tests**, including all 18 storage
regressions, in the pinned serial debug workspace. These historical timers
exclude compilation and precede the RNG dependency optimization above:

| Workload | Before | After |
| --- | ---: | ---: |
| All test execution | 625.35 s | 201.07 s |
| Complete corpus parity | 187.54 s | 39.20 s |
| WS setup parity | 37.35 s | 6.85 s |
| Controller-transition replay | 31.44 s | 7.00 s |
| Compact-window geometry | 25.89 s | 14.81 s |
| Separate 18-million-sample gate | 66.71 s | 67.20 s |

Hash controls processed the same **30,185 cases / 2,682,609,015 bytes**.
The SHA stage includes its existing formatting/integrity checks; other columns
time the digest alone. Optimized-package controls are separate from algorithm
choice, so the combined speedup is not attributed to XXH3 alone.

| Algorithm | Ordinary debug | Optimized package |
| --- | ---: | ---: |
| SHA-256 | 38.304 s | 1.206 s |
| XXH3-128 | 12.993 s | 0.169 s |
| XXH3-64 | 12.981 s | 0.168 s |
| CRC32 | 2.121 s | 0.050 s |

Those earlier granularity experiments kept one integrity unit per case and
16 MiB physical parts. Reads/reconstruction across **30,201 files** took 1.05 s,
versus 122.62 s for the original owned-JSON decoding. The 29,622 documents no
larger than 64 KiB accounted for 68.54 s of that decoding; the 13 documents above
16 MiB accounted for 21.62 s. Reconstructing identical availability bytes from
1, 8, 120 or 1,905 warm files took approximately 0.269–0.273 s per trial.
Repartitioning did not remove the full-document tree allocations.

MessagePack was tested with optimized concrete slice decoders, complete-input
checks, and exact recursive equivalence for every source field, map/array order,
integer/float distinction and binary64 bit pattern. Encoding and equivalence
work are excluded from decode times:

| Format | Encoded bytes | Full-corpus decode |
| --- | ---: | ---: |
| JSON | 2,682,609,015 | 16.543 s |
| MessagePack | 1,994,249,028 | 15.952 s |

Those earlier measurements retained JSON: MessagePack saved only 0.59 s overall
and was slightly slower for large availability/distribution captures, with
essentially unchanged destruction cost. The bitcode/Zstandard cutover below
supersedes that storage decision; no MessagePack dependency is retained.

The executable release verifier passed **30,184 exercised cases**, one inert
case and all **18 million statistical samples**. The updated native generator
produced **137 byte-identical observation documents** and matching ordered index
metadata without rewriting the active observations. The discarded-index
validation regression was observed failing before its fix and passing after.
Completed workload, operation counts and the complete numerical-comparison
summary are identical to the preceding numerical report.
Evidence: `target/fixture-ingestion-optimization/`, especially
`test-comparison.json`, `hash-and-granularity.json`, `codec-decision.json`,
`source-byte-audit.json`, `native-generator-compatibility.json`,
`compile-profile-verification.json`, `numerical-work-invariance.json` and
`numerical-report.json`.

### Fixture Value lifecycle

Primary corpus cases now decode directly into complete ordered
`wsdist_fixtures::Value` trees, without an intermediate JSON value or flattened
token-to-JSON reconstruction. `drop_value` consumes each case after replay and
owned coverage extraction at the original lifetime boundary. Its concrete native
destructor runs in the optimized helper crate; panic unwinding retains ownership.

Only `legacy_parity` and `verify_numerics` explicitly select MiMalloc as their
Rust global allocator. This applies to all Rust allocations in those two
processes, including core replay and worker threads, not just fixture trees.
The allocator is a core dev-dependency; no library selects a global allocator.
The GUI and other executable roots retain their existing allocator. Native C
allocator overriding is disabled.

The lockfile selects `mimalloc 0.1.48` and `libmimalloc-sys 0.1.49` (native v3),
with empty feature sets. Dev-package opt-level 3 for both allocator packages
also covers the inherited test profile and ordinary development examples;
core/app/parity code remains at opt-level zero. The native allocator needs the
platform C/C++ build toolchain and is linked statically. Separate native-v2
controls did not justify retaining an older transitive dependency pin.

Intrinsic test timers, excluding compilation, measured:

| Full-corpus parity configuration | Test time |
| --- | ---: |
| Unchanged system-allocator baseline | 39.44 s |
| Optimized fixture drop and scoped MiMalloc | 32.47 s |

That lifecycle-only change saved 17.7% of the test's runtime. A capacity-hint
JSON visitor matched every source value, key/array order, scalar kind and float bit
across all 30,185 cases, but its modest decode saving did not justify a second
JSON decoder. No parser, wire-format or fixture changes were part of that earlier
optimization; the current binary format is a separate cutover.

The persisted configuration passed all **206 workspace tests** in 151.06 s
warm wall time, with 148.21 s summed test execution. The actual numerical
verifier passed **30,184 exercised cases**, one inert case and all **18 million
statistical samples**. Complete baseline/candidate reports matched every
non-timing field, ordered key and binary64 bit except the intentional
`delivery_hashes.Cargo.lock.sha256` change, which matches the updated lockfile.
Only `started_unix_seconds`, `elapsed_seconds` and
`canonical_parity.cases[*].elapsed_ms` were normalized.

Compiler-artifact and dependency-graph evidence confirms the scoped profiles,
empty allocator features and absence of either allocator package from the GUI's
normal dependency graph. Execution was verified on Linux GNU x86-64; Windows
MSVC execution was not exercised by this optimization run.
Evidence: `target/corpus-value-optimization-20261030/`, especially
`performance-comparison.json`, `selected-workspace-summary.json`,
`selected-build-summary.json`, `selected-graph-proof.json`,
`numerical-invariance.json`, `native-version-runs.json` and `selected-self.txt`.

### Compressed bitcode fixture storage

All **30,585 indexed payloads** use one `.bitcode.zst` file per case: 30,185
canonical legacy/UI cases, 263 formula-boundary documents and 137 native goldens.
The seven active indexes remain readable JSON with schema version **3**.
Frozen manifests, original-index archives and unindexed capture/export evidence
remain unchanged. Original JSON case files, three multipart descriptors and all
sixteen raw parts have been removed; there is no old-format runtime fallback.

The file is one standard Zstandard level-3 frame with a declared content size and
content checksum. Its decompressed contents are:

- Eight-byte magic/version `WSFIX\0\x01\0`.
- Little-endian `u64` bitcode payload length.
- Little-endian `u128` XXH3-128 of that payload.
- Exactly one bitcode payload, with no trailing data.

The decoder checks the frame size, version, declared length, compression checksum
and header hash, then independently checks the canonical index hash when supplied.
Trailing compressed bytes, concatenated frames and trailing bitcode are rejected.
Uncompressed bitcode is capped at **512 MiB**, container nesting at **128**,
and every stored repository fixture remains below **50,000,000 bytes**.
The largest current compressed case is **5,412,524 bytes**, so multipart storage
is no longer necessary. `.gitattributes` marks `*.bitcode.zst` and `*.zip` as `-text`.

The stable tagged native `Value`/`Number` schema uses `bitcode 0.6.9`'s serde
integration; direct `serde_json::Value` deserialization and recursive native
`Encode`/`Decode` derivation are not supported by that codec. Object insertion
order, array order, integer/float distinctions and binary64 bits are preserved.
`zstd 0.14.3` uses the bundled native library without optional dictionary/legacy
features. Codec upgrades that change the wire schema require a versioned migration.

`read_value`/`decode_value` return the final native tree. `read`/`from_value`
deserialize typed inputs by borrowing it; `to_value` constructs native observation
trees directly. Neither path creates a JSON value tree or a JSON byte roundtrip.
Abilities and recorded RNG tapes use typed streaming deserializers rather than
hidden JSON intermediaries. Genuine application profile/control/resource APIs
and produced reports remain JSON at their actual boundaries.

Before replacement, every original logical JSON stream passed both its active
XXH3 and immutable original-index SHA-256 checks. Every staged compressed case
was read back and compared across **240,318,605 nodes**, including every key
sequence, array position, number kind and float bit, with no mismatches.

| Payload representation, all indexed cases | Bytes |
| --- | ---: |
| Original logical JSON | 2,733,075,633 |
| Uncompressed bitcode | 1,976,026,924 |
| Stored Zstandard files | 190,523,466 |

`cargo test --locked -p wsdist-core --test fixture_storage` covers native and typed
roundtrips, ordering/numeric boundaries, corrupt/truncated data, version/length/
limit failures, independent hashes, complete compressed/bitcode input, discarded
metadata validation and the repository size ceiling.
Conversion evidence: `target/bitcode-zstd-cutover-20261030/{conversion-report.json,conversion-audit.jsonl,apply-report.json}`.

#### Cutover verification and measured loading

The final Linux workspace run passed **215 tests** across all targets:

```sh
env -u SLINT_SCALE_FACTOR SLINT_EMIT_DEBUG_INFO=1 \
  cargo test --locked --offline --workspace --all-targets -- --test-threads=1
```

The real release numerical verifier passed all **30,184 exercised cases**, with
one inert case, plus **18 scenarios / 18,000,000 statistical samples**. Its complete
report matched the previous verified report across **1,099,454 nodes** after
normalizing only timings, renamed storage paths and the independently checked
Cargo.lock fingerprint. Numeric values, binary64 bits and object/array order were
unchanged.

The real native generator reproduced all **137 compressed payloads**, their
original-source provenance and the complete index byte-for-byte. The real native
Linux UI smoke passed its **36 top-level checks** and produced **165 screenshots**;
the rendered equipped-results surface was inspected. Windows was not exercised.
The GUI's normal dependency graph still excludes the fixture helper, bitcode,
zstd and the test allocator.

The loading comparison covers the **30,185 canonical cases**. Each loading result
is the median of five complete warm-cache passes after a full warmup, pinned to
CPU 2 with the same allocator and workspace-matched dependency profiles. Loading
includes file reads, integrity checks and decoding; the final path also includes
bounded Zstandard decompression.

| Canonical corpus measurement | Previous JSON | Previous bitcode prototype | Native bitcode + Zstandard |
| --- | ---: | ---: | ---: |
| Load/check/decode | 14.454 s | 10.242 s | 11.868 s |
| Destroy decoded trees | 3.192 s | 2.399 s | 2.394 s |
| Load + destruction, median per-pass total | 17.644 s | 12.664 s | 14.254 s |
| Full-corpus parity test, intrinsic timer | 33.91 s | Not measured | 28.06 s |

The JSON/prototype columns come from the preceding same-host experiment, not a
new three-way paired run. The prototype reconstructed `serde_json::Value` from
flat tokens; the final implementation uses a different recursive native schema.
The final path is slower than that uncompressed prototype, so these measurements
do not isolate compression overhead. Full-test times are single warm runs,
exclude compilation and are not extrapolated from the loading measurements.

Evidence: `target/bitcode-zstd-cutover-20261030/{performance-comparison.json,native-loading.json,selected-corpus.stdout,final-workspace-tests-summary.json,numerical-invariance.json,native-byte-diff-summary.json,ui-smoke-100/report.json}`.

#### Historical multipart evidence

The preceding JSON-only storage split three large captures into sixteen parts.
Its historical verification below describes that superseded format, not a
supported reader path in the current code.

The original splitting run of `cargo test --locked --workspace --all-targets -- --test-threads=1`
passed **192 tests**, including the storage regression suite.
Evidence: `target/fixture-splitting/workspace-tests.log`.
A size scan covered **30,218 fixture files**; the largest was **48,566,551 bytes**,
with zero oversized files. Evidence: `target/fixture-splitting/size-report.json`.

The release verifier passed **30,192 exercised cases** with zero failures after
splitting, including both 20,000-sample multipart distributions and the
availability case. Evidence: `target/fixture-splitting/numerical-report.json`
and `target/fixture-splitting/reconstruction-report.json`.
A focused native Winit/software probe at 100% scale loaded the multipart
availability case and matched all three Stats sources plus TP/WS/spell displays
against the frozen Ninja reference. Its rendered screenshots were inspected.
Evidence: `target/fixture-splitting/native-ui-100/report.json`.
The disposable probe source was removed after proof.
This is not a passing rerun of the complete native driver: the earlier full
`ui_smoke` run stopped at `Visible accessible Combobox "Master level" not found`
during profile geometry traversal, before this storage change.

#### Unpublished history rewrite

The native migration history was rewritten to store the three large captures
as multipart fixtures from their introduction. Historical reader versions were
updated to use the shared loader, rather than deleting evidence or leaving
intermediate commits unable to read it.

| Reference at rewrite | Original commit | Rewritten commit |
| --- | --- | --- |
| Fixture capture | `3094c376` | `8633df12` |
| Local `rust` tip | `7480fc3e` | `3da4be91` |
| Local `ui` tip | `ed4fc9fb` | `24101290` |

All 14 rewritten commits, including the reflog-only LFS experiment, retained
their author/committer records, dates, messages, ordering and parent topology.
Complete-tree comparisons verified that unrelated paths and file modes stayed
unchanged. The current working tree was archive-identical after promotion;
remaining uncommitted documentation and storage tests were preserved.

The three original fixture blobs are no longer reachable from refs, reflogs,
`ORIG_HEAD` or the index. No blob newly introduced relative to `origin` reaches
50,000,000 bytes. Published `main`/remote refs, all three stashes and repository
configuration stayed unchanged; no remote push was performed. Older Windows
executables already present in published history were deliberately not rewritten.
Old unreachable local objects were not garbage-collected.

The earliest rewritten Rust snapshot passed 10 original buff/player tests.
After live promotion, `git fsck --full --no-dangling` passed and the release
numerical verifier again passed 30,192 exercised cases with zero failures.
Evidence: `target/history-rewrite/history-report.json` and
`target/history-rewrite/numerical-report.json`.
The history report identifies the external recovery directory containing the
complete original Git archive, tracked/untracked working-tree archive and
original-to-rewritten commit map. Both archives were compared against their
original contents before promotion.

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
`numerical_boundaries`, `formula_boundaries`, `rng_protocol`, `legacy_round`, `legacy_spell`,
`legacy_ws_action` and the complete `legacy_parity` driver. Action coverage
includes all 816 round, 9,565 spell and 12,812 WS recorded-draw cases and 112 strict scripted
counterparts. All 184 WS specifications and literal setup/hooks are checked.
GUI and optimizer spell dispatch remain intentionally distinct.

The 23,193 combat cases replay their frozen Python compiled draws through
`ScriptedTape`, checking exact stream/parameter/value order and exhaustion,
including captured action errors. Kernel cases likewise replay recorded draws.
The small exception decoder reads already-frozen Python words; it is not a
generator. Eight unconsumed direct-MT fixtures were removed; all remaining
legacy case documents retain their original bytes and hashes.

### Native random protocol and formula boundaries

`rand_chacha =0.9.0` supplies `ChaCha8Rng`. One `u64` seed occupies key bytes
`0..8` in little-endian order; bytes `8..32` are zero. `Primary` uses stream 0
for action/resist rolls and optimizer choice/shuffle; `Pdif` uses stream 1 for
pDIF kernels. Clones own both continuations, and reseeding resets both streams.
Blank optimizer seeds do not reseed; production startup uses OS entropy.

Each uniform consumes one `u64`, takes its upper 53 bits, and scales by
`2^-53` before `low + (high - low) * unit`. Zero-width intervals still consume a
draw. Bounded integers use masked rejection on Primary; a singleton returns
zero without consuming, and a zero bound panics. `rng_protocol` pins these
invariants, full-width tape reseeds, and seed-0 output bits for both streams.
Captured tape records keep their original `NumpyHost`/`NumbaPython` labels, decoded only as
`Primary`/`Pdif`; native fixtures contain no tape records.

`tests/fixtures/boundaries/` records 89 accepted Python threshold groups
(49 round, 38 WS, 2 pDIF kernel), with zero rejected groups. The 261 combat
cases cover the adjacent binary64 values below/equal/above each threshold;
two kernel cases each contain those three rows. Hash-checked replays preserve
both strict tape consumption and the expected branch pattern. Mutation proofs
changing resist `>=` to `>` and QA `<` to `<=` failed on the intended groups;
both changes were reverted. Capture provenance includes the pinned runtime,
source snapshot and capture-program hashes. The local untracked capture tool
is `target/migration/capture_boundaries.py`; its second run was byte-identical.

Native generator-dependent evidence lives separately in
`tests/fixtures/native/`: 137 cases (44 neighborhoods, 28 passes, 30 builds,
one sequence, 20 DPS trajectories, 12 distributions, one nonprogress
diagnostic, and one sampled WS). Each index entry binds the native bytes to the
source legacy path and SHA-256. Projections include complete search decisions,
results, draw-call counts and four-uniform continuations per stream; simulation
hit buffers retain their length and serialized SHA-256. No model-state address
or worker scheduling identity is projected.

To regenerate deliberately after reviewing a generator/projection change, use a
fresh output directory first; the tool refuses an existing directory. Review and
compare the result before replacing accepted evidence. Normal builds and tests
never regenerate fixtures. The writer emits the same compressed format, bitcode
hashes and original JSON source identity/provenance as the checked-in corpus.

```sh
cargo run --locked --release -p wsdist-core --example regenerate_native_goldens -- \
  --legacy crates/wsdist-core/tests/fixtures/legacy --out target/native-regen-check
diff -r crates/wsdist-core/tests/fixtures/native target/native-regen-check
```

The two earlier JSON generation runs were byte-identical; their largest native
JSON case was 1,283,069 bytes. Usage, missing-input I/O and existing-output refusal
paths each exited 2 in that verification.

### Ordered optimizer and full simulations

`optimizer_behavior`, `simulation_behavior` and `simulation_statistics` replay
432 search/simulation cases: 399 optimizer cases, 20 complete DPS trajectories,
one nonprogress diagnostic and 12 complete distributions. Checks include
ordered combinations, full-value gear equality, legality, first strict
improvement, DT stages, restarts/ties, and worker/coordinator contexts.
Generator-independent options, initial IDs, combinations, legality, score
results, player sheets, bound checks, metadata transitions and errors still
compare against Python. Generator-dependent order, selected gear, outputs,
trajectories and continuations compare against native goldens instead of MT.
The public seed-11 optimizer result is checked across workers 1/2/auto, without
pinning the former MT-specific equipment or metric literals. Unsatisfied
DT/no-winner results and zero-time inverse-metric errors remain explicit, not
fabricated feasible/infinite winners.

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
Batch `i=0..39` uses one native seed `20240101+i` with independent Primary/Pdif streams.
Means must meet `max(1e-9, 6 * standard_error)`; categorical frequencies meet
`6 * sqrt(p * (1-p) / 1000000)`, with p=0/1 exact. The report retains sample
counts, seeds, batch variances, errors, expectations and pass/fail gates.
Full mixed-combat recorded/tape parity remains separately required.

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

Linux native proof uses an isolated X11 display and the development focus helper:

```sh
mkdir -p target
cc -std=c11 -O2 tools/native-window-x11.c -o target/native-window-x11 \
  $(pkg-config --cflags --libs x11 libpng)
xvfb-run -a -n 198 -s '-screen 0 2560x1800x24' \
  env -u SLINT_SCALE_FACTOR -u WAYLAND_DISPLAY -u WAYLAND_SOCKET WSDIST_DATA_DIR="$PWD" \
  SLINT_BACKEND=winit-software SLINT_EMIT_DEBUG_INFO=1 \
  target/native-window-x11 --focus-main \
  cargo run --locked -p wsdist-app --example ui_smoke -- --output target/ui-smoke-100
xvfb-run -a -n 199 -s '-screen 0 2560x1800x24' \
  env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET WSDIST_DATA_DIR="$PWD" \
  SLINT_BACKEND=winit-software SLINT_EMIT_DEBUG_INFO=1 SLINT_SCALE_FACTOR=1.5 \
  target/native-window-x11 --focus-main \
  target/debug/examples/ui_smoke --output target/ui-smoke-150
```

The helper focuses the actual mapped main window with `XSetInputFocus` and checks
`XGetInputFocus`; it is not shipped. Bare Xvfb otherwise lets injected keys reach
Slint's stored focus item while the window remains inactive, bypassing
focus-dependent behavior. Both Wayland variables must be unset so the clipboard
uses the isolated X11 display rather than the inherited host session. Concurrent
runs require separate displays and a screen large enough for scaled windows.
Windows PowerShell sets the corresponding Slint/data environment variables and
runs the Cargo command without the Linux helper or POSIX prefix. The driver uses
the real Winit/software backend, never the testing backend, and temporary
resources for save/reload.

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

The compact-dropdown update passed all **66 app tests** and the complete native
driver at **100% and 150%** scale. Master level and TP set metric popup rows
match their 26px selected displays; Resistance rank matches its 21px display.
All three retain an effective 11px text inset and zero vertical-center offset.
The driver exercises pointer selection, long-list wheel scrolling, selected-item
visibility during Up/Down navigation, Return/Escape closing and reopening, and
restores controller inputs before the existing workflows. Closed/open native
screenshots were inspected at both scales. Evidence:
`target/dropdown-padding-final-100/report.json` and
`target/dropdown-padding-final-150/report.json`.
The rebuilt local `wsdist-gui` also rendered a 760×900 native window from an
unrelated working directory with `WSDIST_DATA_DIR` unset and exited cleanly;
its observed surface is `target/dropdown-padding-launch.png`.

The compact renderer reuses Slint 1.16's common combo interaction base and Fluent
style primitives. Test input adapters map popup-local geometry to its observed
native origin: Slint 1.16's embedded popup `ElementHandle::absolute_position()`
does not include that origin. These internal APIs are covered by the exact
Slint version pin and need review when upgrading the framework.

The vertical-density update passed a focused native Winit/software probe at
**100% and 150%** scale. All **22 job profiles** retain fully visible Basic and
Enemy inputs. Four-tab captures at 760×900, 760×440, 720×440 and 1440×1080 verify
compact input gutters, top-aligned Enemy/Food content, 2px section gaps and
26px equipment-option rows. Native tab clicks, radio pointer/DownArrow selection,
checkbox selection/restoration, checkbox containment and outer-wheel access to
lower controls passed. Representative compact and high-DPI screenshots were
inspected. Evidence: `target/vertical-spacing/verified-100/report.json` and
`target/vertical-spacing/verified-150/report.json`. This is focused layout proof,
not a rerun of the complete numerical UI workflow above. The rebuilt local
`wsdist-gui` also painted a 760×900 native window from an unrelated working
directory without `WSDIST_DATA_DIR` and closed cleanly; its capture is
`target/vertical-spacing/standalone-final.png`.
All **66 app tests** passed after these layout changes. The disposable density
probe was removed after capturing the evidence.

The compact-layout correction was compared with the frozen Qt application.
Qt received a 760×440 resize request but enforced an actual 760×897 window
(`target/compact-layout/reference/report.json`); the native application supports
an actual 760×440 window using outer vertical scrolling, not column stacking.
The permanent native driver now checks 760×440, 720×560 and 1440×1080 at both
100% and 150% scale, verifies the resulting logical dimensions, and records the
three input columns, four buff columns and grouped Stats geometry. It rejects
stretched/overlapping editor rows, checks equipment beside candidate lists,
scrolls to lower actions and performs a native compact Quicklook WS calculation
against the frozen Ninja result. Both runs passed, and compact screenshots for
all four tabs were inspected. Evidence:
`target/compact-layout/verified-100/report.json` and
`target/compact-layout/verified-150/report.json`.

The subsequent fixed-width/icon-centering proof also passed at 100% and 150%.
The four main tabs now have a fixed 704px rail and 680px content rows, with
left-aligned horizontal geometry independent of window width. The regression
and native driver compare element X positions and widths across 760×440,
720×560 and 1440×1080, including profile changes, all displayed Stats rows,
candidate lists/inspectors, TP/WS gear and lower actions. Normal, empty and
label-only gear cards were checked through debug element geometry: 48×48px
cards, centered 28×28px Image canvases or centered slot-label canvases. Oversized
Stats values retain fixed columns, gain vertical space and do not overlap
subsequent rows/groups. Native compact/expanded screenshots were inspected;
both Quicklook WS and TP results/time fields remain reachable and match the
frozen Ninja calculations. All 61 app tests passed. Evidence:
`target/fixed-width-layout/verified-100/report.json` and
`target/fixed-width-layout/verified-150/report.json`.

The missing-icon/visible-scrollbar update passed all 65 app tests. The native
driver removes the requested `icons32/20977.png` only in its temporary sandbox,
verifies a blank 0×0 image with the gear card still usable, restores the original
bytes, and recovers the image through the normal reload-data action.

The persistent scrollbar lane is 18 logical pixels wide, with an opaque 14px
thumb and a 32px minimum length capped to a shorter track. The complete native
driver passed at 100% and 150% scale with actual 760×440, 720×440, 720×560 and
1440×1080 logical windows. It exercises pointer down/move/up thumb dragging,
track page clicks and wheel scrolling on the four tabs, nested scalar inputs,
2,048 virtual candidates and their inspector, 1,024 variable-height progress
rows, and the native report editor. Empty/fitting content retains its reserved
lane without a handle. Keyboard End/Home leaves gear radio selection fully
visible after virtual realization; explicit user scrolling remains independent.
For variable-height rows, wheel direction is verified with unique rendered text
anchors, not monotonic estimated offsets: Slint may refine the global estimate
while correctly moving the visible rows. The real TP/WS comparison is repeated
four times to ensure report overflow; scrollbar interaction preserves selection
and exact native clipboard contents. Each full run captured 156 screenshots;
representative blank/restored icons and compact/high-DPI scrollbar surfaces were
inspected. Evidence:
`target/visible-scrollbars/verified-100/ui-workflows/report.json` and
`target/visible-scrollbars/verified-150/ui-workflows/report.json`.

Observed Winit/software renderer limit: selected report backgrounds can paint
below the native viewport, while text glyphs and the enlarged scrollbar remain
clipped and clipboard contents remain exact. [INFERENCE] Source inspection
attributes this to Slint 1.16's stock Parley/software selection rectangle path
bypassing the inherited clip, not to removal of a clip by the wrapper. No
pre-change selected-report screenshot was available for a visual before/after
comparison. The native editor and renderer are retained; no dependency patch is
included in this scrollbar change.

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

The Quicklook two-field stacking update passed all **66 app tests** and a focused
native Winit/software probe at **100% and 150%** scale. At 760×900, 760×440,
720×440, 720×560 and 1440×1080, both labels match Active Food's natural text
height and sit immediately above unclipped, full-column-width entries. Other
buff controls retain their horizontal positions, widths and heights. Native
click/typing/Return committed skill `512` and bubble potency `75` without
starting calculations; Paladin, Dark Knight and Ninja retained their Divine,
Dark and Enhancing Skill labels. Compact and high-DPI screenshots were inspected.
Evidence: `target/stacked-buff-inputs/focused-100/report.json` and
`target/stacked-buff-inputs/focused-150/report.json`. The disposable probe was
removed. The rebuilt local `wsdist-gui` rendered at 760×900 from an unrelated
working directory without `WSDIST_DATA_DIR` and closed cleanly; its inspected
capture is `target/stacked-buff-inputs/standalone.png`.

The complete native driver did not pass: its Master level popup selected-item
visibility check failed before reaching the buff geometry checks. No popup UI
changes were made. The obsolete fixed dropdown-height assertion was removed,
while popup rows must still match their rendered selected-value control.
The evidence above is focused layout proof, not a complete UI-workflow pass.

The Quicklook content-height update passed focused native Winit/software probes
at **100% and 150%** scale. All **22 job profiles** were checked at 760×900;
Ninja and Black Mage were also checked at 760×440, 720×440, 720×560 and
1440×1080, then resized back to 720×440. Special Toggles and the final Enemy
row share their bottom edge. Both horizontal sections fit their natural content
without stretching in expanded windows; normal control sizes and fixed widths
remain intact. Native wheel input reaches the last special toggle, equipment
and lower actions without changing input values. Compact, expanded and high-DPI
captures were inspected. Evidence: `target/quicklook-sizing/final-100/report.json`
and `target/quicklook-sizing/verified-150/report.json`. The disposable probe was
removed. The rebuilt local `wsdist-gui` also rendered at 760×900 from an unrelated
working directory without a data override and exited cleanly; its capture is
`target/quicklook-sizing/standalone-final.png`. The full native workflow was not
rerun; its previously recorded unrelated popup failure remains outside this change.
All **66 app tests** passed with
`cargo test --locked -p wsdist-app -- --test-threads=1`, including the extended
compact-window alignment and content-height regression checks.

The item-selection details update passed all **66 app tests** with
`cargo test --locked -p wsdist-app -- --test-threads=1` and focused native
Winit/software probes at **100% and 150%** scale. Quicklook, Optimize, TP and WS
were exercised at 760×900, 760×440, 720×440 and 1440×1080. At 760×900, each
candidate viewport is exactly **40 logical pixels (two 20px rows) shorter** than
the captured baseline; the details viewport grows from 50px to 96px after the
grouping caption is removed. Native hover, click, End/Home/Down navigation and
details-wheel scrolling retained the corresponding item-name heading and exact
stats-only body, with no repeated name or grouping caption. Compact and high-DPI
screenshots were inspected. Evidence: `target/item-selection/baseline/report.json`,
`target/item-selection/final-100/report.json` and
`target/item-selection/final-150/report.json`. The disposable probe was removed
after proof. This focused run did not repeat the full numerical UI workflow.

The Optimize result-popup update passed all **67 app tests** with
`cargo test --locked -p wsdist-app -- --test-threads=1` and focused native
Winit/software probes at **100% and 150%** scale. Seed-11 one-slot searches from
the shipped Ninja profile exercised **Optimize WS**, **Optimize Spell**, and
**Optimize TP** with both Time to WS and DPS metrics. Each completion dismissed
progress and opened exactly one visible final report; its native accessible
text matched the completed result, winning equipment and computed numeric values.
Closing a result left the main window usable for native tab navigation and the
next optimization. Final-result screenshots were inspected. Evidence:
`target/optimize-result-popup/final-100/report.json` and
`target/optimize-result-popup/final-150/report.json`.

Early progress dismissal used the production close callback synchronously before
native configuration; the controller regression also exercises the accessible
Close progress button. The disposable native probe was removed after proof.
These focused runs did not repeat the full numerical UI workflow. The rebuilt
local `wsdist-gui` rendered at 760×900 from `/tmp` without a data override and
exited cleanly; its inspected capture is `target/optimize-result-popup/standalone.png`.

The worker-error popup fix passed all **68 app tests** with
`cargo test --locked -p wsdist-app -- --test-threads=1`. Focused native
Winit/software probes passed at **100% and 150%** scale: **Optimize WS** with
empty pools, **Optimize Spell** with one singleton head pool, and **Optimize TP**
with every slot singleton each displayed the complete candidate-selection
assertion in one fresh result window. The main error stayed visible, progress
was dismissed, inputs became idle, and no successful result or best set was
installed. Closing the error reports left inputs usable; a subsequent real
seed-11 optimization and **Equip best set** both succeeded.

The controller regression also verifies retention of an earlier successful
optimizer result and best set, and no popup for cancellation or rejected busy
edits/actions. Native error/recovery screenshots were inspected. Evidence:
`target/optimizer-error-popup/native-100/report.json` and
`target/optimizer-error-popup/native-150/report.json`. Early progress dismissal
used the production close callback before native configuration. The disposable
probe was removed after proof. The rebuilt local `wsdist-gui` also rendered from
`/tmp` without a data override and exited cleanly; its inspected capture is
`target/optimizer-error-popup/standalone.png`. These focused native runs did not
repeat the full numerical UI workflow.

### Calculation-process result popups

`cargo test --locked --workspace -- --test-threads=1` passed all **199 tests**,
including the frozen numerical/RNG corpus and the new round, spell and optimizer
calculation-trace regressions. The trace checks preserve numerical results and
random-stream continuation, reconcile captured stages, retain the earliest
restart winner and pre-reorder scored output, and restore caller observers.

Focused native Winit/software probes passed at **100% and 150%** scale, with
**25 scenarios and 52 screenshots per scale**. Production controller callbacks
exercised analytical melee/magical/hybrid WS, their sampled WS paths,
single/dual-wield and H2H TP rounds, EnSpell, Quick Draw, Ninjutsu, elemental
nukes/helixes, Kaustra, Impact, ranged/Barrage, and the existing divine zero-output
branch. Two-restart one-slot searches covered Optimize WS, Spell, TP/Time to WS,
and TP/DPS.

Each completion produced one visible result editor whose accessible text matched
the completed report, with numerically substituted calculation stages and no
unresolved operand tokens. Native scrolling and independent Close result actions
remained usable; optimizer progress was dismissed. Representative formula,
random-placeholder, spell-stage, high-DPI and minimum-size popup screenshots were
inspected. These were focused popup checks, not a rerun of the complete native
workflow. The disposable probe source was removed after proof. Evidence:
`target/calculation-popups/final-100/report.json` and
`target/calculation-popups/final-150/report.json`.

The earlier probe's Ranger WS case retained Viper Bite after profile menu updates,
so it did not establish ranged-WS coverage. Its Barrage case also left the ability
disabled. The BG Wiki formula-form proof below explicitly selects and checks
those paths rather than relying on scenario names.

#### BG Wiki formula form

`cargo test --locked --workspace --lib --bins --tests -- --test-threads=1`
passed all **199 tests** after adding named equations alongside the worked
substitutions. The numerical/RNG corpus and complete-observer restoration checks
remain unchanged in behavior. The displayed wiki reference forms are explicitly
separated from the evaluator's existing truncation, factor order and unmodeled
terms; this update does not recalibrate damage to the wiki.

A focused native Winit/software probe passed **31 scenarios at each of 100% and
150% scale**: 30 formula-result cases and the unchanged Simulate-page comparison.
It explicitly selected Jishnu's Radiance and asserted ranged dispatch for
Quicklook, sampled WS and Optimize WS; enabled and checked Barrage; covered all
four WS families, TP rounds, spell families, and small two-restart optimization
searches. Every formula result retained named equations, worked numeric inputs,
random placeholders and exact accessible report text through scrolling/resizing.
Popup closing left the main window open, progress was dismissed, and source
resource/profile hashes were unchanged.

Each scale produced **114 native screenshots**, including 900×680 and 400×280
logical result windows. Requested and actual main/result/snapshot scales were
asserted; the 150% popup captures were 1350×1020 and 600×420 physical pixels.
Representative normal, compact, ranged, magic and optimization formula captures
were inspected.

The probe was compiled with `SLINT_SCALE_FACTOR` unset, then the same binary was
launched with runtime scale 1 and 1.5. Slint 1.16 also reads that variable during
compilation and otherwise locks the generated UI scale. The earlier `final-150`
attempt recorded actual scale 1.0 and is not high-DPI proof; the scale-asserted
evidence is `target/wiki-formula-popups/verified-100/report.json` and
`target/wiki-formula-popups/verified-150/report.json`. The disposable probe source
was removed after proof. These focused checks do not repeat the complete native
workflow.

#### Tactical Points formula form

`env -u SLINT_SCALE_FACTOR cargo test --locked --workspace --lib --bins --tests -- --test-threads=1`
passed all **199 tests** after the TP presentation update. Shared capture-only TP
notation replaces duplicate WS/spell display helpers; `math::get_tp` remains the
sole numerical kernel. Wiki flooring and integer range labels are distinguished
from the evaluator's truncation, continuous delay boundaries and expected-hit
weighting. Action-specific Store TP units, WS eligibility/retention, Regain,
Occult Acumen and Recycle exceptions remain explicit rather than changing results.

Focused native Winit/software runs passed **19 scenarios at each of 100% and
150% scale**, with **32 captures per scale**. The cases exercised single-wield,
dual-wield, H2H, SAM Zanshin, positive Regain, first/additional-hit and Fotia WS TP,
actual ranged WS dispatch, probability-weighted pure magical WS TP in analytical
and sampled modes, raw Quick Draw Store TP, EnSpell, nonzero Occult Acumen, and
Barrage with Arcadian Beret Recycle. Four small real searches covered Optimize TP
with Time to WS/DPS and Optimize WS/Spell with TP return.

The pure magical WS cases deliberately set positive enemy magic evasion to
exercise a fractional TP weight; the shipped zero-evasion profile bypasses that
branch. Worked TP values and summaries matched actual results, optimizer details
followed retained output, and closing each report left main-window editing usable
with no progress window left open. Defaults were isolated in a temporary directory.

Normal 900×680 and minimum 400×280 logical report windows were captured with
asserted runtime scales and physical pixel dimensions. The actual native
accessible scrollbar located the named/worked TP sections, and representative
Regain, sampled WS, Occult Acumen and compact Quick Draw captures were inspected.
The same scale-neutral binary served both runs. Evidence:
`target/tp-formula-popups/inspected-100/evidence.json` and
`target/tp-formula-popups/inspected-150/evidence.json`. The disposable probe source
was removed after proof; these are focused TP popup checks, not a rerun of the
complete native workflow.

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

The original cutover Linux proof ran that archive's production GUI and complete UI
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

The compact-layout Linux archive was rebuilt and its extracted production GUI
launched at 100% and 150% from an unrelated empty CWD, with `WSDIST_DATA_DIR`
unset. Both native launch/close proofs passed; these archive runs did not repeat
the separately completed full UI workflows above. Evidence:
`target/compact-layout/archive-100/report.json` and
`target/compact-layout/archive-150/report.json`.
Archive SHA-256:
`4d8fabc2511a3be22307266823f684ae2100ec2f999d80a64743a5b815bec249`.

The fixed-width/icon-centering Linux archive was rebuilt. Its extracted
production GUI rendered and closed at both scales from an unrelated empty CWD,
with executable-relative resources and `WSDIST_DATA_DIR` unset. The full
workflows were exercised separately by the native driver above. Evidence:
`target/fixed-width-layout/archive-100/report.json` and
`target/fixed-width-layout/archive-150/report.json`.
Archive SHA-256:
`c478267082ed7702ae276dc9294cb7f7527de5bb3c9e393fe4e390397d2f10f0`.

The blank-missing-icon/larger-scrollbar Linux archive was rebuilt. Its extracted
production GUI rendered and closed at 100% and 150% scale from an unrelated empty
CWD, with executable-relative resources and `WSDIST_DATA_DIR` unset. The native
surfaces were inspected: persistent wider scrollbars do not cover controls, and
the WHM/GEO numeric fields remain fully readable. These archive runs cover
production launch/close, not the separate development driver's full workflows.
Evidence: `target/visible-scrollbars/archive-100/report.json` and
`target/visible-scrollbars/archive-150/report.json`.
Archive SHA-256:
`a08ab0619b79c7ae697bc2169a72051ebe3cb509ff7a9c85f6e2f23e08964812`.

Subsequent extracted runs also passed the complete development UI driver at
both scales, in addition to production launch/render/close:
`target/visible-scrollbars/verified-100/report.json` and
`target/visible-scrollbars/verified-150/report.json`. Each report links its
complete workflow evidence and screenshots. The archive hash above is unchanged.

## Popup monospace family

Focused Linux Winit/software probes passed at 100% and 150% scale. The controller
resolved the installed generic monospace family to `Noto Sans Mono` and applied
its concrete family name to result-editor and optimizer-progress text, without
embedding or bundling a font.

Before the change, six equal-length rows of `i`, `W`, `0`, `1`, `.`, and `M` had
different trailing-marker positions in the result popup. Afterward, all six
markers aligned exactly in both result and progress popups at both scales.
The main-window screenshots were byte-for-byte unchanged at each scale.

The probes used production Quicklook WS and one-iteration seeded Optimize WS
callbacks. Native selection/Ctrl+A/Ctrl+C copied the complete 16,595-byte Quicklook
report unchanged; wheel scrolling and 440x300 logical-pixel resizing were
exercised and inspected. Close result hid only its popup and left the main window
visible. The disposable probe source was removed after verification. This is
focused Linux font/rendering proof, not a Windows runtime or full-workflow rerun.

Evidence: `target/popup-font/after-100/report.json`,
`target/popup-font/after-150/report.json`, and
`target/popup-font/glyph-metrics.json`, with native screenshots beside each report.

`cargo test --locked --workspace --lib --bins --tests -- --test-threads=1` passed
all 206 tests. Warnings were limited to unused imports/dead code in core test
helpers.
