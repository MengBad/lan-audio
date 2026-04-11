param(
  [switch]$BuildApk,
  [string]$DeviceId = ''
)

$ErrorActionPreference = 'Stop'
$machinePath = [Environment]::GetEnvironmentVariable('Path','Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
$env:Path = "$machinePath;$userPath"
$repo = (Resolve-Path "$PSScriptRoot\..").Path
$appDir = Join-Path $repo 'apps/android_flutter'
Set-Location $appDir

if (-not (Get-Command flutter -ErrorAction SilentlyContinue)) {
  Write-Error 'flutter not found in PATH. Install Flutter SDK first.'
}

& flutter pub get
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($BuildApk) {
  & flutter build apk --debug
  exit $LASTEXITCODE
}

if ($DeviceId -ne '') {
  & flutter run -d $DeviceId
} else {
  & flutter run
}
exit $LASTEXITCODE
