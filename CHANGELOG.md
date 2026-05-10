# Changelog

All notable changes to LAN Audio are documented in this file.

The format follows Keep a Changelog, and this project uses `v<major.minor>` release tags.

## [1.7.1] - 2026-05-11

### Fixed

- Corrected the desktop and Android update checker GitHub API endpoint to `MengBad/lan-audio`.
- Moved Android update checks off the main thread to avoid `NetworkOnMainThreadException`.
- Kept desktop manual update checks on a non-blocking path so release lookup cannot freeze the Tauri UI thread.
- Aligned Android release signing in GitHub Actions with the fixed keystore-backed `local.properties` flow.
- Kept Android `versionCode` monotonic for the v1.7 patch release (`1.7.1` -> `3071`).

### Verification

- `scripts/validate_local.ps1` passed.
- `cargo test -p lan_audio_desktop -- update` passed.
- `flutter test` passed.
- Main branch Android APK workflow passed after the release signing fix.

## [1.7.0] - 2026-05-10

### Added

- Android 3-band EQ using Android `Equalizer`, with low/mid/high controls, presets, persistence, and stable snapshot fields.
- Loudness normalization on the Android PCM write path with RMS analysis, bounded software gain, ramp smoothing, and `low_latency` bypass.
- Multi-device server streaming support for up to 4 Android clients with independent sessions and disconnect isolation.
- Desktop active-device list summaries for multi-client sessions.
- mDNS LAN service registration on the Windows sender and Android `NsdManager` discovery for nearby devices.
- Smart reconnect with 1s, 2s, 4s, 8s, and 16s backoff, plus `reconnect_attempts` and `reconnect_delay_ms` snapshot fields.
- Connection history and favorites on Android, persisted with SharedPreferences and capped at 10 entries.
- Android power-saving guidance page and buffering-timeout notification for Xiaomi, Huawei, and generic Android battery policies.
- Contributor documentation, issue templates, pull request template, Codecov badge, protocol coverage workflow, and this changelog.

### Changed

- `PlaybackSessionController.kt` is now a small coordinator; playback state and jitter coordination were split into dedicated modules.
- Protocol tests now cover negotiation errors, v2 header round trips, legacy/v2 handshake distinction, and mode profile boundaries.
- CI can generate LCOV coverage for `lan_audio_protocol` and upload it to Codecov.

### Fixed

- USB direct stability was improved with timeout and buffer handling on the server and Android connection path.
- Reconnect exhaustion now enters an explicit error state instead of silently falling back to disconnected.

### Known Limitations

- Desktop per-device disconnect command is deferred to v1.8 until a stable per-client command contract exists.

## [1.6.0] - 2026-05-10

### Added

- Playback runtime modules for buffer policy, pacing, latency guard, and metrics collection.
- USB direct groundwork using `adb reverse` and a length-prefixed TCP data path.
- Shared playback diagnostics used by Android and desktop snapshots.

### Changed

- The recommended main path remains `windows_loopback + v2_header + opus`.
- The permanent rollback path remains `legacy_las1 + pcm16`.

### Fixed

- Improved Android playback diagnostics around buffering, underrun, silence fill, and sink write gaps.
- Reduced coupling between playback runtime internals and UI-facing snapshot state.

## [1.5.0] - 2026-04-23

### Added

- Android foreground MediaSession integration with MediaStyle notification controls.
- Manual update-check entry for Android and desktop update banner support.
- Desktop diagnostics export to JSON.
- Release packaging and validation refinements for Android APK and Windows executable outputs.

### Changed

- Protocol v2 and Opus were promoted as the recommended product path after synthetic and loopback validation work.
- Mode profiles for `low_latency`, `balanced`, and `high_quality` became the shared source of playback strategy.

### Fixed

- Duplicate-start and app-open restore regressions in Android foreground playback.
- Balanced-mode buffering behavior through one-shot refill and startup/steady-state silence accounting.
- Mode-switch diagnostics and startup silence classification.
