param(
    [switch]$SkipValidate,
    [switch]$NoAndroid,
    [switch]$NoWindows,
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

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

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$version = (Get-Content -Raw (Join-Path $repoRoot 'VERSION')).Trim()
if ($version -notmatch '^\d+\.\d+$') {
    throw "Unexpected VERSION value: $version"
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
