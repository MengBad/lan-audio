param(
    [string]$Version
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    [System.IO.File]::WriteAllText((Resolve-Path $Path), $Content, $Utf8NoBom)
}

function Parse-ShortVersion {
    param([Parameter(Mandatory = $true)][string]$InputVersion)

    if ($InputVersion -notmatch '^(\d+)\.(\d+)(?:\.(\d+))?$') {
        throw "Invalid version '$InputVersion'. Expected format: major.minor or major.minor.patch (e.g. 1.2 or 1.2.3)"
    }

    return [pscustomobject]@{
        Major = [int]$Matches[1]
        Minor = [int]$Matches[2]
        Patch = if ($Matches[3]) { [int]$Matches[3] } else { 0 }
        HasPatch = -not [string]::IsNullOrWhiteSpace($Matches[3])
    }
}

function Get-NextShortVersion {
    param([Parameter(Mandatory = $true)][string]$Current)
    $parsed = Parse-ShortVersion -InputVersion $Current
    return "{0}.{1}" -f $parsed.Major, ($parsed.Minor + 1)
}

function Get-ShortVersionString {
    param([Parameter(Mandatory = $true)]$ParsedVersion)

    if ($ParsedVersion.HasPatch) {
        return "{0}.{1}.{2}" -f $ParsedVersion.Major, $ParsedVersion.Minor, $ParsedVersion.Patch
    }

    return "{0}.{1}" -f $ParsedVersion.Major, $ParsedVersion.Minor
}

function Get-SemverString {
    param([Parameter(Mandatory = $true)]$ParsedVersion)

    return "{0}.{1}.{2}" -f $ParsedVersion.Major, $ParsedVersion.Minor, $ParsedVersion.Patch
}

function Get-AndroidVersionCode {
    param([Parameter(Mandatory = $true)]$ParsedVersion)

    return 2000 + ($ParsedVersion.Major * 1000) + ($ParsedVersion.Minor * 10) + $ParsedVersion.Patch
}

function Update-TomlVersionInSection {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$SectionName,
        [Parameter(Mandatory = $true)][string]$NewSemver
    )

    $lines = Get-Content $Path
    $inSection = $false
    $updated = $false

    for ($i = 0; $i -lt $lines.Length; $i++) {
        $line = $lines[$i]

        if ($line -match '^\[(.+)\]\s*$') {
            $inSection = ($Matches[1] -eq $SectionName)
            continue
        }

        if ($inSection -and $line -match '^version\s*=\s*"\d+\.\d+(?:\.\d+)?"\s*$') {
            $lines[$i] = "version = `"$NewSemver`""
            $updated = $true
            break
        }
    }

    if (-not $updated) {
        throw "Could not update version in [$SectionName] for $Path"
    }

    Write-Utf8NoBom -Path $Path -Content ($lines -join [Environment]::NewLine)
}

function Update-LineByRegexOrFail {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Pattern,
        [Parameter(Mandatory = $true)][string]$Replacement
    )

    $raw = Get-Content -Raw $Path
    $matched = [regex]::IsMatch($raw, $Pattern, [System.Text.RegularExpressions.RegexOptions]::Multiline)
    if (-not $matched) {
        throw "Pattern not found in $Path"
    }

    $newRaw = [regex]::Replace($raw, $Pattern, $Replacement, [System.Text.RegularExpressions.RegexOptions]::Multiline)
    if ($newRaw -ne $raw) {
        Write-Utf8NoBom -Path $Path -Content $newRaw
    }
}

function Update-DocVersionMarker {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ShortVersion
    )

    if (-not (Test-Path $Path)) {
        return
    }

    $raw = Get-Content -Raw $Path
    $bt = [char]96
    $replacement = "- 当前版本（短版本）：$bt$ShortVersion$bt"
    $newRaw = [regex]::Replace($raw, '(?m)^- 当前版本（短版本）：`[^`]+`$', $replacement)
    $newRaw = [regex]::Replace($newRaw, '(?m)^Current version: `[^`]+`$', "Current version: $bt$ShortVersion$bt")
    if ($newRaw -ne $raw) {
        Write-Utf8NoBom -Path $Path -Content $newRaw
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$versionFile = Join-Path $repoRoot 'VERSION'
if (-not (Test-Path $versionFile)) {
    throw "VERSION file is missing: $versionFile"
}

$currentShort = (Get-Content -Raw $versionFile).Trim()
if ([string]::IsNullOrWhiteSpace($currentShort)) {
    throw 'VERSION file is empty.'
}

$targetShort = if ($Version) { $Version.Trim() } else { Get-NextShortVersion -Current $currentShort }
$parsed = Parse-ShortVersion -InputVersion $targetShort
$targetShort = Get-ShortVersionString -ParsedVersion $parsed
$semver = Get-SemverString -ParsedVersion $parsed
$androidCode = Get-AndroidVersionCode -ParsedVersion $parsed

Write-Host "Bumping version: $currentShort -> $targetShort" -ForegroundColor Cyan

Write-Utf8NoBom -Path $versionFile -Content ($targetShort + [Environment]::NewLine)

Update-TomlVersionInSection -Path (Join-Path $repoRoot 'Cargo.toml') -SectionName 'workspace.package' -NewSemver $semver
Update-TomlVersionInSection -Path (Join-Path $repoRoot 'apps/desktop/src-tauri/Cargo.toml') -SectionName 'package' -NewSemver $semver

$tauriPath = Join-Path $repoRoot 'apps/desktop/src-tauri/tauri.conf.json'
$tauriJson = Get-Content -Raw $tauriPath | ConvertFrom-Json
$tauriJson.version = $semver
Write-Utf8NoBom -Path $tauriPath -Content ($tauriJson | ConvertTo-Json -Depth 20)

Update-LineByRegexOrFail -Path (Join-Path $repoRoot 'apps/android_flutter/pubspec.yaml') -Pattern '^version:\s*\d+\.\d+(?:\.\d+)?\+\d+\r?$' -Replacement ("version: $semver+$androidCode")

$localPropsPath = Join-Path $repoRoot 'apps/android_flutter/android/local.properties'
$localProps = Get-Content -Raw $localPropsPath
if ($localProps -match '(?m)^flutter\.versionName=') {
    $localProps = [regex]::Replace($localProps, '(?m)^flutter\.versionName=.*$', "flutter.versionName=$targetShort")
}
else {
    $localProps = $localProps.TrimEnd() + [Environment]::NewLine + "flutter.versionName=$targetShort" + [Environment]::NewLine
}

if ($localProps -match '(?m)^flutter\.versionCode=') {
    $localProps = [regex]::Replace($localProps, '(?m)^flutter\.versionCode=.*$', "flutter.versionCode=$androidCode")
}
else {
    $localProps = $localProps.TrimEnd() + [Environment]::NewLine + "flutter.versionCode=$androidCode" + [Environment]::NewLine
}
Write-Utf8NoBom -Path $localPropsPath -Content $localProps

Update-DocVersionMarker -Path (Join-Path $repoRoot 'README.md') -ShortVersion $targetShort
Update-DocVersionMarker -Path (Join-Path $repoRoot 'docs/todo.md') -ShortVersion $targetShort
Update-DocVersionMarker -Path (Join-Path $repoRoot 'docs/RELEASE_POLICY.md') -ShortVersion $targetShort

Write-Host "Version bump complete. short=$targetShort semver=$semver androidCode=$androidCode" -ForegroundColor Green
