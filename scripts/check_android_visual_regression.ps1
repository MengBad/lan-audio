param(
  [string]$Serial = "",
  [string]$BaselineDir = "artifacts/ui_baseline/android/baseline_5391d451",
  [string]$CaptureRoot = "artifacts/ui_baseline/android",
  [double]$DiffThreshold = 0.03,
  [int]$CropTop = 80,
  [int]$CropBottom = 0
)

$ErrorActionPreference = "Stop"

Write-Host "[1/3] Run Flutter golden regression..."
Push-Location "apps/android_flutter"
try {
  flutter test test/ui_console_golden_test.dart
}
finally {
  Pop-Location
}

Write-Host "[2/3] Capture current real-device baseline..."
$captureArgs = @(
  "-ExecutionPolicy", "Bypass",
  "-File", ".\scripts\capture_android_ui_baseline.ps1",
  "-OutDir", $CaptureRoot
)
if (-not [string]::IsNullOrWhiteSpace($Serial)) {
  $captureArgs += @("-Serial", $Serial)
}
powershell @captureArgs

$latestDir = Get-ChildItem -Path $CaptureRoot -Directory |
  Where-Object { $_.Name -match "^\w+_\d{8}_\d{6}$" } |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $latestDir) {
  throw "No captured screenshot directory found under $CaptureRoot"
}

Write-Host "[3/3] Compare screenshot diff against baseline..."
python .\scripts\android_visual_diff.py `
  --baseline-dir $BaselineDir `
  --current-dir $latestDir.FullName `
  --threshold $DiffThreshold `
  --crop-top $CropTop `
  --crop-bottom $CropBottom

Write-Host "Visual regression check passed."
Write-Host "Current capture: $($latestDir.FullName)"
