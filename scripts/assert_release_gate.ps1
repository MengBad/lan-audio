param(
    [string]$GatePath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $GatePath) {
    $GatePath = Join-Path $repoRoot 'artifacts/release/acceptance_gate.json'
}

if (-not (Test-Path $GatePath)) {
    throw "Release gate missing: $GatePath"
}

$gate = Get-Content -Raw $GatePath | ConvertFrom-Json

$requiredFields = @(
    'contract_version',
    'release_decision',
    'current_main_path',
    'rollback_path',
    'validate_local_passed',
    'rewrite_validate_passed',
    'device_acceptance_passed',
    'acceptance_json_present',
    'rollback_verified',
    'android_release_apk_present',
    'windows_exe_present',
    'known_blockers',
    'critical_bugs',
    'blocking_failure_codes'
)

foreach ($field in $requiredFields) {
    if ($null -eq $gate.$field) {
        throw "Release gate missing required field: $field"
    }
}

foreach ($pathField in @('current_main_path', 'rollback_path')) {
    $pathValue = $gate.$pathField
    if ($null -eq $pathValue) {
        throw "Release gate missing required object: $pathField"
    }
    foreach ($nested in @('transport', 'mode', 'data_plane', 'codec', 'effective_codec', 'rollback_state')) {
        if ($null -eq $pathValue.$nested) {
            throw "Release gate missing required field: $pathField.$nested"
        }
    }
}

$blockingReasons = New-Object System.Collections.Generic.List[string]
if ($gate.release_decision -ne 'allow_release') {
    $blockingReasons.Add("release_decision=$($gate.release_decision)")
}
foreach ($field in @(
    'validate_local_passed',
    'rewrite_validate_passed',
    'device_acceptance_passed',
    'acceptance_json_present',
    'rollback_verified',
    'android_release_apk_present',
    'windows_exe_present'
)) {
    if (-not [bool]$gate.$field) {
        $blockingReasons.Add("$field=false")
    }
}
if ([int]$gate.known_blockers -gt 0) {
    $blockingReasons.Add("known_blockers=$($gate.known_blockers)")
}
if ([int]$gate.critical_bugs -gt 0) {
    $blockingReasons.Add("critical_bugs=$($gate.critical_bugs)")
}
$failureCodes = @($gate.blocking_failure_codes)
if ($failureCodes.Count -gt 0) {
    $blockingReasons.Add("blocking_failure_codes=$($failureCodes -join ',')")
}

if ($blockingReasons.Count -gt 0) {
    throw "Release gate blocked: $($blockingReasons -join '; ')"
}

Write-Host "Release gate allows packaging/release." -ForegroundColor Green
