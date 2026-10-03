#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Archive,
    [string] $Output = '',
    [string] $UiSmoke = '',
    [ValidateSet('1', '1.5')] [string] $Scale = '1'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'The archive must be exercised natively on Windows' }
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$Archive = (Resolve-Path -LiteralPath $Archive).Path
if (-not $Output) { $Output = Join-Path $root 'target/archive-smoke' }
$Output = [IO.Path]::GetFullPath($Output)
if ($UiSmoke) { $UiSmoke = (Resolve-Path -LiteralPath $UiSmoke).Path }
$null = [IO.Directory]::CreateDirectory($Output)
$work = Join-Path $Output ('.wsdist-archive-' + [Guid]::NewGuid().ToString('N'))
$bundle = Join-Path $work 'extracted'
$otherCwd = Join-Path $work 'unrelated working directory'
$null = [IO.Directory]::CreateDirectory($bundle)
$null = [IO.Directory]::CreateDirectory($otherCwd)
$process = $null
$uiProcess = $null
$status = 'running'
$failure = $null
function Start-Native([string] $Executable, [string[]] $Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.WorkingDirectory = $otherCwd
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    foreach ($key in @('WSDIST_DATA_DIR', 'PYTHONPATH', 'PYTHONHOME', 'QT_PLUGIN_PATH')) { $null = $start.Environment.Remove($key) }
    $start.Environment['SLINT_BACKEND'] = 'winit-software'
    $start.Environment['SLINT_SCALE_FACTOR'] = $Scale
    $start.Environment['SLINT_EMIT_DEBUG_INFO'] = '1'
    # App startup must not resolve tools/runtimes from a developer PATH.
    $start.Environment['PATH'] = "$env:SystemRoot\System32;$env:SystemRoot"
    $child = [Diagnostics.Process]::Start($start)
    [pscustomobject]@{ Process = $child; Stdout = $child.StandardOutput.ReadToEndAsync(); Stderr = $child.StandardError.ReadToEndAsync() }
}
function Save-Logs($Started, [string] $Prefix) {
    if ($Started -and $Started.Process.HasExited) {
        [IO.File]::WriteAllText((Join-Path $Output "$Prefix.stdout.log"), $Started.Stdout.GetAwaiter().GetResult())
        [IO.File]::WriteAllText((Join-Path $Output "$Prefix.stderr.log"), $Started.Stderr.GetAwaiter().GetResult())
    }
}
$started = $null
$uiStarted = $null
try {
Add-Type -AssemblyName System.Drawing.Common
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class WsdistNativeWindow {
    public delegate bool EnumWindow(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindow callback, IntPtr data);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("gdi32.dll")] static extern uint GetPixel(IntPtr dc, int x, int y);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public static string Title(IntPtr window) {
        var text = new StringBuilder(4096); GetWindowText(window, text, text.Capacity); return text.ToString();
    }
    public static IntPtr Find(uint pid) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner == pid && IsWindowVisible(window)) {
                string title = Title(window);
                if (title.StartsWith("Kastra FFXI Damage Simulator", StringComparison.Ordinal) || title.Contains("Startup Error")) {
                    result = window; return false;
                }
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }
    public static bool Rendered(IntPtr dc, int width, int height) {
        int[] min = {765, 765, 765, 765}, max = {0, 0, 0, 0};
        int top = height > 120 ? 60 : 0;
        // Body quadrants distinguish real painted controls from a mapped title
        // over a blank client surface, without imposing a golden pixel style.
        for (int y = top; y < height; y += 3) {
            for (int x = 0; x < width; x += 3) {
                uint pixel = GetPixel(dc, x, y); if (pixel == 0xffffffff) return false;
                int value = (int)((pixel & 255) + ((pixel >> 8) & 255) + ((pixel >> 16) & 255));
                int quadrant = (x >= width / 2 ? 1 : 0) + (y >= top + (height - top) / 2 ? 2 : 0);
                min[quadrant] = Math.Min(min[quadrant], value); max[quadrant] = Math.Max(max[quadrant], value);
            }
        }
        for (int index = 0; index < 4; ++index) if (max[index] - min[index] < 192) return false;
        return true;
    }
}
'@
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $members = foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\', '/')
            if ($name.StartsWith('/') -or $name -match '(^|/)\.\.(/|$)' -or $name -match '^[A-Za-z]:') { throw "Unsafe archive member: $name" }
            $name
        }
        $members | Set-Content -LiteralPath (Join-Path $Output 'archive-members.txt') -Encoding utf8
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $bundle)
    foreach ($name in @('wsdist-gui.exe', 'defaults.json', 'item_list.csv', 'THIRD-PARTY-NOTICES.html', 'PROVENANCE.txt', 'BUILD-INFO.json')) {
        if (-not (Test-Path -LiteralPath (Join-Path $bundle $name) -PathType Leaf)) { throw "Missing archive content: $name" }
    }
    foreach ($name in @('gear.json', 'enemies.json', 'buffs.json', 'pup-attachments.json')) {
        if (-not (Test-Path -LiteralPath (Join-Path $bundle "data/$name") -PathType Leaf)) { throw "Missing bundled catalog: $name" }
    }
    foreach ($name in @('icons32', 'licenses')) {
        if (-not (Test-Path -LiteralPath (Join-Path $bundle $name) -PathType Container)) { throw "Missing archive directory: $name" }
    }
    $allowed = @('wsdist-gui.exe', 'data', 'defaults.json', 'item_list.csv', 'icons32', 'licenses', 'THIRD-PARTY-NOTICES.html', 'PROVENANCE.txt', 'BUILD-INFO.json')
    foreach ($item in Get-ChildItem -LiteralPath $bundle -Force) { if ($item.Name -notin $allowed) { throw "Unexpected archive root entry: $($item.Name)" } }
    foreach ($item in Get-ChildItem -LiteralPath $bundle -Recurse -Force) {
        if ($item.Name -match '\.(py|pyc|pkl)$|^(Qt[56]|python).*\.dll$|^(fixtures|migration)$') { throw "Unexpected Python/Qt/reference content: $($item.FullName)" }
    }
    $build = Get-Content -LiteralPath (Join-Path $bundle 'BUILD-INFO.json') -Raw | ConvertFrom-Json
    if ($build.target -ne 'x86_64-pc-windows-msvc' -or $build.rust_toolchain -ne '1.92.0') { throw 'The archive is not the pinned native Windows build' }
    $started = Start-Native (Join-Path $bundle 'wsdist-gui.exe') @()
    $process = $started.Process
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $window = [IntPtr]::Zero
    while ($clock.Elapsed.TotalSeconds -lt 30 -and -not $process.HasExited) {
        $window = [WsdistNativeWindow]::Find([uint32] $process.Id)
        if ($window -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    }
    if ($window -eq [IntPtr]::Zero) { throw 'No expected visible native application window appeared' }
    $title = [WsdistNativeWindow]::Title($window)
    $null = [WsdistNativeWindow]::SetWindowPos($window, [IntPtr]::Zero, 20, 20, 1280, 900, 0)
    $null = [WsdistNativeWindow]::SetForegroundWindow($window)
    Start-Sleep -Milliseconds 1200
    $rect = [WsdistNativeWindow+Rect]::new()
    if (-not [WsdistNativeWindow]::GetWindowRect($window, [ref] $rect)) { throw 'Cannot observe native window geometry' }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -lt 320 -or $height -lt 240 -or -not [WsdistNativeWindow]::IsWindowVisible($window)) { throw 'The native window is not a visible desktop surface' }
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $rendered = $false
    $method = 'PrintWindow'
    try {
        $dc = $graphics.GetHdc()
        try {
            $printed = [WsdistNativeWindow]::PrintWindow($window, $dc, 2)
            if ($printed) { $rendered = [WsdistNativeWindow]::Rendered($dc, $width, $height) }
        } finally { $graphics.ReleaseHdc($dc) }
        if (-not $rendered) {
            # An observed screen capture is a real backend alternative, not a
            # fabricated/offscreen/testing-backend screenshot.
            $method = 'Visible desktop CopyFromScreen'
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, [Drawing.Size]::new($width, $height))
            $dc = $graphics.GetHdc()
            try { $rendered = [WsdistNativeWindow]::Rendered($dc, $width, $height) } finally { $graphics.ReleaseHdc($dc) }
        }
        $bitmap.Save((Join-Path $Output 'gui.png'), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    $modules = @($process.Modules | ForEach-Object { [ordered]@{ name = $_.ModuleName; path = $_.FileName } })
    $modules | ConvertTo-Json -Depth 5 -AsArray | Set-Content -LiteralPath (Join-Path $Output 'loaded-modules.json') -Encoding utf8
    if (@($modules | Where-Object { $_.name -match '^(python|PySide|Qt[56])' }).Count -ne 0) { throw 'The native application unexpectedly loaded a Python/Qt runtime' }
    [ordered]@{ title = $title; visible = $true; rendered = $rendered; width = $width; height = $height; capture = $method } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Output 'window.json') -Encoding utf8
    if (-not $rendered) { throw 'Native client frame is blank or not painted across its body' }
    if (-not $title.StartsWith('Kastra FFXI Damage Simulator', [StringComparison]::Ordinal)) { throw "Startup-error surface: $title" }
    if (-not [WsdistNativeWindow]::PostMessage($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Could not send native WM_CLOSE' }
    if (-not $process.WaitForExit(30000)) { throw 'The GUI did not exit after native WM_CLOSE' }
    Save-Logs $started 'gui'
    if ($process.ExitCode -ne 0) { throw "Native GUI exited with $($process.ExitCode)" }
    if ($UiSmoke) {
        # The development-only driver is added only to the extracted proof copy,
        # never to the release archive. Its resources are executable-relative.
        $driver = Join-Path $bundle 'ui_smoke.exe'
        Copy-Item -LiteralPath $UiSmoke -Destination $driver
        $uiOutput = Join-Path $Output 'ui-workflows'
        $uiStarted = Start-Native $driver @('--output', $uiOutput)
        $uiProcess = $uiStarted.Process
        if (-not $uiProcess.WaitForExit(1200000)) { throw 'Native UI workflows did not complete within 20 minutes' }
        Save-Logs $uiStarted 'ui'
        if ($uiProcess.ExitCode -ne 0) { throw "Native ui_smoke exited with $($uiProcess.ExitCode)" }
        $uiReport = Get-Content -LiteralPath (Join-Path $uiOutput 'report.json') -Raw | ConvertFrom-Json
        if ($uiReport.status -ne 'passed' -or $uiReport.scale_factor -ne [double] $Scale) { throw 'Native UI workflows or requested scale did not pass' }
    }
    $status = 'passed'
    Write-Host "Extracted native archive proof: $(Join-Path $Output 'report.json')"
} catch {
    $failure = $_.Exception.Message
    $status = 'failed'
    throw
} finally {
    foreach ($child in @($process, $uiProcess)) {
        if ($child -and -not $child.HasExited) { $child.Kill($true); $null = $child.WaitForExit(30000) }
    }
    Save-Logs $started 'gui'
    Save-Logs $uiStarted 'ui'
    [ordered]@{ status = $status; platform = 'windows'; archive = $Archive; archive_sha256 = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant();
        requested_scale = [double] $Scale; working_directory = $otherCwd; executable = (Join-Path $bundle 'wsdist-gui.exe');
        resource_mode = 'Executable-relative; WSDIST_DATA_DIR unset; unrelated empty CWD'; production_surface = 'gui.png'; production_window = 'window.json';
        loaded_modules = 'loaded-modules.json'; ui_workflows = $(if ($UiSmoke) { 'ui-workflows/report.json' } else { 'Not requested' }); error = $failure;
        limits = @('Native file-dialog interaction requires the manual desktop checklist') } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Output 'report.json') -Encoding utf8
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
}
