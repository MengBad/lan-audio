param(
    [string]$GatePath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $GatePath) {
    $GatePath = Join-Path $repoRoot 'artifacts/release/acceptance_gate.json'
}

$validateDir = Join-Path $repoRoot 'artifacts/validate'
$logDir = Join-Path $validateDir 'logs'
New-Item -ItemType Directory -Force $validateDir, $logDir | Out-Null

$timestamp = Get-Date -Format 'yyyy-MM-ddTHH-mm-ss'
$logPath = Join-Path $logDir "rewrite_validate_$timestamp.json"

$results = [ordered]@{
    timestamp       = (Get-Date -Format 'o')
    steps           = @()
    all_passed      = $true
    failure_codes   = @()
}

function Add-Step {
    param([string]$Name, [bool]$Passed, [string]$Detail)
    $step = [ordered]@{ name = $Name; passed = $Passed; detail = $Detail }
    $script:results.steps += $step
    if (-not $Passed) {
        $script:results.all_passed = $false
    }
}

function Invoke-NativeCommand {
    param([string[]]$Command)
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & $Command[0] $Command[1..($Command.Length-1)] 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $prevEAP
    $stdout = ($output | Where-Object { $_ -isnot [System.Management.Automation.ErrorRecord] }) -join "`n"
    return @{ ExitCode = $exitCode; Output = $stdout }
}

# Step 1: Export domain contracts and verify parseable JSON
Write-Host "[1/6] Exporting domain contracts..." -ForegroundColor Cyan
try {
    $r = Invoke-NativeCommand -Command @('cargo', 'run', '--quiet', '-p', 'lan_audio_domain', '--bin', 'export_contracts')
    if ($r.ExitCode -ne 0) {
        throw "export_contracts exited with code $($r.ExitCode)"
    }
    $contracts = $r.Output | ConvertFrom-Json
    if (-not $contracts.release_gate_template) {
        throw "Missing release_gate_template in exported contracts"
    }
    $cv = $contracts.release_gate_template.contract_version
    if (-not $cv) {
        throw "Missing contract_version in release_gate_template"
    }
    Add-Step -Name 'export_contracts' -Passed $true -Detail "contract_version=$cv"
} catch {
    Add-Step -Name 'export_contracts' -Passed $false -Detail $_.Exception.Message
    $results.failure_codes += 'METRICS_SCHEMA_DRIFT'
}

# Step 2: Run domain crate tests
Write-Host "[2/6] Running lan_audio_domain tests..." -ForegroundColor Cyan
try {
    $r = Invoke-NativeCommand -Command @('cargo', 'test', '-p', 'lan_audio_domain')
    if ($r.ExitCode -ne 0) {
        throw "lan_audio_domain tests failed (exit code: $($r.ExitCode))"
    }
    Add-Step -Name 'domain_tests' -Passed $true -Detail 'all tests passed'
} catch {
    Add-Step -Name 'domain_tests' -Passed $false -Detail $_.Exception.Message
    $results.failure_codes += 'BUILD_TEST'
}

# Step 3: Run protocol crate tests (consumes domain contracts)
Write-Host "[3/6] Running lan_audio_protocol tests..." -ForegroundColor Cyan
try {
    $r = Invoke-NativeCommand -Command @('cargo', 'test', '-p', 'lan_audio_protocol')
    if ($r.ExitCode -ne 0) {
        throw "lan_audio_protocol tests failed (exit code: $($r.ExitCode))"
    }
    Add-Step -Name 'protocol_tests' -Passed $true -Detail 'all tests passed'
} catch {
    Add-Step -Name 'protocol_tests' -Passed $false -Detail $_.Exception.Message
    $results.failure_codes += 'BUILD_TEST'
}

# Step 4: Run server crate tests (includes rollback CLI contract test)
Write-Host "[4/6] Running lan_audio_server tests..." -ForegroundColor Cyan
try {
    $r = Invoke-NativeCommand -Command @('cargo', 'test', '-p', 'lan_audio_server')
    if ($r.ExitCode -ne 0) {
        throw "lan_audio_server tests failed (exit code: $($r.ExitCode))"
    }
    Add-Step -Name 'server_tests' -Passed $true -Detail 'all tests passed'
} catch {
    Add-Step -Name 'server_tests' -Passed $false -Detail $_.Exception.Message
    $results.failure_codes += 'BUILD_TEST'
}

