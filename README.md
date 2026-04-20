# LAN Audio

Windows-to-Android LAN audio streaming. Use a Windows PC as the sender and an Android phone as a speaker on the same local network.

Current version: `1.1`

> Status: usable MVP. The default path is still the safe PCM legacy path. Protocol v2, V2 header, and Opus are available as explicit gray/experimental paths.

## What It Does

- Streams audio from Windows to Android over LAN.
- Supports Windows system audio capture (`windows_loopback`) and a built-in test tone (`synthetic`).
- Provides an Android client with discovery, manual connection, recent servers, background playback, and bilingual UI.
- Provides a Windows desktop client plus `desktop_headless` for debugging.
- Keeps a safe rollback path while Protocol v2 and Opus evolve.

## Current Release Focus

Version `1.1` is focused on productizing the first usable release path:

- Android app size reduction with release shrink and split-per-ABI APKs.
- Android background playback and controlled auto reconnect.
- Protocol v2 control plane and data-plane gray path retained.
- Opus experimental path wired through standard libopus/JNI.
- GitHub Actions release workflow builds Android release APKs and Windows exe artifacts.

## Current Stability

Current conclusion: **audio is playable, but not yet declared long-term stable across devices**.

Validated so far:

- Android real device can connect to Windows and play audio.
- `synthetic + v2_header + opus_experimental` has passed real-device listening validation.
- Android auto reconnect behavior has passed real-device validation:
  - abnormal disconnect retries up to 3 times;
  - after retries are exhausted, playback stops instead of retrying forever;
  - reopening the app restores the last successful streaming server.

Still gray/experimental:

- `windows_loopback + v2_header`
- `opus_experimental` on real system audio
- long-duration, multi-device stability
- USB low-latency validation

## Architecture

```text
Windows sender
  ├─ Tauri desktop client        user-facing control app
  ├─ desktop_headless            debug/regression entry
  ├─ Rust server                 capture, protocol, UDP sender, metrics
  └─ Protocol v1/v2              WebSocket control + UDP audio

Android client
  ├─ Flutter UI                  discovery, controls, status, diagnostics
  ├─ Foreground playback service background playback and reconnect
  ├─ AudioTrack playback         PCM output
  └─ libopus JNI                 experimental Opus decode path
```

## Protocol And Codec Status

Default path:

- Data plane: `legacy_las1`
- Codec: `pcm16`
- Recommended for normal use and rollback

Gray paths:

- `synthetic + v2_header`
- `windows_loopback + v2_header` only with explicit gray flag
- `opus_experimental` only when V2 header is active and capabilities allow it

Protocol v2 currently provides:

- `hello / hello_ack`
- capabilities negotiation
- `set_audio_mode / audio_mode_changed`
- mode profile synchronization
- `config_changed / discontinuity` handling points
- `LAS1 / LAV2` packet recognition on the client side

## Audio Modes

| Mode | Use case | Strategy |
| --- | --- | --- |
| `low_latency` | video/game sync | smaller buffer, batch=1, more aggressive catch-up |
| `balanced` | default use | moderate buffer, stable playback |
| `high_quality` | music/long listening | larger buffer, smoother playback |

The modes are not just labels. They map to playback buffer, batch size, drop threshold, and backend preference across protocol/server/Android/desktop semantics.

## Android Auto Reconnect

The Android foreground playback service keeps the last successful server target.

Behavior:

- abnormal WebSocket disconnect enters `reconnecting`;
- automatic reconnect is limited to 3 attempts;
- after 3 failed attempts, playback stops and the last successful target remains recoverable;
- reopening the app attempts `app_open_restore` to reconnect that target;
- pressing Stop clears the restore target.

Real-device validation on `2026-04-21`:

- Device: `5391d451 / Xiaomi 24129PN74C`
- Path: `synthetic + v2_header + opus_experimental`
- Initial playback: `playing`, `buffered_ms≈70-80`, `rx_frames_per_sec≈99-101`, underrun/late/silence all 0
- Disconnect test: observed `attempt=1/3`, `2/3`, `3/3`, then `auto reconnect exhausted`
- Reopen test: app restored `10.0.0.185:39991/39992` and returned to `playing`

## Build Artifacts

GitHub Release `v1.1` is expected to contain:

- Windows: `lan-audio-desktop-v1.1.exe`
- Android release APKs split by ABI:
  - `arm64-v8a`
  - `armeabi-v7a`
  - `x86_64`
- `SHA256SUMS.txt`

Windows release is intentionally exe-only for now. MSI/NSIS installers are not part of this release path.

## Quick Start

### Windows Sender

Use the desktop client for normal operation.

For debug or regression testing:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic
```

System audio capture:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

V2 + Opus experimental test tone:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --data-plane v2_header --codec opus_experimental
```

Loopback + V2 gray path requires an explicit flag:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback --data-plane v2_header --allow-loopback-v2-header-gray
```

### Android Client

Install the APK matching your device ABI from the GitHub Release.

Recommended first test:

1. Start Windows sender with `synthetic`.
2. Open Android app.
3. Use discovery, LAN scan, or manual address.
4. Connect and start playback.
5. If discovery fails, confirm both devices are on the same network and not isolated by guest/AP isolation.

## Local Development

Run the full local validation suite:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

The validation script runs:

- `cargo fmt --all -- --check`
- `cargo check`
- `cargo test -p lan_audio_protocol -p lan_audio_server`
- `cargo check -p lan_audio_desktop`
- `flutter analyze`
- `flutter test`
- `android/gradlew.bat assembleDebug`

Build local release artifacts:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

Outputs:

- `dist/release/android/`
- `dist/release/windows/`
- `dist/release/SHA256SUMS.txt`

## Release Process

Version source: `VERSION`

Version rule:

- short version: `1.0`, `1.1`, `1.2`, ...
- Git tag: `v<major.minor>`
- Rust/Tauri semver: `<major.minor>.0`
- Android version name: `<major.minor>`
- Android version code: `2000 + major * 100 + minor` to stay installable over earlier test builds

Release workflow:

1. Update version with `scripts/bump_version.ps1 -Version 1.1`.
2. Commit and tag `v1.1`.
3. Push branch and tag.
4. GitHub Actions builds release artifacts.
5. GitHub Actions publishes the release.

Manual release helper:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version 1.1
```

## Documentation

- [Protocol v2](docs/protocol.md)
- [Protocol v2 Migration](docs/protocol_v2_migration.md)
- [Roadmap](docs/roadmap.md)
- [Desktop UI](docs/desktop_ui.md)
- [Release Policy](docs/RELEASE_POLICY.md)
- [TODO / Status](docs/todo.md)

## Rollback

If a V2 or Opus path is unstable, use one of these safe paths:

- `legacy_las1 + pcm16`
- `synthetic + legacy_las1`
- `synthetic + v2_header + pcm16`

Do not make V2 header or Opus the default until long-duration loopback validation passes.
