param(
  [ValidateSet('synthetic','windows_loopback')]
  [string]$AudioSource = 'windows_loopback',
  [switch]$NoAudioFallback,
  [switch]$CaptureDumpWav,
  [int]$CaptureDumpSeconds = 5,
  [string]$CaptureDumpDir = 'debug_captures'
)

$ErrorActionPreference = 'Stop'
$machinePath = [Environment]::GetEnvironmentVariable('Path','Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
$env:Path = "$machinePath;$userPath"
$repo = (Resolve-Path "$PSScriptRoot\..").Path
Set-Location $repo

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Error 'cargo not found in PATH. Install Rust toolchain first.'
}

$args = @('run','-p','lan_audio_server','--bin','desktop_headless','--','--audio-source',$AudioSource)
if ($NoAudioFallback) { $args += '--no-audio-fallback' }
if ($CaptureDumpWav) {
  $args += '--capture-dump-wav'
  $args += '--capture-dump-seconds'; $args += "$CaptureDumpSeconds"
  $args += '--capture-dump-dir'; $args += $CaptureDumpDir
}

Write-Host "Running: cargo $($args -join ' ')"
& cargo @args
