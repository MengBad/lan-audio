param(
  [switch]$BuildApk,
  [string]$DeviceId = '',
  [string]$AsciiWorkspaceLink = 'C:\lan_audio_mvp'
)

$ErrorActionPreference = 'Stop'
$machinePath = [Environment]::GetEnvironmentVariable('Path','Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
$env:Path = "$machinePath;$userPath"
$repo = (Resolve-Path "$PSScriptRoot\..").Path

function Ensure-AsciiRepoPath([string]$originalRepo, [string]$linkPath) {
  if ($originalRepo -notmatch '[^\x00-\x7F]') {
    return $originalRepo
  }

  if (-not (Test-Path $linkPath)) {
    cmd /c "mklink /J `"$linkPath`" `"$originalRepo`"" | Out-Null
  } else {
    $item = Get-Item -LiteralPath $linkPath -Force
    if (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      throw "ASCII workspace link exists and is not a junction: $linkPath"
    }
  }

  return $linkPath
}

$repo = Ensure-AsciiRepoPath -originalRepo $repo -linkPath $AsciiWorkspaceLink
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
