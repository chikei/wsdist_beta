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

Current integrated Linux results: **109 core tests** and **61 app tests** passed.
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

Linux native proof uses an isolated X11 display and the development focus helper:

```sh
mkdir -p target
cc -std=c11 -O2 tools/native-window-x11.c -o target/native-window-x11 \
  $(pkg-config --cflags --libs x11 libpng)
xvfb-run -a -n 198 -s '-screen 0 2560x1800x24' \
  env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET WSDIST_DATA_DIR="$PWD" \
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
