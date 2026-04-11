param(
  [string]$Task = ':app:assembleDebug'
)

$ErrorActionPreference = 'Stop'
$machinePath = [Environment]::GetEnvironmentVariable('Path','Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
$env:Path = "$machinePath;$userPath"
$repo = (Resolve-Path "$PSScriptRoot\..").Path
$androidDir = Join-Path $repo 'apps/android_flutter/android'
Set-Location $androidDir

if (-not (Test-Path '.\gradlew.bat')) {
  Write-Error 'gradlew.bat not found.'
}

Write-Host "Running Gradle task: $Task"
& .\gradlew.bat $Task --stacktrace
exit $LASTEXITCODE