# Step 5: Validate release gate schema structure
Write-Host "[5/6] Validating release gate schema..." -ForegroundColor Cyan
try {
    if (-not $contracts.release_gate_template) {
        throw "Missing release_gate_template in contracts"
    }
    $tpl = $contracts.release_gate_template
    $requiredFields = @(
        'contract_version', 'release_decision', 'current_main_path', 'rollback_path',
        'validate_local_passed', 'rewrite_validate_passed', 'device_acceptance_passed',
        'acceptance_json_present', 'rollback_verified', 'android_release_apk_present',
        'windows_exe_present', 'known_blockers', 'critical_bugs', 'blocking_failure_codes'
    )
    $missing = @()
    foreach ($f in $requiredFields) {
        if ($null -eq $tpl.$f) { $missing += $f }
    }
    foreach ($pathField in @('current_main_path', 'rollback_path')) {
        $pv = $tpl.$pathField
        if ($null -ne $pv) {
            foreach ($nested in @('transport', 'mode', 'data_plane', 'codec', 'effective_codec', 'rollback_state')) {
                if ($null -eq $pv.$nested) { $missing += "$pathField.$nested" }
            }
        }
    }
    if ($missing.Count -gt 0) {
        throw "Missing gate schema fields: $($missing -join ', ')"
    }
    Add-Step -Name 'gate_schema' -Passed $true -Detail "all $($requiredFields.Count) fields present"
} catch {
    Add-Step -Name 'gate_schema' -Passed $false -Detail $_.Exception.Message
    $results.failure_codes += 'METRICS_SCHEMA_DRIFT'
}

# Step 6: rollback verification
Write-Host "[6/6] Rollback verification..." -ForegroundColor Cyan
try {
    $evidencePath = Join-Path $validateDir 'rollback_evidence.json'
    if (Test-Path $evidencePath) {
        Remove-Item -LiteralPath $evidencePath -Force
    }

    $rollbackProc = Start-Process "cargo" -ArgumentList @(
        "run", "-p", "lan_audio_server", "--bin", "desktop_headless",
        "--", "--audio-source", "synthetic", "--force-rollback"
    ) -WorkingDirectory $repoRoot -Wait -PassThru -NoNewWindow

    if ($rollbackProc.ExitCode -ne 0) {
        throw "rollback verification failed (exit code: $($rollbackProc.ExitCode))"
    }
    if (-not (Test-Path $evidencePath)) {
        throw "rollback evidence missing: $evidencePath"
    }

    $evidence = Get-Content -Raw $evidencePath | ConvertFrom-Json
    if ($evidence.active_data_plane -ne "legacy_las1") {
        throw "rollback evidence: active_data_plane != legacy_las1"
    }
    if ($evidence.codec -ne "pcm16") {
        throw "rollback evidence: codec != pcm16"
    }
    if ($evidence.rollback_state -ne "active") {
        throw "rollback evidence: rollback_state != active"
    }
    if (-not [bool]$evidence.snapshot_observed) {
        throw "rollback evidence: snapshot_observed != true"
    }

    Add-Step -Name 'rollback_verification' -Passed $true -Detail "evidence=$evidencePath"
} catch {
    Add-Step -Name 'rollback_verification' -Passed $false -Detail $_.Exception.Message
    $results.failure_codes += 'RELEASE_GATE_BLOCKED'
}

# Write structured log
$logJson = $results | ConvertTo-Json -Depth 8
Set-Content -LiteralPath $logPath -Value $logJson -Encoding utf8
Write-Host "Validation log: $logPath" -ForegroundColor Gray

# Update gate if all passed
if ($results.all_passed) {
    if (Test-Path $GatePath) {
        $gate = Get-Content -Raw $GatePath | ConvertFrom-Json
        $gate.rewrite_validate_passed = $true
        $gate.rollback_verified = $true
        $gateJson = $gate | ConvertTo-Json -Depth 8
        Set-Content -LiteralPath $GatePath -Value $gateJson -Encoding utf8
        Write-Host "rewrite_validate_passed -> true in $GatePath" -ForegroundColor Green
        Write-Host "rollback_verified -> true in $GatePath" -ForegroundColor Green
    } else {
        Write-Host "Gate file not found at $GatePath; skipping update." -ForegroundColor Yellow
    }
} else {
    Write-Host "Rewrite validation FAILED. Gate not updated." -ForegroundColor Red
    Write-Host "Failure codes: $($results.failure_codes -join ', ')" -ForegroundColor Red
    exit 1
}
