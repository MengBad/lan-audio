param(
    [string]$Version,
    [switch]$SkipValidate,
    [switch]$SkipPackage,
    [switch]$NoPush,
    [switch]$AllowDirty,
    [switch]$ForceRelease
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
    if (-not $ForceRelease -and $env:FORCE_RELEASE -eq 'true') {
        $ForceRelease = $true
    }
    if ($ForceRelease) {
        $env:FORCE_RELEASE = 'true'
        Write-Warning 'FORCE_RELEASE=true: manual acceptance items are human-override. Validation, packaging, CI and rollback checks still run.'
    }

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
        Invoke-Step -Name 'Bump version (minor +1)' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/bump_version.ps1')
        }
    }

    $newVersion = (Get-Content -Raw (Join-Path $repoRoot 'VERSION')).Trim()
    if ($newVersion -notmatch '^\d+\.\d+(?:\.\d+)?$') {
        throw "Unexpected VERSION value after bump: $newVersion"
    }

    $changelogPath = Join-Path $repoRoot 'CHANGELOG.md'
    if (Test-Path $changelogPath) {
        $changelogText = Get-Content -Raw $changelogPath
        $shortVersion = $newVersion
        $semverForChangelog = $newVersion
        if ($newVersion -match '^\d+\.\d+$') {
            $semverForChangelog = "$newVersion.0"
        }
        $versionPatterns = @(
            "[$newVersion]",
            "[$shortVersion]",
            "[$($newVersion).0]",
            "[$semverForChangelog]"
        ) | Select-Object -Unique
        $hasChangelogEntry = $false
        foreach ($pattern in $versionPatterns) {
            if ($changelogText.Contains($pattern)) {
                $hasChangelogEntry = $true
                break
            }
        }
        if (-not $hasChangelogEntry) {
            Write-Warning "CHANGELOG.md does not contain an entry for version $newVersion. Release will continue."
        }
    }
    else {
        Write-Warning "CHANGELOG.md is missing. Release will continue."
    }

    if (-not $SkipPackage) {
        Invoke-Step -Name 'Build local release artifacts' -Action {
            powershell -ExecutionPolicy Bypass -File (Join-Path $repoRoot 'scripts/package_release.ps1') -SkipValidate -Clean
        }
    }

    if ($ForceRelease) {
        $gatePath = Join-Path $repoRoot 'artifacts/release/acceptance_gate.json'
        $gate = Get-Content -Raw -LiteralPath $gatePath | ConvertFrom-Json
        $override = [pscustomobject]@{
            forced_by          = 'FORCE_RELEASE=true'
            recorded_at        = (Get-Date -Format 'yyyy-MM-dd')
            covered_fields     = @('device_acceptance_passed', 'rewrite_validate_passed', 'acceptance_json_present', 'device_acceptance_decision', 'release_decision')
            compile_artifact_rollback_checks = 'not overridden'
        }
        $gate | Add-Member -NotePropertyName human_override -NotePropertyValue $override -Force
        [System.IO.File]::WriteAllText($gatePath, ($gate | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
        Write-Host "Recorded [human-override] in $gatePath" -ForegroundColor Yellow
    }

    Invoke-Step -Name 'Git add' -Action {
        git add VERSION AGENTS.md .gitignore .cargo README.md README.zh-CN.md CONTRIBUTING.md CHANGELOG.md docs/RELEASE_POLICY.md docs/todo.md docs/protocol.md docs/protocol_v2_migration.md docs/desktop_ui.md docs/roadmap.md docs/MAINTENANCE_PLAN.md Cargo.toml Cargo.lock crates apps scripts .github/workflows .github/ISSUE_TEMPLATE .github/PULL_REQUEST_TEMPLATE.md artifacts/release/acceptance_gate.json artifacts/release/device_acceptance.json
    }

    $staged = git diff --cached --name-only
    $tag = "v$newVersion"

    if ($ForceRelease) {
        Write-Host "Release note marker: [human-override] — manual acceptance items are human-confirmed for $tag." -ForegroundColor Yellow
    }

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
