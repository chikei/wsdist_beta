#Requires -Version 7.0
[CmdletBinding()]
param(
    [string] $Output = '',
    [switch] $SkipBuild
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'Packaging requires native x86_64 Windows'
}
. (Join-Path $PSScriptRoot 'windows-file-type.ps1')
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Output) { $Output = Join-Path $root 'target/dist' }
$Output = [IO.Path]::GetFullPath($Output)
$target = 'x86_64-pc-windows-msvc'
function Invoke-Checked([string] $Program, [string[]] $Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program exited with $LASTEXITCODE" }
}
function Html([object] $Value) { [Net.WebUtility]::HtmlEncode([string] $Value) }
# Do not depend on a separately installed VC++ runtime in a portable archive.
# The same linkage flag is used by the native Windows CI job.
if ($env:CARGO_ENCODED_RUSTFLAGS) { throw 'Unset CARGO_ENCODED_RUSTFLAGS before native packaging; it overrides the required CRT linkage flag' }
$oldFlags = $env:RUSTFLAGS
$env:RUSTFLAGS = "$oldFlags -C target-feature=+crt-static".Trim()
$work = Join-Path $Output ('.wsdist-package-' + [Guid]::NewGuid().ToString('N'))
Push-Location $root
try {
    $version = (Invoke-Checked 'cargo' @('+1.92.0', 'about', '--version')) -join "`n"
    if ($version.Trim() -ne 'cargo-about 0.8.2') {
        throw 'Install the pinned notice tool: cargo +1.92.0 install --locked cargo-about --version 0.8.2'
    }
    if (-not $SkipBuild) {
        Invoke-Checked 'cargo' @('+1.92.0', 'build', '--locked', '--release', '--target', $target, '-p', 'wsdist-app', '--bin', 'wsdist-gui')
    }
    $metadataJson = (Invoke-Checked 'cargo' @('+1.92.0', 'metadata', '--locked', '--format-version', '1', '--no-deps')) -join "`n"
    $metadata = $metadataJson | ConvertFrom-Json
    $binary = Join-Path $metadata.target_directory "$target/release/wsdist-gui.exe"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Missing release executable: $binary" }
    $payload = Join-Path $work 'payload'
    $null = [IO.Directory]::CreateDirectory((Join-Path $payload 'data'))
    $null = [IO.Directory]::CreateDirectory((Join-Path $payload 'licenses/crates'))
    Copy-Item -LiteralPath $binary -Destination (Join-Path $payload 'wsdist-gui.exe')
    foreach ($name in @('enemies.json5', 'buffs.json5', 'pup-attachments.json5')) {
        $path = Join-Path $root "data/$name"
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing required catalog: $path" }
        $item = Get-Item -LiteralPath $path -Force
        if (-not (Get-WsdistFileType $item).IsFile) {
            throw "Required catalog is not a regular file: $path"
        }
        Copy-Item -LiteralPath $path -Destination (Join-Path $payload 'data')
    }
    $gearRoot = Join-Path $root 'data/gear'
    $gearDirectory = Get-Item -LiteralPath $gearRoot -Force
    if (-not (Get-WsdistFileType $gearDirectory).IsDirectory) {
        throw "Missing regular gear directory: $gearRoot"
    }
    # Enumerate explicitly without following name-surrogate links/junctions.
    # Culture comparisons and -Filter are not exact extension checks.
    $gearDirectories = [Collections.Generic.Stack[string]]::new()
    $gearDirectories.Push($gearRoot)
    $gearCount = 0
    $null = [IO.Directory]::CreateDirectory((Join-Path $payload 'data/gear'))
    while ($gearDirectories.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $gearDirectories.Pop() -Force) {
            $type = Get-WsdistFileType $item
            if ($type.IsSymlink) { continue }
            if ($type.IsDirectory) {
                $gearDirectories.Push($item.FullName)
            } elseif ($type.IsFile -and
                [string]::Equals($item.Extension, '.json5', [StringComparison]::Ordinal) -and
                -not [string]::Equals($item.Name, '.json5', [StringComparison]::Ordinal)) {
                $relative = [IO.Path]::GetRelativePath($gearRoot, $item.FullName)
                $destination = Join-Path (Join-Path $payload 'data/gear') $relative
                $null = [IO.Directory]::CreateDirectory((Split-Path -Parent $destination))
                Copy-Item -LiteralPath $item.FullName -Destination $destination
                $gearCount++
            }
        }
    }
    if ($gearCount -eq 0) { throw "No gear fragments found: $gearRoot" }
    foreach ($name in @('defaults.json', 'item_list.csv')) { Copy-Item -LiteralPath (Join-Path $root $name) -Destination $payload }
    Copy-Item -LiteralPath (Join-Path $root 'icons32') -Destination (Join-Path $payload 'icons32') -Recurse
    Copy-Item -LiteralPath (Join-Path $root 'tools/licenses/PROVENANCE.txt') -Destination (Join-Path $payload 'PROVENANCE.txt')
    foreach ($name in @('OFL-1.1-RFN.txt', 'Slint-asset-attributions.txt')) {
        Copy-Item -LiteralPath (Join-Path $root "tools/licenses/$name") -Destination (Join-Path $payload 'licenses')
    }

    # Complete metadata can need other-platform source before target filtering.
    # Fetch the locked source cache, then scan licensing offline.
    Invoke-Checked 'cargo' @('+1.92.0', 'fetch', '--locked')
    $noticePath = Join-Path $work 'notices.json'
    Invoke-Checked 'cargo' @('+1.92.0', 'about', 'generate', '--locked', '--offline', '--fail',
        '--manifest-path', (Join-Path $root 'crates/wsdist-app/Cargo.toml'), '--target', $target,
        '--config', (Join-Path $root 'tools/licenses/about.toml'), '--format', 'json', '--output-file', $noticePath)
    $notices = Get-Content -LiteralPath $noticePath -Raw | ConvertFrom-Json
    $slint = @($notices.crates | Where-Object { $_.package.name -eq 'slint' -and $_.package.version -eq '1.16.0' })
    if ($slint.Count -ne 1) { throw 'Pinned Slint 1.16.0 must be present in the application graph' }
    $inventory = foreach ($item in $notices.crates) {
        $package = $item.package
        [ordered]@{ name = $package.name; version = $package.version; declared_license = $package.license;
            detected_license = $item.license; authors = $package.authors; repository = $package.repository;
            source = $package.source; source_archive = "https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download" }
        $crateRoot = Split-Path -Parent $package.manifest_path
        $destination = Join-Path $payload "licenses/crates/$($package.name)-$($package.version)"
        $null = [IO.Directory]::CreateDirectory($destination)
        foreach ($document in Get-ChildItem -LiteralPath $crateRoot -File -Recurse) {
            $relative = [IO.Path]::GetRelativePath($crateRoot, $document.FullName)
            if ($document.Name -match '^(LICENSE|LICENCE)([.\-].*)?$|^(COPYING|COPYRIGHT|NOTICE|UNLICENSE).*|\.license$' -or
                $relative -match '(^|[\\/])LICENSES[\\/]') {
                $to = Join-Path $destination $relative
                $null = [IO.Directory]::CreateDirectory((Split-Path -Parent $to))
                Copy-Item -LiteralPath $document.FullName -Destination $to
            }
        }
        if ($package.license_file) {
            $document = if ([IO.Path]::IsPathRooted($package.license_file)) { $package.license_file } else { Join-Path $crateRoot $package.license_file }
            Copy-Item -LiteralPath $document -Destination (Join-Path $destination 'DECLARED-LICENSE')
        }
    }
    $inventory | ConvertTo-Json -Depth 10 -AsArray | Set-Content -LiteralPath (Join-Path $payload 'licenses/dependencies.json') -Encoding utf8
    $slintLicense = Join-Path (Split-Path -Parent $slint[0].package.manifest_path) 'LICENSES/LicenseRef-Slint-Royalty-free-2.0.md'
    if ((Get-FileHash -LiteralPath $slintLicense -Algorithm SHA256).Hash.ToLowerInvariant() -ne '5167f5056e850419106ab6265efbdca7cba4d99c849d1445ca0bbf6a1e2315fe') {
        throw "Unexpected Slint application-license contents: $slintLicense"
    }
    Copy-Item -LiteralPath $slintLicense -Destination (Join-Path $payload 'licenses/Slint-Royalty-free-2.0.txt')
    $sysroot = ((Invoke-Checked 'rustc' @('+1.92.0', '--print', 'sysroot')) -join "`n").Trim()
    Copy-Item -LiteralPath (Join-Path $sysroot 'share/doc/rust/COPYRIGHT-library.html') -Destination (Join-Path $payload 'licenses/Rust-standard-library.html')
    $logoLicense = Join-Path $payload 'licenses/crates/i-slint-compiler-1.16.0/LICENSES/CC-BY-ND-4.0.txt'
    if (-not (Test-Path -LiteralPath $logoLicense -PathType Leaf)) { throw 'Missing AboutSlint artwork license text' }

    $html = [Text.StringBuilder]::new()
    $null = $html.AppendLine('<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>WSDist third-party notices</title><style>body{max-width:75rem;margin:2rem auto;padding:0 1rem;font-family:sans-serif}pre{white-space:pre-wrap;overflow-wrap:anywhere}td,th{padding:.4rem;text-align:left;vertical-align:top}</style></head><body><h1>WSDist third-party software notices</h1>')
    $null = $html.AppendLine('<p>Generated from the locked application graph with cargo-about 0.8.2. Build dependencies are included as a conservative superset. Upstream expressions are not a license grant for WSDist code, game data or icons. <a href="PROVENANCE.txt">Provenance and licensing boundaries</a>.</p><p>For OR expressions the policy prefers MIT, then Apache-2.0; all required AND terms are retained. Slint 1.16.0 uses its <a href="licenses/Slint-Royalty-free-2.0.txt">Royalty-free Applications License 2.0</a> and top-level Help &gt; About Slint attribution. Cargo-about cannot render custom LicenseRef text; the full text is supplied separately.</p><p><a href="licenses/Slint-asset-attributions.txt">Slint artwork/font attributions</a>, <a href="licenses/OFL-1.1-RFN.txt">Inter font license</a>, <a href="licenses/crates/i-slint-compiler-1.16.0/LICENSES/CC-BY-ND-4.0.txt">AboutSlint logo license</a> and <a href="licenses/Rust-standard-library.html">Rust 1.92.0 standard-library notices</a> supplement the crate texts below. Original and alternative documents are retained in licenses/crates/.</p><h2>Resolved third-party crates and exact source</h2><table><tr><th>Crate</th><th>Version</th><th>Declared expression</th><th>Source archive</th></tr>')
    foreach ($item in $notices.crates) {
        $package = $item.package
        $repository = if ($package.repository) { $package.repository } else { "https://crates.io/crates/$($package.name)" }
        $source = "https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download"
        $expression = if ($package.license) { $package.license } else { $item.license }
        $null = $html.AppendLine("<tr><td><a href=`"$(Html $repository)`">$(Html $package.name)</a></td><td>$(Html $package.version)</td><td>$(Html $expression)</td><td><a href=`"$(Html $source)`">Released source</a></td></tr>")
    }
    $null = $html.AppendLine('</table><h2>Selected standard license texts and upstream copyright notices</h2>')
    foreach ($license in $notices.licenses) {
        $users = @($license.used_by | ForEach-Object { "$($_.crate.name) $($_.crate.version)" }) -join ', '
        $null = $html.AppendLine("<section><h3>$(Html $license.name) ($(Html $license.id))</h3><p>Used by: $(Html $users)</p><pre>$(Html $license.text)</pre></section>")
    }
    $null = $html.AppendLine('</body></html>')
    [IO.File]::WriteAllText((Join-Path $payload 'THIRD-PARTY-NOTICES.html'), $html.ToString(), [Text.UTF8Encoding]::new($false))
    [ordered]@{ target = $target; rust_toolchain = '1.92.0'; cargo_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $root 'Cargo.lock')).Hash.ToLowerInvariant();
        notice_generator = 'cargo-about 0.8.2'; requested_rustflags = $env:RUSTFLAGS;
        verification = 'Packaging is not an execution or numerical-verification claim' } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $payload 'BUILD-INFO.json') -Encoding utf8
    $archiveName = 'wsdist-windows-x86_64.zip'
    $temporaryArchive = Join-Path $work $archiveName
    [IO.Compression.ZipFile]::CreateFromDirectory($payload, $temporaryArchive)
    $archive = Join-Path $Output $archiveName
    Move-Item -LiteralPath $temporaryArchive -Destination $archive -Force
    "$((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant())  $archiveName" |
        Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
    Write-Host "Native archive: $archive"
} finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
    $env:RUSTFLAGS = $oldFlags
    Pop-Location
}
