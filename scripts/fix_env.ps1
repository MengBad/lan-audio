param(
  [switch]$TryInstallMissing
)

$ErrorActionPreference = 'Stop'

function Find-Cmd($name) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  return $null
}

function Add-ToUserPathIfNeeded($dir) {
  if (-not (Test-Path $dir)) { return $false }
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not $userPath) { $userPath = '' }
  $parts = $userPath.Split(';') | Where-Object { $_ -ne '' }
  if ($parts -notcontains $dir) {
    $newPath = ($parts + $dir) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "Added to user PATH: $dir"
    return $true
  }
  return $false
}

# Auto-discover existing installations first.
$adbDir = "$env:LOCALAPPDATA\Android\Sdk\platform-tools"
if (Test-Path (Join-Path $adbDir 'adb.exe')) {
  Add-ToUserPathIfNeeded $adbDir | Out-Null
}

$cargoDir = "$env:USERPROFILE\.cargo\bin"
if (Test-Path (Join-Path $cargoDir 'cargo.exe')) {
  Add-ToUserPathIfNeeded $cargoDir | Out-Null
}

$flutterCandidates = @(
  "$env:USERPROFILE\flutter\bin",
  "$env:USERPROFILE\scoop\apps\flutter\current\bin",
  'C:\src\flutter\bin',
  'C:\flutter\bin',
  'D:\flutter\bin',
  'G:\flutter\bin'
)
foreach ($d in $flutterCandidates) {
  if (Test-Path (Join-Path $d 'flutter.bat')) {
    Add-ToUserPathIfNeeded $d | Out-Null
  }
}

# Refresh process PATH from machine+user for this session
$machinePath = [Environment]::GetEnvironmentVariable('Path','Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
$env:Path = "$machinePath;$userPath"

$missing = @()
if (-not (Find-Cmd 'cargo')) { $missing += 'cargo' }
if (-not (Find-Cmd 'rustup')) { $missing += 'rustup' }
if (-not (Find-Cmd 'flutter')) { $missing += 'flutter' }
if (-not (Find-Cmd 'adb')) { $missing += 'adb' }

if ($missing.Count -gt 0 -and $TryInstallMissing) {
  if (Get-Command winget -ErrorAction SilentlyContinue) {
    if ($missing -contains 'cargo' -or $missing -contains 'rustup') {
      Write-Host 'Installing rustup via winget...'
      winget install -e --id Rustlang.Rustup --accept-package-agreements --accept-source-agreements --scope user --silent
    }
    if ($missing -contains 'flutter') {
      Write-Warning 'No official winget Flutter package id detected in this environment.'
      Write-Host 'Manual minimal install: download stable Flutter zip and extract to %USERPROFILE%\\flutter'
      Write-Host 'Download page: https://docs.flutter.dev/get-started/install/windows/mobile'
    }
  } else {
    Write-Warning 'winget not available; cannot auto-install missing tools.'
  }
}

# Try to write local.properties after possible install/discovery.
powershell -ExecutionPolicy Bypass -File "$PSScriptRoot\write_local_properties.ps1" | Out-Host

# Final status
powershell -ExecutionPolicy Bypass -File "$PSScriptRoot\check_env.ps1"
