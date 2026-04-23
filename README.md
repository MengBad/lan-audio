# LAN Audio

LAN Audio turns a Windows PC into an audio sender and an Android phone into a network speaker.

It supports both Wi-Fi and USB transport, keeps a safe rollback path, and is built around a Rust server, a Flutter Android client, and a Tauri desktop app.

## Overview

- Latest release: `v1.3`
- Primary path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Transport modes: `wifi`, `usb`
- Audio modes: `low_latency`, `balanced`, `high_quality`

## Features

- Stream Windows system audio to Android in real time
- Use a built-in `synthetic` source for testing and diagnostics
- Run over normal LAN or USB via `adb reverse`
- Switch between `low_latency`, `balanced`, and `high_quality` playback strategies
- Keep Protocol v2 on the main path while preserving a stable legacy rollback route
- Inspect runtime state from desktop and Android through a shared snapshot contract
- Export desktop diagnostics snapshots as JSON for troubleshooting (`dist/diagnostics/`)
- Desktop app can silently check GitHub Releases and show an in-window update banner (manual open only)
- Android foreground playback notification now uses MediaStyle with MediaSession state/metadata and manual update check entry

## Current Status

`v1.3` has been released.

Current validated release facts:

- Release gate: `allow_release`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Rollback verification: `desktop_headless --force-rollback`
- Verified device: `5391d451` (`Xiaomi 24129PN74C`)
- Verified scenarios:
  - `USB + synthetic`
  - `WiFi + windows_loopback`

Current mainline work after `v1.3` is focused on Android runtime and desktop refactor follow-up, not on changing the default path again.

## Architecture

```text
Windows
  - Tauri desktop app
  - desktop_headless debug entry
  - Rust server
  - Protocol v1/v2 control + audio transport

Android
  - Flutter UI
  - Foreground playback service
  - Oboe / AudioTrack playback runtime
  - libopus JNI decode path
```

## Repository Layout

```text
apps/
  android_flutter/        Android client (Flutter + native playback bridge)
  desktop/                Windows desktop app (Tauri)

crates/
  lan_audio_domain/       Shared domain contracts and release gate schema
  lan_audio_protocol/     Protocol types and packet formats
  lan_audio_server/       Capture, transport, session, and runtime logic

docs/                     Protocol, UI, release, and roadmap docs
scripts/                  Local run, validate, package, and release scripts
artifacts/release/        Tracked release gate and device acceptance evidence
```

## Quick Start

### 1. Start the Windows sender

Synthetic test source:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic
```

Windows system audio:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

USB mode:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --transport usb --adb-serial <serial> --audio-source windows_loopback
```

Force rollback path:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback --force-rollback
```

### 2. Install the Android app

Download the APK that matches your device ABI from the GitHub release:

- `arm64-v8a`
- `armeabi-v7a`
- `x86_64`

### 3. Connect and play

1. Launch the Windows sender.
2. Open the Android app.
3. Use discovery, manual address entry, or USB mode.
4. Start playback.
5. If playback is unstable, use rollback mode on the desktop side.

## Data Plane And Codec

Recommended runtime path:

- Source: `windows_loopback`
- Data plane: `v2_header`
- Codec: `opus`

Maintained fallback path:

- Data plane: `legacy_las1`
- Codec: `pcm16`

Service snapshots expose both configured and active runtime path state:

- `data_plane`
- `active_data_plane`
- `rollback_available`
- `rollback_state`

## Local Development

Full local validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

This runs:

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

## Release

Version source:

- `VERSION`

Release entry:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version 1.3
```

The release pipeline is contract-gated by:

- `artifacts/release/acceptance_gate.json`
- `artifacts/release/device_acceptance.json`

GitHub Actions release behavior:

- `scripts/release.ps1` remains the source of version bumping and tag creation
- `.github/workflows/release.yml` publishes builds for an existing `v<major.minor>` tag
- manual `workflow_dispatch` is for rebuilding/publishing an existing tag with an optional fix summary, verified scope, and known limitations in the release notes
- the workflow validates that the checked-out `VERSION` matches the requested tag before publishing

GitHub Release artifacts include:

- Windows desktop `.exe`
- Android split-per-ABI release APKs
- `SHA256SUMS.txt`

## Documentation

- [Protocol](docs/protocol.md)
- [Protocol v2 Migration](docs/protocol_v2_migration.md)
- [Desktop UI](docs/desktop_ui.md)
- [Release Policy](docs/RELEASE_POLICY.md)
- [TODO / Status](docs/todo.md)
- [Roadmap](docs/roadmap.md)

## Rollback

If the recommended path is unstable, fall back to one of these:

- `legacy_las1 + pcm16`
- `windows_loopback + legacy_las1 + pcm16`
- `synthetic + v2_header + pcm16`

For explicit rollback verification, use:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --force-rollback
```
