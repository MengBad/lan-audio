param(
  [string]$ProjectRoot = (Resolve-Path "$PSScriptRoot\..").Path
)

# Refresh process PATH from persisted machine+user values.
$machinePath = [Environment]::GetEnvironmentVariable('Path','Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
$env:Path = "$machinePath;$userPath"

function Test-Cmd($name) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($null -eq $cmd) {
    [PSCustomObject]@{ Name = $name; Found = $false; Path = '' }
  } else {
    [PSCustomObject]@{ Name = $name; Found = $true; Path = $cmd.Source }
  }
}

function Find-Alt($name) {
  switch ($name) {
    'adb' {
      $c = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
      if (Test-Path $c) { return $c }
    }
    'cargo' {
      $c = "$env:USERPROFILE\.cargo\bin\cargo.exe"
      if (Test-Path $c) { return $c }
    }
    'rustup' {
      $c = "$env:USERPROFILE\.cargo\bin\rustup.exe"
      if (Test-Path $c) { return $c }
    }
    'flutter' {
      $cands = @(
        "$env:USERPROFILE\flutter\bin\flutter.bat",
        "$env:USERPROFILE\scoop\apps\flutter\current\bin\flutter.bat",
        'C:\src\flutter\bin\flutter.bat',
        'C:\flutter\bin\flutter.bat',
        'D:\flutter\bin\flutter.bat',
        'G:\flutter\bin\flutter.bat'
      )
      foreach ($c in $cands) { if (Test-Path $c) { return $c } }
    }
  }
  return ''
}

$tools = @('cargo', 'rustup', 'flutter', 'adb', 'java') | ForEach-Object {
  $t = Test-Cmd $_
  if (-not $t.Found) {
    $alt = Find-Alt $_
    if ($alt -ne '') {
      $t.Found = $true
      $t.Path = $alt + ' (detected-not-in-path)'
    }
  }
  $t
}
$tools | Format-Table -AutoSize

$gradlewPath = Join-Path $ProjectRoot 'apps/android_flutter/android/gradlew.bat'
$gradlewFound = Test-Path $gradlewPath
Write-Host ("gradlew.bat: " + ($(if ($gradlewFound) {"FOUND"} else {"MISSING"})) + " -> " + $gradlewPath)

$missing = @()
$missing += ($tools | Where-Object { -not $_.Found } | Select-Object -ExpandProperty Name)
if (-not $gradlewFound) { $missing += 'gradlew.bat' }

if ($missing.Count -gt 0) {
  Write-Warning ("Missing tools/artifacts: " + ($missing -join ', '))
  Write-Host "Minimum install hints:"
  Write-Host "- Rust: https://rustup.rs/"
  Write-Host "- Flutter: https://docs.flutter.dev/get-started/install/windows"
  Write-Host "- Android SDK/ADB: Android Studio SDK Manager"
  Write-Host "- Java 17+: required by Android build"
  Write-Host "- For Flutter Android build, set android/local.properties flutter.sdk or FLUTTER_ROOT env"
  exit 1
}

Write-Host "All required tools are present."
exit 0
