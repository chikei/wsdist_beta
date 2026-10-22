# WSDist

Native Rust/Slint desktop application for FFXI damage calculations, gear
optimization and simulations. Four tabs: **Quicklook**, **Optimize**,
**Simulate**, and **Player Stats**. The application does not require Python,
Qt, NumPy, Numba or Matplotlib.

## Portable application

Native CI produces complete bundles:

- `wsdist-linux-x86_64.tar.gz`
- `wsdist-windows-x86_64.zip`

Extract the entire archive into a writable directory and launch
`wsdist-gui` (Linux) or `wsdist-gui.exe` (Windows). Keep `data/`,
`defaults.json`, `item_list.csv`, `icons32/` and the license notices beside the
executable. Launching from another working directory is supported; missing
resources produce a visible startup error rather than loading another directory.
Windows bundles use the static MSVC CRT. Linux bundles use the system's
fontconfig, FreeType, fonts and X11/Wayland libraries; CI builds on Ubuntu 24.04.
An interactive Linux inventory file chooser also needs an XDG Desktop Portal
implementation such as GTK, GNOME or KDE.

The archive and its `.sha256` file are uploaded only after the native numerical,
GUI and extracted-archive CI gates pass. Workflow definitions are not evidence
that a particular Windows build has already been executed; see [VERIFY.md](VERIFY.md)
for the exercised verification and remaining manual checks.

## Workflows

- **Quicklook:** select a job/profile, abilities, enemy and party buffs; edit the
  independent Quicklook equipment set. Use **Quicklook TP**, **Quicklook WS**,
  **Quicklook Spell** or **Simulate WS** explicitly. Editing does not recompute
  the last displayed results or Player Stats.
- **Optimize:** check candidates independently for each slot, apply the bulk
  selection filters or import a Windower inventory file, choose metrics and
  search options, then run an optimizer. **Equip best set** is enabled only for
  an actual returned player. Unsatisfied damage-taken constraints remain visible.
- **Simulate:** maintain separate TP and WS equipment sets; run the full
  7200-second DPS simulation, a 20000-sample WS distribution, or the TP/WS stat
  comparison. Reports and charts open in independent windows.
- **Player Stats:** explicitly refresh from Quicklook, TP or WS gear. Reports
  preserve the existing source-specific display units and haste caps.

At 760×440, the UI preserves the original three input columns, four buff columns,
equipment grid beside its candidate list, and grouped Stats columns. The main
tabs use a fixed 704px rail with 680px content rows: controls, dropdowns, buttons,
candidate lists and Stats columns keep their widths when the window grows.
Content stays left-aligned; only the available height and vertical scrolling
expand. Equipment cards are 48×48px with centered 28×28px icon canvases. Long
Stats labels/values wrap vertically instead of widening their columns.
Stacked sections use 2px vertical gaps. Input rows have no extra vertical gutter;
equipment options use 26px rows without shrinking the dropdowns, entries or
checkboxes. Compact section heights avoid leaving the saved row space blank.
Quicklook's first input row fits the enemy grid, with the Special Toggles scroll
area ending at the same bottom edge. The Active Buffs row fits its tallest column.
In Quicklook's Active Buffs, the WHM skill and Geomancy Bubble Potency fields
stack their labels above full-width inputs, matching Active Food.
Dropdown options match the selected-value display's row height, text padding and
vertical centering, including the compact Resistance rank control.
Missing or unreadable icon assets leave the image blank; the slot button and item
details remain usable. App-owned scroll views reserve an 18px vertical scrollbar
lane with a persistent 14px thumb and contrasting track whenever content overflows.
Drag the thumb, click the track to page, or use the mouse wheel; report text retains
native selection and copying.
The preferred window size is 760×900; the minimum remains 720×440.

The three equipment sets have independent slot grids, variant lists, icons and
item details. Copy actions support Quicklook ↔ TP and Quicklook ↔ WS;
clipboard exports use the actual selected source. Closing a result/chart window
does not close the application. One calculation runs at a time; inputs freeze
while it runs, while tab navigation and Close remain available. Cancel/Close
joins the owned workers without publishing a partial result.

`Ctrl+1` through `Ctrl+4` select tabs. `q` closes the application only outside
text editing. The TP entry accepts 1000–3000 with mouse-wheel steps of 100.
Use **File > Save Defaults** to save both the current job and default profile;
there is no exit autosave. **Settings** retains the unpersisted Damage Limit
(DPS) and Verbose Swaps toggles. **Help > About Slint** supplies framework
attribution.

## Profiles and editable data

Existing JSON profiles retain their original setting names, unknown keys and
all three sets of equipment references. Copy an existing `defaults.json` into
the bundle to retain those profiles. The verified cutover removed the Python
application, runtime configuration and `defaults.pkl`; `defaults.json`, CSV and
icons were retained byte-for-byte. Rust does not execute Python catalogs or load
pickle. Convert any external pickle-only settings to JSON with the original
application before adopting the native build.

Edit the inert catalogs under `data/`, then select **File > Reload Data**.
Reload validates all catalogs and the CSV before committing, retains current
selections by item variant name, reports removed items, and resets session-local
H2H metadata. Invalid data leaves the previous catalog and selections intact.
Malformed profile JSON is reported and never silently overwritten.

`WSDIST_DATA_DIR` explicitly overrides the entire resource root. Otherwise the
executable's directory is authoritative, not the process working directory.

## Build and verification

Install Rust through rustup; `rust-toolchain.toml` pins Rust **1.92.0**. The
workspace uses edition 2024, Slint **1.16.0** and committed locked dependencies.
Linux build prerequisites include a C/C++ linker, pkg-config, fontconfig/FreeType
development files and the native X11/Wayland/xkbcommon libraries. On Windows use
a native x64 MSVC build environment.

```sh
cargo build --locked --release -p wsdist-app --bin wsdist-gui
WSDIST_DATA_DIR="$PWD" SLINT_BACKEND=winit-software target/release/wsdist-gui
```

On Windows PowerShell, set `$env:WSDIST_DATA_DIR=(Get-Location).Path` and launch
`target/release/wsdist-gui.exe`. To create a native portable archive, install the
build-only notice generator and run the platform's packaging tool:

```sh
cargo +1.92.0 install --locked cargo-about --version 0.8.2
bash tools/package-linux.sh --output target/dist
```

Windows: `./tools/package-windows.ps1 -Output target/dist`. Packaging uses an
allowlisted staging directory, target-filtered locked dependency notices and
archive checksums; it does not ship reference fixtures, capture tools or caches.

[VERIFY.md](VERIFY.md) documents the complete test commands, executable numerical
verifier, real-backend screenshot smoke and extracted-archive proof. Frozen JSON
fixtures record the reviewed original implementation. Legacy numerical results
are preserved, including known analytical/sampled discrepancies; parity and
independent internal checks do **not** establish accuracy against live-game data.

## Provenance

Third-party notices, exact dependency source links, Slint license/asset
attributions and Rust standard-library notices accompany each archive. No new
license is granted for project code, game data or item images; redistribution
permissions remain separate. See `tools/licenses/PROVENANCE.txt`.

Report issues through the repository's GitHub issue tracker, with the affected
operation, inputs and visible error/report.
