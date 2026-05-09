[English](./README.md) | [简体中文](./README.zh-CN.md)

# LAN Audio
Stream Windows audio to an Android phone over your local network.

LAN Audio is a Windows-to-Android audio streaming project for turning a phone into a wireless speaker. The repository combines a Rust streaming backend, a Flutter Android client, and a Tauri desktop app, with Protocol v2 and an explicit rollback path tracked side by side.

## Overview

The goal of this project is straightforward: play audio on a Windows PC, send it over LAN or USB-assisted network transport, and play it back on an Android device. The current repository is still geared more toward development, testing, and controlled release work than toward consumer-ready installation.

## Current Status

- Current version: `1.5`.
- The repository contains a Rust LAN server, an Android Flutter client, and a Windows Tauri desktop app.
- The documented recommended path is `windows_loopback + v2_header + opus`.
- The maintained rollback path is `legacy_las1 + pcm16`.
- The `v1.5` release is an operator-approved FORCE_RELEASE build that keeps the protocol path stable while shipping the Audio Console Dark UI refresh, latency probe sign-off, Android/Windows update checks, diagnostics export, buffering follow-ups, and release toolchain upgrades.
- Android and Windows UI surfaces now use the Audio Console Dark design direction with DM Sans and IBM Plex Mono.
- Android MediaSession integration is available with playback state, metadata, MediaStyle controls, play/pause, and stop.
- Android and Windows update detection are available as silent checks with manual entry points that link to GitHub Releases.
- Desktop diagnostics snapshot export writes JSON artifacts under `dist/diagnostics/`.
- Balanced-mode playback buffering has been tuned for better short-run stability.
- Latency revalidation is now systematized through `scripts/export_latency_probe.ps1`, which exports per-mode structured artifacts under `artifacts/latency/`.
- Local validation, packaging, and release scripts are part of the repository.
- Current follow-up work is focused on stability, latency tuning, mode strategy, Protocol v2 evolution, and desktop/UI productization.
- This project is currently a better fit for developers and testers on Windows + Android than for general end users.

## Quick Start

1. Check the local toolchain:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1
```

2. Start the Windows sender:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource windows_loopback
```

For a synthetic test source instead of system audio:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic
```

3. Start the Android client:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

4. In the Android app, discover or enter the desktop address, connect, and start playback.

For a full local validation pass:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

To export the structured latency probe artifact from desktop or Android snapshot JSON:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\export_latency_probe.ps1 -SnapshotPath .\dist\diagnostics\*.json
```

## How It Works

```text
Windows audio capture (windows_loopback / synthetic)
    -> Rust LAN server
    -> WebSocket control + UDP audio
       or localhost TCP audio over adb reverse in USB mode
    -> Android playback service and client UI
    -> jitter buffer / native playback output
    -> phone speaker
```

## Repository Layout

```text
apps/
  android_flutter/   Android client (Flutter UI + native playback bridge)
  desktop/           Windows desktop app (Tauri)

crates/
  lan_audio_domain/   Shared runtime and release contracts
  lan_audio_protocol/ Protocol messages and packet formats
  lan_audio_server/   Audio capture, transport, session, and metrics runtime

docs/               Architecture, protocol, setup, UI, roadmap, and release docs
scripts/            Environment checks, local run helpers, validation, packaging, and release scripts
artifacts/release/  Tracked release-gate and device-acceptance artifacts
```

## Development

This is a multi-component repository: Rust backend crates, a Flutter Android app, and a Tauri desktop frontend. Start with the docs below, then use `scripts/check_env.ps1` and `scripts/validate_local.ps1` to confirm the toolchain before changing runtime behavior, release logic, or UI code.

## Documentation

- [Architecture](docs/architecture.md)
- [Development Setup](docs/dev_setup.md)
- [Protocol](docs/protocol.md)
- [Protocol v2 Migration](docs/protocol_v2_migration.md)
- [Desktop UI](docs/desktop_ui.md)
- [Known Issues](docs/known_issues.md)
- [TODO / Status](docs/todo.md)
- [Roadmap](docs/roadmap.md)
- [Release Policy](docs/RELEASE_POLICY.md)
- [Android Visual Regression](docs/android_visual_regression.md)

## Roadmap

- Keep improving playback stability on the recommended Windows-to-Android path.
- Reduce and better control end-to-end latency.
- Keep `low_latency`, `balanced`, and `high_quality` behavior aligned across desktop and Android.
- Continue Protocol v2 evolution without removing the explicit rollback path.
- Productize the desktop and Android experience with clearer onboarding, diagnostics, and update flows.

## License

No root `LICENSE` file is currently present in this repository.
