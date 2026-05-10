# How to Contribute

Thanks for helping improve LAN Audio. This project ships a Windows sender and an Android receiver for real-time LAN audio, so changes should keep playback stability, latency, and rollback safety visible.

## Development Requirements

- Rust 1.75+
- Flutter 3.x
- Android Studio, including Android SDK, NDK, and platform tools
- Windows 10+
- PowerShell 5+ or PowerShell 7+

## Local Setup

1. Clone the repository.
2. Run local validation from the repository root:

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
   ```

3. Run the Android client:

   ```powershell
   cd apps\android_flutter
   flutter run
   ```

4. Run the desktop app:

   ```powershell
   cd apps\desktop
   cargo tauri dev
   ```

5. Run the headless Windows sender for debugging:

   ```powershell
   cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
   ```

## Commit Guidelines

- Use one of these prefixes: `feat`, `fix`, `chore`, `refactor`, `docs`.
- Format: `feat: short description`.
- Keep each pull request focused on one change.
- Run `scripts/validate_local.ps1` before opening a pull request.
- Do not remove or hide the rollback path.

## Architecture

- Protocol layer: `crates/lan_audio_protocol/`
- Shared domain contracts: `crates/lan_audio_domain/`
- Server and sender runtime: `crates/lan_audio_server/`
- Android client: `apps/android_flutter/`
- Desktop client: `apps/desktop/`
- Local validation, packaging, and release scripts: `scripts/`
- Protocol, release, and product notes: `docs/`

## Runtime Paths

Primary path:

- `windows_loopback + v2_header + opus`

Permanent rollback path:

- `legacy_las1 + pcm16`

Additional maintained fallback/testing paths:

- `synthetic + v2_header`
- Protocol gray and rollback switch paths

Protocol v2 is the recommended product path, but compatibility and rollback are part of the contract. Changes that touch transport, codec, playback, or negotiation should include tests and documentation updates that keep both the main path and rollback path understandable.
