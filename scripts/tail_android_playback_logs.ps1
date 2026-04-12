param(
  [switch]$Clear
)

if ($Clear) {
  adb logcat -c | Out-Null
}

Write-Host "[lan-audio] streaming android playback logs (Ctrl+C to stop)..."
adb logcat -v time | findstr /I "ui_build lan_audio_service lan_audio_session lan_audio_stream"
