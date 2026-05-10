# LAN Audio

[![codecov](https://codecov.io/gh/MengBad/lan-audio/branch/main/graph/badge.svg)](https://codecov.io/gh/MengBad/lan-audio)

LAN Audio turns a Windows PC into an audio sender and an Android phone into a network speaker.

It supports Wi-Fi and USB transport, keeps a permanent rollback path, and is built around a Rust server, a Flutter Android client, and a Tauri desktop app.

## Overview

- Current version: `1.7.1`
- Latest release: `v1.7.1`
- Primary path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Transport modes: `wifi`, `usb`
- Audio modes: `low_latency`, `balanced`, `high_quality`

## Features

- Stream Windows system audio to Android in real time.
- Use a built-in `synthetic` source for testing and diagnostics.
- Run over normal LAN or USB via `adb reverse`.
- Switch between `low_latency`, `balanced`, and `high_quality` playback strategies.
- Keep Protocol v2 on the main path while preserving `legacy_las1 + pcm16` as a stable rollback route.
- Inspect runtime state from desktop and Android through a shared snapshot contract.
- Export desktop diagnostics snapshots as JSON for troubleshooting.
- Android foreground playback notification uses MediaStyle with MediaSession state/metadata.
- Android background playback guide surfaces battery-saver steps for Xiaomi, Huawei, and generic Android devices.
- Android 3-band EQ with low/mid/high controls, presets, and persistent settings.
- Optional loudness normalization with live gain display.
- Multi-device streaming from one Windows sender to up to 4 Android clients.
- mDNS LAN discovery shows nearby senders without manual IP entry.
- Smart reconnect uses exponential backoff after short network interruptions.
- Connection history and favorites persist common devices for one-tap reconnect.
- Contributor docs, issue templates, PR template, changelog, and Codecov coverage reporting are in place.

## Current Status

`v1.7.1` is the current patch release for the v1.7 line.

Validated release facts:

- Release gate: `allow_release`
- FORCE_RELEASE: `false`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Rollback verification: `desktop_headless --force-rollback`
- Verified device: `5391d451` (`Xiaomi 24129PN74C`)
- Verified scenarios: `USB direct`, `WiFi + windows_loopback`, `2 Android clients`
- Latency probe: `low_latency p95=64ms`, `balanced p95=185ms`, `high_quality p95=505ms`
- Known issue: desktop per-device disconnect command is deferred to v1.8.
- v1.7.1 hotfix: update checker endpoints use `MengBad/lan-audio`, Android update checks run off the main thread, and release signing CI is aligned with the fixed keystore flow.

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

Start the Windows sender with the real system audio path:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

Synthetic test source:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic
```

USB mode:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --transport usb --adb-serial <serial> --audio-source windows_loopback
```

Force rollback path:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback --force-rollback
```

Install the Android APK that matches your device ABI from the GitHub release:

- `arm64-v8a`
- `armeabi-v7a`
- `x86_64`

Open the Android app, choose a nearby mDNS-discovered sender or enter an IP manually, and start playback.

## Data Plane And Codec

Recommended runtime path:

- Source: `windows_loopback`
- Data plane: `v2_header`
- Codec: `opus`

Maintained fallback path:

- Data plane: `legacy_las1`
- Codec: `pcm16`

Service snapshots expose configured and active runtime path state, including EQ, loudness, reconnect, and multi-device summary fields.

## Local Development

Full local validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

Build local release artifacts:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

Confirm the v1.7 latency probe:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\export_latency_probe.ps1
```

## Release

Version source:

- `VERSION`

Release entry:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version 1.7.1
```

GitHub Release artifacts include:

- `lan-audio-android-arm64-v8a-v1.7.1.apk`
- `lan-audio-android-armeabi-v7a-v1.7.1.apk`
- `lan-audio-android-x86_64-v1.7.1.apk`
- `lan-audio-desktop-v1.7.1.exe`
- `SHA256SUMS.txt`

## Documentation

- [Protocol](docs/protocol.md)
- [Protocol v2 Migration](docs/protocol_v2_migration.md)
- [Desktop UI](docs/desktop_ui.md)
- [Release Policy](docs/RELEASE_POLICY.md)
- [TODO / Status](docs/todo.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)

## Rollback

If the recommended path is unstable, fall back to one of these:

- `legacy_las1 + pcm16`
- `windows_loopback + legacy_las1 + pcm16`
- `synthetic + v2_header + pcm16`

For explicit rollback verification, use:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --force-rollback
```
