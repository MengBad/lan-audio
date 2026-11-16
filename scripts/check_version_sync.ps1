#!/usr/bin/env pwsh
# Verify every version surface derives from the single source: VERSION.
# Markdown backticks are matched as \x60 to keep this script ASCII-clean.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$versionBytes = [System.IO.File]::ReadAllBytes((Join-Path $repoRoot 'VERSION'))
if ($versionBytes.Length -ge 3 -and $versionBytes[0] -eq 0xEF -and $versionBytes[1] -eq 0xBB -and $versionBytes[2] -eq 0xBF) {
    throw 'VERSION must be pure ASCII without BOM'
}
foreach ($b in $versionBytes) {
    if ($b -gt 0x7F) { throw 'VERSION contains a non-ASCII byte' }
}
$short = ([System.Text.Encoding]::ASCII.GetString($versionBytes)).Trim()
if ($short -notmatch '^(\d+)\.(\d+)(?:\.(\d+))?$') { throw "Invalid VERSION: $short" }
$major = [int]$Matches[1]
$minor = [int]$Matches[2]
$patch = if ($Matches[3]) { [int]$Matches[3] } else { 0 }
$semver = "$major.$minor.$patch"
$androidCode = 2000 + ($major * 1000) + ($minor * 10) + $patch
$lf = [string][char]10
$crlfSeq = [string][char]13 + $lf

$checks = @(
    @{ Path = 'Cargo.toml'; Pattern = '(?m)^version = "([^"]+)"$'; Expected = $semver; Label = 'workspace.package.version' },
    @{ Path = 'apps/desktop/src-tauri/Cargo.toml'; Pattern = '(?m)^version = "([^"]+)"$'; Expected = $semver; Label = 'desktop Cargo version' },
    @{ Path = 'apps/desktop/src-tauri/tauri.conf.json'; Pattern = '"version":\s*"([^"]+)"'; Expected = $semver; Label = 'tauri.conf.json version' },
    @{ Path = 'apps/android_flutter/pubspec.yaml'; Pattern = '(?m)^version:\s*([^\r\n]+)$'; Expected = "$semver+$androidCode"; Label = 'pubspec version+code' },
    @{ Path = 'apps/android_flutter/lib/main.dart'; Pattern = "const String kAppVersion = '([^']+)';"; Expected = $short; Label = 'kAppVersion' },
    @{ Path = 'apps/android_flutter/android/local.properties'; Pattern = '(?m)^flutter\.versionName=([^\r\n]*)$'; Expected = $short; Label = 'flutter.versionName' },
    @{ Path = 'apps/android_flutter/android/local.properties'; Pattern = '(?m)^flutter\.versionCode=([^\r\n]*)$'; Expected = [string]$androidCode; Label = 'flutter.versionCode' },
    @{ Path = 'README.md'; Pattern = '(?m)^Current version: \x60([^\x60]+)\x60$'; Expected = $short; Label = 'README.md marker' },
    @{ Path = 'README.zh-CN.md'; Pattern = '(?m)^当前版本（短版本）：\x60([^\x60]+)\x60$'; Expected = $short; Label = 'README.zh-CN.md marker' },
    @{ Path = 'docs/roadmap.md'; Pattern = '(?m)^- Current version \(short version\): \x60([^\x60]+)\x60$'; Expected = $short; Label = 'roadmap marker' },
    @{ Path = 'docs/RELEASE_POLICY.md'; Pattern = '(?m)^- Current version \(short version\): \x60([^\x60]+)\x60$'; Expected = $short; Label = 'release policy marker' },
    @{ Path = 'docs/todo.md'; Pattern = '(?m)^- Current version \(short version\): \x60([^\x60]+)\x60$'; Expected = $short; Label = 'todo marker' },
    @{ Path = 'CHANGELOG.md'; Pattern = '(?m)^## \[([^\]]+)\]'; Expected = $short; Label = 'latest CHANGELOG entry' }
)

$errors = New-Object System.Collections.Generic.List[string]
foreach ($c in $checks) {
    $full = Join-Path $repoRoot $c.Path
    if (-not (Test-Path -LiteralPath $full)) { $errors.Add("$($c.Label): missing $($c.Path)"); continue }
    $raw = (Get-Content -Raw -LiteralPath $full).Replace($crlfSeq, $lf)
    $m = [regex]::Match($raw, $c.Pattern)
    if (-not $m.Success) { $errors.Add("$($c.Label): pattern not found in $($c.Path)"); continue }
    if ($m.Groups[1].Value -ne $c.Expected) {
        $errors.Add("$($c.Label): expected '$($c.Expected)' got '$($m.Groups[1].Value)' ($($c.Path))")
    }
}

$service = (Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'crates/lan_audio_server/src/service.rs')).Replace($crlfSeq, $lf)
if ($service -notmatch 'version:\s*env!\("CARGO_PKG_VERSION"\)\.to_string\(\)') {
    $errors.Add('mDNS version is hardcoded; use env!("CARGO_PKG_VERSION")')
}

if ($errors.Count -gt 0) {
    throw ("Version sync failed:" + $lf + "  " + ($errors -join ($lf + "  ")))
}
Write-Host "PASS: version sync v$short (semver=$semver androidCode=$androidCode) across $($checks.Count) surfaces"
exit 0
