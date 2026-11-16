#!/usr/bin/env pwsh
# Release gate assertion - artifact-driven, not prose-driven.
# Source of truth: artifacts/release/acceptance_gate.json + device_acceptance.json
# FORCE_RELEASE / gate.human_override covers MANUAL acceptance fields only.
# Compile, artifact, blocking-code, main-path and rollback-contract checks always run.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$version = (Get-Content -Raw (Join-Path $repoRoot 'VERSION')).Trim()
if ($version -notmatch '^\d+\.\d+(?:\.\d+)?$') {
    throw "Invalid VERSION value: $version"
}

$gatePath = Join-Path $repoRoot 'artifacts/release/acceptance_gate.json'
$devicePath = Join-Path $repoRoot 'artifacts/release/device_acceptance.json'
foreach ($p in @($gatePath, $devicePath)) {
    if (-not (Test-Path -LiteralPath $p)) { throw "Missing acceptance artifact: $p" }
}

$gate = Get-Content -Raw -LiteralPath $gatePath | ConvertFrom-Json
$device = Get-Content -Raw -LiteralPath $devicePath | ConvertFrom-Json

$required = @(
    'contract_version', 'release_decision', 'current_main_path', 'rollback_path',
    'validate_local_passed', 'rewrite_validate_passed', 'device_acceptance_passed',
    'acceptance_json_present', 'rollback_verified', 'android_release_apk_present',
    'windows_exe_present', 'known_blockers', 'critical_bugs', 'blocking_failure_codes'
)
foreach ($field in $required) {
    if ($null -eq $gate.$field) { throw "Release gate missing field: $field" }
}

$hasHumanOverride = ($gate.PSObject.Properties.Name -contains 'human_override')
$forceRelease = ($env:FORCE_RELEASE -eq 'true') -or $hasHumanOverride
$manualFields = @('device_acceptance_passed', 'rewrite_validate_passed', 'acceptance_json_present')

$errors = New-Object System.Collections.Generic.List[string]
foreach ($field in @('validate_local_passed','rewrite_validate_passed','device_acceptance_passed','acceptance_json_present','rollback_verified','android_release_apk_present','windows_exe_present')) {
    if (-not [bool]$gate.$field) {
        if ($forceRelease -and ($manualFields -contains $field)) {
            Write-Warning "FORCE_RELEASE: manual field '$field' is [human-override], not a compile/artifact/rollback field."
        }
        else {
            $errors.Add("$field=false")
        }
    }
}
if ([int]$gate.known_blockers -gt 0) { $errors.Add("known_blockers=$($gate.known_blockers)") }
if ([int]$gate.critical_bugs -gt 0) { $errors.Add("critical_bugs=$($gate.critical_bugs)") }
$nonReleaseCodes = @($gate.blocking_failure_codes | Where-Object { $_ -ne 'RELEASE_GATE_BLOCKED' })
if ($nonReleaseCodes.Count -gt 0) { $errors.Add("blocking_failure_codes=$($nonReleaseCodes -join ',')") }
if ($gate.release_decision -ne 'allow_release') {
    if ($forceRelease -and $gate.release_decision -eq 'continue_fixing' -and $hasHumanOverride) {
        Write-Warning 'FORCE_RELEASE: release_decision=continue_fixing is [human-override] (confirmed in gate artifact).'
    }
    else {
        $errors.Add("release_decision=$($gate.release_decision)")
    }
}

# Contract checks: never overridable.
$main = $gate.current_main_path
$rollback = $gate.rollback_path
if ($main.data_plane -ne 'v2_header' -or $main.effective_codec -ne 'opus') {
    $errors.Add('current_main_path is not v2_header+opus')
}
if ($rollback.data_plane -ne 'legacy_las1' -or $rollback.effective_codec -ne 'pcm16') {
    $errors.Add('rollback_path is not legacy_las1+pcm16')
}
if ($device.acceptance_decision -ne 'passed') {
    if ($forceRelease) {
        Write-Warning "FORCE_RELEASE: device_acceptance=$($device.acceptance_decision) is [human-override]."
    }
    else {
        $errors.Add("device_acceptance=$($device.acceptance_decision)")
    }
}
if ($device.test_matrix.usb_synthetic.status -ne 'passed' -or $device.test_matrix.wifi_windows_loopback.status -ne 'passed') {
    if ($forceRelease) {
        Write-Warning 'FORCE_RELEASE: device test_matrix statuses are [human-override].'
    }
    else {
        $errors.Add('device_acceptance: both usb_synthetic and wifi_windows_loopback must be passed')
    }
}

if ($errors.Count -gt 0) {
    throw "Release gate blocked: $($errors -join '; ')"
}

if ($forceRelease) {
    Write-Host "PASS: release gate open for v$version (FORCE_RELEASE: manual items are [human-override])"
}
else {
    Write-Host "PASS: release gate open for v$version (main=v2_header+opus, rollback=legacy_las1+pcm16)"
}
exit 0
