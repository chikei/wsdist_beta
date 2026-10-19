#!/usr/bin/env bash
# Run from an X11 session (or xvfb-run + dbus-run-session). Never ships smoke tools.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
archive=''
output="$root/target/archive-smoke"
ui_smoke=''
scale=1
while (($#)); do
    case "$1" in
        --archive|--output|--ui-smoke|--scale)
            [[ $# -ge 2 ]] || { echo "$1 needs a value" >&2; exit 2; }
            case "$1" in
                --archive) archive=$2 ;; --output) output=$2 ;; --ui-smoke) ui_smoke=$2 ;; --scale) scale=$2 ;;
            esac
            shift 2 ;;
        --help) echo 'bash tools/smoke-linux-archive.sh --archive FILE [--output DIR] [--ui-smoke BINARY] [--scale 1|1.5]'; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -n "$archive" && -f "$archive" ]] || { echo 'A native Linux release archive is required' >&2; exit 2; }
[[ "$scale" == 1 || "$scale" == 1.5 ]] || { echo '--scale must be 1 or 1.5' >&2; exit 2; }
for tool in cc pkg-config jq tar realpath timeout; do
    command -v "$tool" >/dev/null || { echo "Missing archive-smoke prerequisite: $tool" >&2; exit 1; }
done
pkg-config --exists x11 libpng
archive=$(realpath -- "$archive")
if [[ -n "$ui_smoke" ]]; then
    ui_smoke=$(realpath -- "$ui_smoke")
    [[ -x "$ui_smoke" ]] || { echo "Missing native ui_smoke executable: $ui_smoke" >&2; exit 1; }
fi
mkdir -p -- "$output"
output=$(cd -- "$output" && pwd -P)
work=$(mktemp -d "$output/.wsdist-archive.XXXXXXXX")
bundle="$work/extracted"
other_cwd="$work/unrelated working directory"
mkdir -p -- "$bundle" "$other_cwd"
status=running
pid=''
archive_hash=$(sha256sum -- "$archive"); archive_hash=${archive_hash%% *}
finish() {
    code=$?
    trap - EXIT
    if ((code != 0)); then status=failed; fi
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
    jq -n --arg status "$status" --arg archive "$archive" --arg hash "$archive_hash" --arg scale "$scale" \
        --arg cwd "$other_cwd" --arg executable "$bundle/wsdist-gui" --arg ui_smoke "$ui_smoke" \
        '{status: $status, platform: "linux", archive: $archive, archive_sha256: $hash,
          requested_scale: ($scale | tonumber), working_directory: $cwd, executable: $executable,
          resource_mode: "Executable-relative; WSDIST_DATA_DIR unset; unrelated empty CWD",
          production_surface: "gui.png", production_window: "window.json", loaded_modules: "gui.png.modules.txt",
          ui_workflows: (if $ui_smoke == "" then "Not requested" else "ui-workflows/report.json" end),
          limits: ["Automated native file-dialog interaction is covered by the manual desktop checklist"]}' > "$output/report.json"
    rm -rf -- "$work"
    exit "$code"
}
trap finish EXIT
# Reject unsafe member names before extraction; the package script emits an
# allowlisted root, not a source-tree archive.
tar --list --gzip --file "$archive" > "$output/archive-members.txt"
while IFS= read -r member; do
    case "$member" in
        /*|../*|*/../*|*/..) echo "Unsafe archive member: $member" >&2; exit 1 ;;
    esac
done < "$output/archive-members.txt"
tar --extract --gzip --file "$archive" --directory "$bundle" --no-same-owner --no-same-permissions
for name in wsdist-gui defaults.json item_list.csv THIRD-PARTY-NOTICES.html PROVENANCE.txt BUILD-INFO.json; do
    [[ -f "$bundle/$name" ]] || { echo "Missing archive content: $name" >&2; exit 1; }
done
for name in gear.json enemies.json buffs.json pup-attachments.json; do
    [[ -f "$bundle/data/$name" ]] || { echo "Missing bundled catalog: $name" >&2; exit 1; }
done
[[ -d "$bundle/icons32" && -d "$bundle/licenses" && -x "$bundle/wsdist-gui" ]]
jq -e '.target == "x86_64-unknown-linux-gnu" and .rust_toolchain == "1.92.0"' "$bundle/BUILD-INFO.json" >/dev/null
while IFS= read -r -d '' member; do
    case "${member##*/}" in
        wsdist-gui|data|defaults.json|item_list.csv|icons32|licenses|THIRD-PARTY-NOTICES.html|PROVENANCE.txt|BUILD-INFO.json) ;;
        *) echo "Unexpected archive root entry: $member" >&2; exit 1 ;;
    esac
done < <(find "$bundle" -mindepth 1 -maxdepth 1 -print0)
while IFS= read -r -d '' forbidden; do
    echo "Unexpected Python/Qt/reference content in archive: $forbidden" >&2
    exit 1
done < <(find "$bundle" \( -iname '*.py' -o -iname '*.pyc' -o -iname '*.pkl' -o -iname 'Qt[56]*.so*' \
    -o -iname 'libpython*.so*' -o -name fixtures -o -name migration \) -print0)
# libpng/Xlib are build-only automation prerequisites, not packaged GUI runtimes.
read -r -a native_flags <<<"$(pkg-config --cflags --libs x11 libpng)"
cc -std=c11 -O2 "$root/tools/native-window-x11.c" -o "$work/native-window-x11" "${native_flags[@]}"
(cd -- "$other_cwd" && exec env -u WSDIST_DATA_DIR -u PYTHONPATH -u PYTHONHOME -u QT_PLUGIN_PATH -u WAYLAND_DISPLAY \
    SLINT_BACKEND=winit-software WINIT_UNIX_BACKEND=x11 SLINT_SCALE_FACTOR="$scale" "$bundle/wsdist-gui") \
    > "$output/gui.stdout.log" 2> "$output/gui.stderr.log" &
pid=$!
"$work/native-window-x11" "$pid" "$output/gui.png" > "$output/window.json"
deadline=$((SECONDS + 30))
while kill -0 "$pid" 2>/dev/null; do
    ((SECONDS < deadline)) || { echo 'Native GUI did not exit after WM_DELETE_WINDOW' >&2; exit 1; }
    sleep 0.1
done
wait "$pid"
pid=''
jq -e '.status == "passed" and .visible == true and .rendered == true' "$output/window.json" >/dev/null
if [[ -n "$ui_smoke" ]]; then
    # Place the development-only driver beside the extracted resources, AFTER
    # archive hygiene checks. It is not present in the delivered archive.
    cp -- "$ui_smoke" "$bundle/ui_smoke"
    (cd -- "$other_cwd" && exec timeout 20m env -u WSDIST_DATA_DIR -u PYTHONPATH -u PYTHONHOME -u QT_PLUGIN_PATH -u WAYLAND_DISPLAY -u WAYLAND_SOCKET \
        SLINT_BACKEND=winit-software SLINT_SCALE_FACTOR="$scale" SLINT_EMIT_DEBUG_INFO=1 \
        "$work/native-window-x11" --focus-main "$bundle/ui_smoke" --output "$output/ui-workflows") > "$output/ui.stdout.log" 2> "$output/ui.stderr.log"
    jq -e --argjson scale "$scale" '.status == "passed" and .scale_factor == $scale' "$output/ui-workflows/report.json" >/dev/null
fi
status=passed
printf 'Extracted native archive proof: %s\n' "$output/report.json"
