param(
    [switch]$SkipValidate,
    [switch]$NoAndroid,
    [switch]$NoWindows,
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Test-ForceReleaseMode {
    $value = [Environment]::GetEnvironmentVariable('FORCE_RELEASE')
    if ([string]::IsNullOrWhiteSpace($value)) {
        return $false
    }

    switch ($value.Trim().ToLowerInvariant()) {
        '1' { return $true }
        'true' { return $true }
        'yes' { return $true }
        'on' { return $true }
        default { return $false }
    }
}

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][scriptblock]$Action
    )

    Write-Host "`n==> $Name" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed: $Name (exit code: $LASTEXITCODE)"
    }
}

function Copy-Artifact {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (-not (Test-Path $Source)) {
        throw "Missing artifact: $Source"
    }
    New-Item -ItemType Directory -Force (Split-Path $Destination -Parent) | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

function Get-ReleaseGate {
    param(
        [Parameter(Mandatory = $true)][string]$GatePath
    )

    if (-not (Test-Path $GatePath)) {
        throw "Release gate missing: $GatePath"
    }

    return Get-Content -Raw $GatePath | ConvertFrom-Json
}

function Assert-PackagingGate {
    param(
        [Parameter(Mandatory = $true)][string]$GatePath
    )

    $gate = Get-ReleaseGate -GatePath $GatePath
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
    foreach ($field in @(
        'validate_local_passed',
        'rewrite_validate_passed',
        'device_acceptance_passed',
        'acceptance_json_present',
        'rollback_verified'
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

    $nonReleaseFailureCodes = @($gate.blocking_failure_codes | Where-Object { $_ -ne 'RELEASE_GATE_BLOCKED' })
    if ($nonReleaseFailureCodes.Count -gt 0) {
        $blockingReasons.Add("blocking_failure_codes=$($nonReleaseFailureCodes -join ',')")
    }

    if ($blockingReasons.Count -gt 0) {
        throw "Packaging gate blocked: $($blockingReasons -join '; ')"
    }
}

function Update-ReleaseGateForArtifacts {
    param(
        [Parameter(Mandatory = $true)][string]$GatePath,
        [Parameter(Mandatory = $true)][string]$AndroidDist,
        [Parameter(Mandatory = $true)][string]$WindowsDist
    )

    $gate = Get-ReleaseGate -GatePath $GatePath

    $androidArtifacts = @(Get-ChildItem -LiteralPath $AndroidDist -Filter '*.apk' -File -ErrorAction SilentlyContinue)
    $windowsArtifacts = @(Get-ChildItem -LiteralPath $WindowsDist -Filter '*.exe' -File -ErrorAction SilentlyContinue)

    $gate.android_release_apk_present = $androidArtifacts.Count -gt 0
    $gate.windows_exe_present = $windowsArtifacts.Count -gt 0

    $blockingCodes = @($gate.blocking_failure_codes | Where-Object { $_ -ne 'RELEASE_GATE_BLOCKED' })
    if (-not $gate.android_release_apk_present -or -not $gate.windows_exe_present) {
        $blockingCodes += 'RELEASE_GATE_BLOCKED'
    }

    $gate.blocking_failure_codes = @($blockingCodes | Select-Object -Unique)
    if (
        [bool]$gate.validate_local_passed -and
        [bool]$gate.rewrite_validate_passed -and
        [bool]$gate.device_acceptance_passed -and
        [bool]$gate.acceptance_json_present -and
        [bool]$gate.rollback_verified -and
        [bool]$gate.android_release_apk_present -and
        [bool]$gate.windows_exe_present -and
        ([int]$gate.known_blockers -eq 0) -and
        ([int]$gate.critical_bugs -eq 0) -and
        ($gate.blocking_failure_codes.Count -eq 0)
    ) {
        $gate.release_decision = 'allow_release'
    } else {
        $gate.release_decision = 'continue_fixing'
    }

    $gateJson = $gate | ConvertTo-Json -Depth 8
    Set-Content -LiteralPath $GatePath -Value $gateJson -Encoding utf8
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$version = (Get-Content -Raw (Join-Path $repoRoot 'VERSION')).Trim()
if ($version -notmatch '^\d+\.\d+(?:\.\d+)?$') {
    throw "Unexpected VERSION value: $version"
}

$gatePath = Join-Path $repoRoot 'artifacts/release/acceptance_gate.json'
$forceRelease = Test-ForceReleaseMode
if ($forceRelease) {
    Write-Warning 'FORCE_RELEASE=true detected; packaging gate enforcement is bypassed.'
} else {
    Assert-PackagingGate -GatePath $gatePath
}

$distRoot = Join-Path $repoRoot 'dist/release'
$androidDist = Join-Path $distRoot 'android'
$windowsDist = Join-Path $distRoot 'windows'

Push-Location $repoRoot
try {
    if ($Clean -and (Test-Path $distRoot)) {
        Remove-Item -LiteralPath $distRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Force $androidDist, $windowsDist | Out-Null

    $localPropsPath = Join-Path $repoRoot 'apps/android_flutter/android/local.properties'
    if (-not (Test-Path $localPropsPath)) {
        Invoke-Step -Name 'Prepare android/local.properties' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/write_local_properties.ps1')
        }
    }

    if (-not $SkipValidate) {
        Invoke-Step -Name 'Run local validation' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/validate_local.ps1')
        }
    }

    if (-not $NoAndroid) {
        Invoke-Step -Name 'Build Android release APKs (split per ABI)' -Action {
            $sourceProject = Join-Path $repoRoot 'apps/android_flutter'
            $stagedProject = Join-Path $env:TEMP "lan_audio_android_release_$PID"
            if (Test-Path $stagedProject) {
                Remove-Item -LiteralPath $stagedProject -Recurse -Force
            }
            New-Item -ItemType Directory -Force $stagedProject | Out-Null

            # Flutter release AOT can fail when the project path contains non-ASCII
            # characters. Stage the Android Flutter app under an ASCII temp path
            # for local packaging; GitHub Actions already uses an ASCII workspace.
            robocopy $sourceProject $stagedProject /MIR /XD .dart_tool build .gradle /XF *.hprof | Out-Host
            if ($LASTEXITCODE -gt 7) {
                throw "robocopy failed with exit code $LASTEXITCODE"
            }
            $LASTEXITCODE = 0

            Push-Location $stagedProject
            try {
                flutter build apk --release --split-per-abi
                if ($LASTEXITCODE -ne 0) {
                    throw "flutter release build failed with exit code $LASTEXITCODE"
                }
            }
            finally {
                Pop-Location
            }

            if (Test-Path (Join-Path $sourceProject 'build/app/outputs/flutter-apk')) {
                Remove-Item -LiteralPath (Join-Path $sourceProject 'build/app/outputs/flutter-apk') -Recurse -Force
            }
            New-Item -ItemType Directory -Force (Join-Path $sourceProject 'build/app/outputs/flutter-apk') | Out-Null
            $stagedApkRoot = Join-Path $stagedProject 'build/app/outputs/flutter-apk'
            $sourceApkRoot = Join-Path $sourceProject 'build/app/outputs/flutter-apk'
            Get-ChildItem -LiteralPath $stagedApkRoot -File | ForEach-Object {
                Copy-Item -LiteralPath $_.FullName -Destination $sourceApkRoot -Force
            }
        }

        $apkRoot = Join-Path $repoRoot 'apps/android_flutter/build/app/outputs/flutter-apk'
        Get-ChildItem -LiteralPath $apkRoot -Filter '*-release.apk' | ForEach-Object {
            $target = Join-Path $androidDist ("lan-audio-android-$version-$($_.Name)")
            Copy-Artifact -Source $_.FullName -Destination $target
        }
    }

    if (-not $NoWindows) {
        Invoke-Step -Name 'Build Windows desktop release EXE only' -Action {
            cargo build --release -p lan_audio_desktop
        }

        $exeSource = Join-Path $repoRoot 'target/release/lan_audio_desktop.exe'
        $exeTarget = Join-Path $windowsDist "lan-audio-desktop-$version.exe"
        Copy-Artifact -Source $exeSource -Destination $exeTarget
    }

    Update-ReleaseGateForArtifacts -GatePath $gatePath -AndroidDist $androidDist -WindowsDist $windowsDist
    if ($forceRelease) {
        $gate = Get-ReleaseGate -GatePath $gatePath
        $gate.force_release_override = $true
        $gate.force_release_note = 'Released under FORCE_RELEASE=true; release gate checklist items may be marked human-override.'
        $gateJson = $gate | ConvertTo-Json -Depth 8
        Set-Content -LiteralPath $gatePath -Value $gateJson -Encoding utf8
    }

    $checksumsPath = Join-Path $distRoot 'SHA256SUMS.txt'
    Get-ChildItem -LiteralPath $distRoot -Recurse -File |
        Where-Object { $_.FullName -ne $checksumsPath } |
        Sort-Object FullName |
        ForEach-Object {
            $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName
            $relative = Resolve-Path -Relative $_.FullName
            "$($hash.Hash)  $relative"
        } | Set-Content -Encoding utf8 $checksumsPath

    Write-Host "`nRelease artifacts:" -ForegroundColor Green
    Get-ChildItem -LiteralPath $distRoot -Recurse -File |
        Sort-Object FullName |
        Select-Object FullName, Length |
        Format-Table -AutoSize
}
finally {
    Pop-Location
}
