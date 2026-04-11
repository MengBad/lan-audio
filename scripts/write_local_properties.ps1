param(
  [string]$AndroidDir = (Join-Path (Resolve-Path "$PSScriptRoot\..").Path 'apps/android_flutter/android')
)

$ErrorActionPreference = 'Stop'

function Find-ExistingPath($candidates) {
  foreach ($p in $candidates) {
    if ($p -and (Test-Path $p)) { return (Resolve-Path $p).Path }
  }
  return $null
}

$flutterRoot = $env:FLUTTER_ROOT
if (-not $flutterRoot -or -not (Test-Path (Join-Path $flutterRoot 'bin\flutter.bat'))) {
  $flutterRoot = Find-ExistingPath @(
    "$env:USERPROFILE\flutter",
    "$env:USERPROFILE\scoop\apps\flutter\current",
    'C:\src\flutter',
    'C:\flutter',
    'D:\flutter',
    'G:\flutter'
  )
}

$androidSdk = $env:ANDROID_SDK_ROOT
if (-not $androidSdk -or -not (Test-Path (Join-Path $androidSdk 'platform-tools\adb.exe'))) {
  $androidSdk = $env:ANDROID_HOME
}
if (-not $androidSdk -or -not (Test-Path (Join-Path $androidSdk 'platform-tools\adb.exe'))) {
  $androidSdk = Find-ExistingPath @(
    "$env:LOCALAPPDATA\Android\Sdk",
    'C:\Android\Sdk'
  )
}

if (-not (Test-Path $AndroidDir)) {
  throw "Android dir not found: $AndroidDir"
}

$localPropsPath = Join-Path $AndroidDir 'local.properties'
$lines = @()
if ($androidSdk) {
  $sdkEscaped = $androidSdk -replace '\\','\\\\'
  $lines += "sdk.dir=$sdkEscaped"
}
if ($flutterRoot) {
  $flutterEscaped = $flutterRoot -replace '\\','\\\\'
  $lines += "flutter.sdk=$flutterEscaped"
}

if ($lines.Count -eq 0) {
  Write-Warning 'No Flutter/Android SDK paths detected; local.properties not updated.'
  exit 1
}

$enc = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($localPropsPath, ($lines -join "`r`n") + "`r`n", $enc)
Write-Host "Wrote $localPropsPath"
$lines | ForEach-Object { Write-Host "  $_" }

if ($flutterRoot) {
  [Environment]::SetEnvironmentVariable('FLUTTER_ROOT', $flutterRoot, 'User')
  Write-Host "Set user env FLUTTER_ROOT=$flutterRoot"
}
if ($androidSdk) {
  [Environment]::SetEnvironmentVariable('ANDROID_SDK_ROOT', $androidSdk, 'User')
  [Environment]::SetEnvironmentVariable('ANDROID_HOME', $androidSdk, 'User')
  Write-Host "Set user env ANDROID_SDK_ROOT/ANDROID_HOME=$androidSdk"
}

exit 0
