param(
    [string]$Version,
    [switch]$SkipValidate,
    [switch]$NoPush,
    [switch]$AllowDirty
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

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $repoRoot

try {
    if (-not $AllowDirty) {
        $dirty = git status --porcelain
        if ($dirty) {
            throw "Working tree is not clean. Commit or stash changes first, or pass -AllowDirty."
        }
    }

    if (-not $SkipValidate) {
        Invoke-Step -Name 'Run local validation' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/validate_local.ps1')
        }
    }

    if ($Version) {
        Invoke-Step -Name "Bump version to $Version" -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/bump_version.ps1') -Version $Version
        }
    }
    else {
        Invoke-Step -Name 'Bump version (minor +1)' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/bump_version.ps1')
        }
    }

    $newVersion = (Get-Content -Raw (Join-Path $repoRoot 'VERSION')).Trim()
    if ($newVersion -notmatch '^\d+\.\d+$') {
        throw "Unexpected VERSION value after bump: $newVersion"
    }

    Invoke-Step -Name 'Git add' -Action {
        git add VERSION AGENTS.md docs/RELEASE_POLICY.md README.md docs/todo.md docs/protocol.md docs/protocol_v2_migration.md Cargo.toml apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/tauri.conf.json apps/android_flutter/pubspec.yaml apps/android_flutter/android/local.properties scripts/validate_local.ps1 scripts/bump_version.ps1 scripts/release.ps1 .github/workflows/ci.yml .github/workflows/release.yml
    }

    $staged = git diff --cached --name-only
    $tag = "v$newVersion"

    if ($staged) {
        Invoke-Step -Name "Git commit $tag" -Action {
            git commit -m "chore(release): $tag"
        }
    }
    else {
        Write-Host "No version changes to commit; tagging current HEAD for $tag." -ForegroundColor Yellow
    }

    Invoke-Step -Name "Git tag $tag" -Action {
        git tag $tag
    }

    if (-not $NoPush) {
        Invoke-Step -Name 'Git push branch' -Action {
            git push
        }
        Invoke-Step -Name "Git push tag $tag" -Action {
            git push origin $tag
        }
    }
    else {
        Write-Host "Skipping push because -NoPush was specified." -ForegroundColor Yellow
    }

    Write-Host "`nRelease flow completed for $tag" -ForegroundColor Green
}
finally {
    Pop-Location
}
