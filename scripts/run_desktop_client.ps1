param(
  [switch]$NoExit
)

$ErrorActionPreference = 'Stop'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) {
  $env:Path = "$cargoBin;$env:Path"
}

Push-Location (Join-Path $PSScriptRoot '..\apps\desktop\src-tauri')
try {
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found. Please install Rust toolchain first."
  }
  Write-Host '[desktop] running cargo tauri dev ...' -ForegroundColor Cyan
  cargo tauri dev
}
finally {
  Pop-Location
}
