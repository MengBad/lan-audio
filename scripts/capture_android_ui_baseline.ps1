param(
  [string]$Serial = "",
  [string]$Package = "com.example.lan_audio_android_mvp",
  [string]$Activity = "com.example.lan_audio_android_mvp.MainActivity",
  [string[]]$Resolutions = @("1200x2670", "960x2136", "720x1600"),
  [string]$OutDir = "artifacts/ui_baseline/android"
)

$ErrorActionPreference = "Stop"

function Invoke-Adb {
  param([string[]]$Args)
  if ([string]::IsNullOrWhiteSpace($Serial)) {
    & adb @Args
  } else {
    & adb -s $Serial @Args
  }
}

function Ensure-Foreground {
  for ($i = 0; $i -lt 5; $i++) {
    if ([string]::IsNullOrWhiteSpace($Serial)) {
      cmd /c "adb shell dumpsys window windows | findstr /I $Package" | Out-Null
    } else {
      cmd /c "adb -s $Serial shell dumpsys window windows | findstr /I $Package" | Out-Null
    }
    if ($LASTEXITCODE -eq 0) {
      return $true
    }
    Start-Sleep -Seconds 1
  }
  return $false
}

function Get-ActiveSerial {
  $lines = @(& adb devices | Select-Object -Skip 1 | Where-Object {
      $_ -match '^\S+\s+device$'
    })
  if ($lines.Count -eq 0) {
    throw "No adb device online. Connect a real Android device and re-run."
  }
  if ($lines.Count -gt 1 -and [string]::IsNullOrWhiteSpace($Serial)) {
    throw "Multiple devices online. Please pass -Serial explicitly."
  }
  return ([string]$lines[0]).Split()[0].Trim()
}

if ([string]::IsNullOrWhiteSpace($Serial)) {
  $Serial = Get-ActiveSerial
}

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$targetDir = Join-Path $OutDir "$Serial`_$timestamp"
New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

$originSize = (Invoke-Adb @("shell", "wm", "size")) | Out-String
$originDensity = (Invoke-Adb @("shell", "wm", "density")) | Out-String
$physicalWidth = 0
$physicalDensity = 0
if ($originSize -match "Physical size:\s*(\d+)x(\d+)") {
  $physicalWidth = [int]$Matches[1]
}
if ($originDensity -match "Physical density:\s*(\d+)") {
  $physicalDensity = [int]$Matches[1]
}

try {
  Invoke-Adb @("shell", "input", "keyevent", "KEYCODE_WAKEUP") | Out-Null
  Invoke-Adb @("shell", "wm", "dismiss-keyguard") | Out-Null
  Invoke-Adb @("shell", "input", "keyevent", "82") | Out-Null

  foreach ($res in $Resolutions) {
    Write-Host "Capturing resolution $res ..."
    Invoke-Adb @("shell", "wm", "size", $res) | Out-Null
    if ($physicalWidth -gt 0 -and $physicalDensity -gt 0 -and $res -match "^(\d+)x(\d+)$") {
      $targetWidth = [int]$Matches[1]
      $targetDensity = [int][Math]::Round($physicalDensity * $targetWidth / $physicalWidth)
      Invoke-Adb @("shell", "wm", "density", "$targetDensity") | Out-Null
    }
    Invoke-Adb @("shell", "am", "force-stop", $Package) | Out-Null
    Start-Sleep -Seconds 1
    Invoke-Adb @("shell", "am", "start", "-n", "$Package/$Activity") | Out-Null
    Start-Sleep -Seconds 3
    $isForeground = Ensure-Foreground
    if (-not $isForeground) {
      Write-Warning "App package is not foreground for $res. Screenshot may be invalid (lockscreen/overlay)."
    }

    $safeRes = $res.Replace("x", "_")
    $outFile = Join-Path $targetDir "main_idle_$safeRes.png"
    if ([string]::IsNullOrWhiteSpace($Serial)) {
      cmd /c "adb exec-out screencap -p > `"$outFile`""
    } else {
      cmd /c "adb -s $Serial exec-out screencap -p > `"$outFile`""
    }
  }
}
finally {
  Invoke-Adb @("shell", "wm", "size", "reset") | Out-Null
  Invoke-Adb @("shell", "wm", "density", "reset") | Out-Null
}

Write-Host "Saved baseline screenshots to $targetDir"
