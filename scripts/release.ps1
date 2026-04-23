param(
    [string]$Version,
    [switch]$SkipValidate,
    [switch]$SkipPackage,
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
    powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/assert_release_gate.ps1')

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
        Invoke-Step -Name 'Bump version (next release number)' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/bump_version.ps1')
        }
    }

    $newVersion = (Get-Content -Raw (Join-Path $repoRoot 'VERSION')).Trim()
    if ($newVersion -notmatch '^\d+\.\d+(?:\.\d+)?$') {
        throw "Unexpected VERSION value after bump: $newVersion"
    }

    if (-not $SkipPackage) {
        Invoke-Step -Name 'Build local release artifacts' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/package_release.ps1') -SkipValidate -Clean
        }
    }

    Invoke-Step -Name 'Git add' -Action {
        git add VERSION AGENTS.md .cargo README.md docs/RELEASE_POLICY.md docs/todo.md docs/protocol.md docs/protocol_v2_migration.md docs/desktop_ui.md docs/roadmap.md Cargo.toml Cargo.lock crates apps scripts .github/workflows artifacts/release/acceptance_gate.json artifacts/release/device_acceptance.json
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
