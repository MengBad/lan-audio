param(
    [switch]$ValidateLocalPassed
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$artifactsRoot = Join-Path $repoRoot 'artifacts'
$baselineDir = Join-Path $artifactsRoot 'baseline'
$contractsDir = Join-Path $artifactsRoot 'contracts'
$releaseDir = Join-Path $artifactsRoot 'release'
$reportsDir = Join-Path $artifactsRoot 'reports'
$validateLogDir = Join-Path $artifactsRoot 'validate/logs'

New-Item -ItemType Directory -Force $baselineDir, $contractsDir, $releaseDir, $reportsDir, $validateLogDir | Out-Null

$contractsJson = cargo run --quiet -p lan_audio_domain --bin export_contracts
if ($LASTEXITCODE -ne 0) {
    throw "Failed to export phase 1 contracts (exit code: $LASTEXITCODE)"
}

$contractsPath = Join-Path $contractsDir 'phase1_contracts.json'
Set-Content -LiteralPath $contractsPath -Value $contractsJson -Encoding utf8
$contracts = Get-Content -Raw $contractsPath | ConvertFrom-Json

$gate = $contracts.release_gate_template
if ($ValidateLocalPassed) {
    $gate.validate_local_passed = $true
}

$gatePath = Join-Path $releaseDir 'acceptance_gate.json'
$gateJson = $gate | ConvertTo-Json -Depth 8
Set-Content -LiteralPath $gatePath -Value $gateJson -Encoding utf8

$baseline = [ordered]@{
    phase = 'phase0'
    release_frozen = $true
    current_main_path = 'windows_loopback + v2_header + opus'
    rollback_path = 'legacy_las1 + pcm16'
    fmt_failure = 'crates/lan_audio_server/src/transport.rs: encoder_sample_rate line wrap drift'
    doc_drift = @(
        'README.md still contains usable MVP / release-centric wording',
        'docs/protocol.md and docs/protocol_v2_migration.md still describe recommended path as if release sign-off were closer than the freeze allows',
        'docs/RELEASE_POLICY.md still describes a live release flow without the acceptance gate hard block'
    )
    release_gate = @{
        path = 'artifacts/release/acceptance_gate.json'
        decision = $gate.release_decision
        validate_local_passed = [bool]$gate.validate_local_passed
        blocking_failure_codes = @($gate.blocking_failure_codes)
    }
}
$baselinePath = Join-Path $baselineDir 'phase0_baseline.json'
Set-Content -LiteralPath $baselinePath -Value ($baseline | ConvertTo-Json -Depth 8) -Encoding utf8

$report = [ordered]@{
    phases_completed = @('phase0', 'phase1')
    release_decision = $gate.release_decision
    release_gate = 'blocked'
    validate_local_passed = [bool]$gate.validate_local_passed
    rollback_path_preserved = $true
    artifacts = @(
        'artifacts/baseline/phase0_baseline.json',
        'artifacts/contracts/phase1_contracts.json',
        'artifacts/release/acceptance_gate.json',
        'artifacts/reports/phase0_phase1_report.json',
        'artifacts/reports/phase0_phase1_report.md'
    )
    notes = @(
        'Phase 0 freezes package_release/release/local CI entry points behind acceptance_gate.json.',
        'Phase 1 moves mode contracts, connection state machine, failure taxonomy, service snapshot, and release gate schema into crates/lan_audio_domain.',
        'Release stays blocked until validate_local, rewrite_validate, device acceptance, rollback verification, and packaging evidence all turn green.'
    )
}
$reportJsonPath = Join-Path $reportsDir 'phase0_phase1_report.json'
Set-Content -LiteralPath $reportJsonPath -Value ($report | ConvertTo-Json -Depth 8) -Encoding utf8

$reportMdPath = Join-Path $reportsDir 'phase0_phase1_report.md'
$reportMd = @"
# Phase 0 / Phase 1 Report

- Release decision: continue_fixing
- Gate status: blocked by `artifacts/release/acceptance_gate.json`
- Local validation passed: $([bool]$gate.validate_local_passed)
- Current main path target: `windows_loopback + v2_header + opus`
- Maintained rollback path: `legacy_las1 + pcm16`
- Phase 0 evidence: release entry points are frozen and the known fmt drift is tracked.
- Phase 1 evidence: shared domain contracts now live in `crates/lan_audio_domain`.
"@
Set-Content -LiteralPath $reportMdPath -Value $reportMd -Encoding utf8
