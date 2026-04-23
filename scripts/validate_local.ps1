param(
    [switch]$SkipCargoFmt,
    [switch]$SkipCargoTests,
    [switch]$SkipDesktopCheck,
    [switch]$SkipFlutter,
    [switch]$SkipAndroidBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    Write-Host "`n==> $Name" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed: $Name (exit code: $LASTEXITCODE)"
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $repoRoot

try {
    if (-not $SkipCargoFmt) {
        Invoke-Step -Name 'cargo fmt --all -- --check' -Action {
            cargo fmt --all -- --check
        }
    }

    Invoke-Step -Name 'cargo check' -Action {
        cargo check
    }

    if (-not $SkipCargoTests) {
        Invoke-Step -Name 'cargo test -p lan_audio_protocol -p lan_audio_server' -Action {
            cargo test -p lan_audio_protocol -p lan_audio_server
        }
    }

    if (-not $SkipDesktopCheck) {
        Invoke-Step -Name 'cargo check -p lan_audio_desktop' -Action {
            cargo check -p lan_audio_desktop
        }
    }

    if (-not $SkipFlutter) {
        Push-Location (Join-Path $repoRoot 'apps/android_flutter')
        try {
            Invoke-Step -Name 'flutter pub get' -Action {
                flutter pub get
            }
            Invoke-Step -Name 'flutter analyze' -Action {
                flutter analyze
            }
            Invoke-Step -Name 'flutter test' -Action {
                flutter test
            }
        }
        finally {
            Pop-Location
        }
    }

    if (-not $SkipAndroidBuild) {
        Push-Location (Join-Path $repoRoot 'apps/android_flutter/android')
        try {
            Invoke-Step -Name 'gradlew.bat assembleDebug' -Action {
                cmd /c gradlew.bat assembleDebug
            }
        }
        finally {
            Pop-Location
        }
    }

    Write-Host "`nLocal validation completed successfully." -ForegroundColor Green
}
finally {
    Pop-Location
}
