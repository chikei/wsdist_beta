#!/usr/bin/env bash
# Native-only release archive. Run with bash; no Python or project license election.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
output="$root/target/dist"
skip_build=false
while (($#)); do
    case "$1" in
        --output) [[ $# -ge 2 ]] || { echo '--output needs a directory' >&2; exit 2; }; output=$2; shift 2 ;;
        --skip-build) skip_build=true; shift ;;
        --help) echo 'bash tools/package-linux.sh [--output DIR] [--skip-build]'; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'Packaging requires native x86_64 Linux' >&2; exit 1; }
cd -- "$root"
for tool in cargo rustc jq tar gzip sha256sum find; do
    command -v "$tool" >/dev/null || { echo "Missing packaging prerequisite: $tool" >&2; exit 1; }
done
[[ $(cargo +1.92.0 about --version) == 'cargo-about 0.8.2' ]] || {
    echo 'Install the pinned build-only notice tool: cargo +1.92.0 install --locked cargo-about --version 0.8.2' >&2
    exit 1
}
target=x86_64-unknown-linux-gnu
if ! $skip_build; then
    cargo +1.92.0 build --locked --release --target "$target" -p wsdist-app --bin wsdist-gui
fi
# Respect Cargo's target-directory setting, including workspace configuration.
metadata=$(cargo +1.92.0 metadata --locked --format-version 1 --no-deps)
target_directory=$(jq -r '.target_directory' <<<"$metadata")
binary="$target_directory/$target/release/wsdist-gui"
[[ -x "$binary" ]] || { echo "Missing release executable: $binary" >&2; exit 1; }
mkdir -p -- "$output"
output=$(cd -- "$output" && pwd -P)
work=$(mktemp -d "$output/.wsdist-package.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
payload="$work/payload"
mkdir -p -- "$payload/data" "$payload/licenses/crates"
cp -- "$binary" "$payload/wsdist-gui"
for name in enemies.json5 buffs.json5 pup-attachments.json5; do
    path="$root/data/$name"
    [[ -f "$path" && ! -L "$path" ]] || { echo "Missing regular required catalog: $path" >&2; exit 1; }
    cp -- "$path" "$payload/data/"
done
gear_root="$root/data/gear"
[[ -d "$gear_root" && ! -L "$gear_root" ]] || { echo "Missing regular gear directory: $gear_root" >&2; exit 1; }
# Match runtime discovery: exact lowercase extension, regular files only, and
# never follow symlinked files or directories. Preserve every nested path.
find -P "$gear_root" -type f -name '*.json5' ! -name '.json5' -print0 > "$work/gear-fragments"
gear_count=0
mkdir -p -- "$payload/data/gear"
while IFS= read -r -d '' path; do
    destination="$payload/data/gear/${path#"$gear_root/"}"
    mkdir -p -- "${destination%/*}"
    cp -- "$path" "$destination"
    gear_count=$((gear_count + 1))
done < "$work/gear-fragments"
((gear_count > 0)) || { echo "No gear fragments found: $gear_root" >&2; exit 1; }
cp -- "$root/defaults.json" "$root/item_list.csv" "$payload/"
cp -R -- "$root/icons32" "$payload/icons32"
cp -- "$root/tools/licenses/PROVENANCE.txt" "$payload/PROVENANCE.txt"
cp -- "$root/tools/licenses/OFL-1.1-RFN.txt" "$root/tools/licenses/Slint-asset-attributions.txt" "$payload/licenses/"

# cargo-about gathers complete metadata before target filtering. Cache every
# locked package (including other-platform source), then scan licensing offline.
# No other-platform code or mutable licensing-service result is packaged.
cargo +1.92.0 fetch --locked
notice_data="$work/notices.json"
cargo +1.92.0 about generate --locked --offline --fail \
    --manifest-path "$root/crates/wsdist-app/Cargo.toml" --target "$target" \
    --config "$root/tools/licenses/about.toml" --format json --output-file "$notice_data"
jq -e 'any(.crates[]; .package.name == "slint" and .package.version == "1.16.0")' "$notice_data" >/dev/null
jq '[.crates[] | {name: .package.name, version: .package.version,
    declared_license: .package.license, detected_license: .license,
    authors: .package.authors, repository: .package.repository, source: .package.source,
    source_archive: ("https://crates.io/api/v1/crates/" + .package.name + "/" + .package.version + "/download")}]' \
    "$notice_data" > "$payload/licenses/dependencies.json"

