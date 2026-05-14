<p align="center">
  <h1 align="center">🔊 LAN Audio</h1>
  <p align="center">
    Stream your Windows PC audio to Android phones over Wi-Fi or USB.<br/>
    Turn any Android device into a wireless speaker.
  </p>
</p>

<p align="center">
  <a href="https://github.com/MengBad/lan-audio/releases"><img alt="Release" src="https://img.shields.io/github/v/release/MengBad/lan-audio?color=6366f1" /></a>
  <a href="https://github.com/MengBad/lan-audio/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/MengBad/lan-audio?color=22c55e" /></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%7C%20Android-8b5cf6" />
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> &nbsp;|&nbsp;
  <a href="https://github.com/MengBad/lan-audio/releases">Download</a> &nbsp;|&nbsp;
  <a href="CHANGELOG.md">Changelog</a>
</p>

---

## What is this?

LAN Audio captures system audio from a Windows PC and streams it in real-time to Android devices over your local network. Your phone becomes a low-latency wireless speaker.

**Use cases:**
- Your PC doesn't have good speakers, but your phone/tablet does
- You want to listen to PC audio from another room
- You need a quick wireless speaker without buying hardware

## Screenshots

<!-- 
  TODO: Add real screenshots here
  Place them in screenshots/ directory:
  - screenshots/desktop.png (Windows desktop app)
  - screenshots/android-playback.png (Android playing)
  - screenshots/android-discovery.png (Android discovering servers)
-->

> Screenshots coming soon. Download the app from [Releases](https://github.com/MengBad/lan-audio/releases) to try it out.

## Features

- **Low latency** — ~64ms (p95) in low_latency mode over Wi-Fi, even lower on USB
- **Opus codec** — 48kHz VBR encoding, low bandwidth usage
- **Auto-discovery** — mDNS finds nearby senders automatically, no manual IP needed
- **Three modes** — low_latency / balanced / high_quality
- **Auto-reconnect** — exponential backoff on network interruptions
- **Equalizer** — 3-band EQ with presets (Flat, Bass Boost, Vocal, Treble)
- **Reverse mic** — stream Android mic back to PC (port 7878)
- **Remote volume** — control phone volume from PC
- **Multi-device** — up to 4 phones receiving simultaneously
- **USB mode** — use a USB cable for more stable connection via adb

## Quick Start

### Download

**Android:** Grab the APK from [Releases](https://github.com/MengBad/lan-audio/releases). Most phones need the `arm64-v8a` build.

**Windows:** Download the `.exe` from Releases, or build from source:

```powershell
git clone https://github.com/MengBad/lan-audio.git
cd lan-audio
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

### Usage

1. Connect PC and phone to the same Wi-Fi network
2. Start LAN Audio on Windows
3. Open the app on Android — it auto-discovers the PC
4. Tap to connect and start streaming

**USB mode** (more stable, no Wi-Fi needed):
```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --transport usb --adb-serial <device-serial> --audio-source windows_loopback
```

## Tech Stack

| Component | Technology |
| :--- | :--- |
| Audio capture | WASAPI Loopback (Windows) |
| Desktop GUI | Tauri 2 + Rust |
| Transport | TCP, custom Protocol v2 |
| Codec | Opus 48kHz VBR |
| Android client | Flutter + Kotlin |
| Audio output | Oboe (NDK), AudioTrack fallback |
| Discovery | mDNS |

## Project Structure

```
apps/
  android_flutter/     Android client (Flutter + Kotlin native)
  desktop/             Windows desktop (Tauri)

crates/
  lan_audio_protocol/  Protocol definitions & parsing
  lan_audio_server/    Audio capture, encoding, transport
  lan_audio_domain/    Shared types

docs/                  Protocol specs, architecture
scripts/               Build & release scripts
```

## Development

### Requirements

- Rust 1.75+
- Flutter 3.x
- Android SDK + NDK (for Oboe)
- Windows 10+

### Build & Test

```powershell
# One-step validation
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1

# Or manually
cargo fmt --all -- --check
cargo check
cargo test -p lan_audio_protocol -p lan_audio_server
```

### Build Release

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

## Docs

- [Protocol Spec](docs/protocol.md) — wire format & negotiation
- [Protocol v2 Migration](docs/protocol_v2_migration.md)
- [Architecture](docs/architecture.md)
- [Desktop UI Design](docs/desktop_ui.md)
- [Dev Setup](docs/dev_setup.md)
- [Known Issues](docs/known_issues.md)

## License

[MIT](LICENSE)