while IFS=$'\t' read -r name version manifest; do
    crate_root=${manifest%/*}
    crate_destination="$payload/licenses/crates/$name-$version"
    mkdir -p -- "$crate_destination"
    while IFS= read -r -d '' document; do
        relative=${document#"$crate_root/"}
        # A root-level filename has no directory component.
        if [[ "$relative" != */* ]]; then
            cp -- "$document" "$crate_destination/"
        else
            mkdir -p -- "${crate_destination}/${relative%/*}"
            cp -- "$document" "$crate_destination/$relative"
        fi
    done < <(find "$crate_root" -type f \( -iname 'license' -o -iname 'license.*' -o -iname 'license-*' \
        -o -iname 'licence' -o -iname 'licence.*' -o -iname 'licence-*' \
        -o -iname 'copying*' -o -iname 'copyright*' -o -iname 'notice*' -o -iname 'unlicense*' \
        -o -iname '*.license' -o -ipath '*/licenses/*' \) -print0)
    if [[ "$name" == slint ]]; then
        license="$crate_root/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md"
        actual=$(sha256sum -- "$license"); actual=${actual%% *}
        [[ "$actual" == 5167f5056e850419106ab6265efbdca7cba4d99c849d1445ca0bbf6a1e2315fe ]] || {
            echo "Unexpected Slint application-license contents: $license" >&2; exit 1
        }
        cp -- "$license" "$payload/licenses/Slint-Royalty-free-2.0.txt"
    fi
done < <(jq -r '.crates[] | [.package.name, .package.version, .package.manifest_path] | @tsv' "$notice_data")
# Also preserve a manifest's explicitly named license-file, even if its filename
# is unconventional. cargo-about embeds any harvested text in the HTML below.
while IFS=$'\t' read -r name version manifest license_file; do
    crate_root=${manifest%/*}
    case "$license_file" in
        /*) document=$license_file ;;
        *) document="$crate_root/$license_file" ;;
    esac
    [[ -f "$document" ]] || { echo "Missing declared license-file: $document" >&2; exit 1; }
    cp -- "$document" "$payload/licenses/crates/$name-$version/DECLARED-LICENSE"
done < <(jq -r '.crates[] | select(.package.license_file != null) | [.package.name, .package.version, .package.manifest_path, .package.license_file] | @tsv' "$notice_data")

sysroot=$(rustc +1.92.0 --print sysroot)
rust_notices="$sysroot/share/doc/rust/COPYRIGHT-library.html"
[[ -f "$rust_notices" ]] || { echo "Missing pinned Rust standard-library notices: $rust_notices" >&2; exit 1; }
cp -- "$rust_notices" "$payload/licenses/Rust-standard-library.html"
# The full logo license is shipped with the pinned Slint compiler crate.
[[ -f "$payload/licenses/crates/i-slint-compiler-1.16.0/LICENSES/CC-BY-ND-4.0.txt" ]] || {
    echo 'Missing AboutSlint artwork license text' >&2; exit 1
}
jq -r -f "$root/tools/licenses/render-notices.jq" "$notice_data" > "$payload/THIRD-PARTY-NOTICES.html"
lock_hash=$(sha256sum -- "$root/Cargo.lock"); lock_hash=${lock_hash%% *}
jq -n --arg target "$target" --arg lock "$lock_hash" --arg rustflags "${RUSTFLAGS-}" \
    '{target: $target, rust_toolchain: "1.92.0", cargo_lock_sha256: $lock,
      notice_generator: "cargo-about 0.8.2", rustflags: $rustflags,
      verification: "Packaging is not an execution or numerical-verification claim"}' > "$payload/BUILD-INFO.json"
archive=wsdist-linux-x86_64.tar.gz
# Fresh allowlisted staging prevents snapshots, fixtures, caches or tool binaries
# from leaking into the release. Preserve executable permission, not host owners.
tar --create --directory "$payload" --file - --owner=0 --group=0 --numeric-owner \
    wsdist-gui data defaults.json item_list.csv icons32 licenses \
    THIRD-PARTY-NOTICES.html PROVENANCE.txt BUILD-INFO.json | gzip -n > "$work/$archive"
mv -f -- "$work/$archive" "$output/$archive"
(cd -- "$output" && sha256sum -- "$archive" > "$archive.sha256")
printf 'Native archive: %s\n' "$output/$archive"
