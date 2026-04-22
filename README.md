# LAN Audio

Windows-to-Android LAN audio streaming. Use a Windows PC as the sender and an Android phone as a speaker on the same local network.

Current release train: `1.3`
Mainline target: `1.4`

> Status: Phase 0 / Phase 1 rewrite is in progress. Release is frozen behind `artifacts/release/acceptance_gate.json`, the current main-path target remains `windows_loopback + v2_header + opus`, and `legacy_las1 + pcm16` remains the maintained rollback path.

## Phase 0 / Phase 3 Freeze

- Release is blocked until `acceptance_gate.json` explicitly allows packaging and release.
- Domain-owned contracts now live in `crates/lan_audio_domain`.
- Desktop and Android runtime status consume the shared `service snapshot` contract, including `active_data_plane` and `rollback_available`.
- Current conclusion is `continue_fixing`; do not run `scripts/package_release.ps1` or `scripts/release.ps1` expecting a publishable build yet.

## What It Does

- Streams audio from Windows to Android over LAN.
- Supports USB mode via `adb reverse` (Android localhost WebSocket + Android localhost TCP data stream).
- Supports Windows system audio capture (`windows_loopback`) and a built-in test tone (`synthetic`).
- Provides an Android client with discovery, manual connection, recent servers, background playback, and bilingual UI.
- Provides a Windows desktop client plus `desktop_headless` for debugging.
- Keeps a safe rollback path while Protocol v2 and Opus evolve.

## Current Release Focus

Release `1.3` closes Phase 1/2 acceptance execution. Current mainline work for `1.4` is focused on:

- Android app size reduction with release shrink and split-per-ABI APKs.
- Android background playback and controlled auto reconnect.
- Protocol v2 control plane and data-plane gray path retained.
- Opus stabilization on the recommended V2 path with fixed 20ms packets and Android PLC fallback.
- GitHub Actions release workflow builds Android release APKs and Windows exe artifacts.

## Current Stability

Current conclusion: **the recommended V2 + Opus path remains available, but v1.3 acceptance metrics did not meet the strict latency/underrun targets yet**.

Validated so far:

- Android real device can connect to Windows and play audio.
- `synthetic + v2_header + opus` has passed real-device listening validation.
- `synthetic + v2_header + opus` has passed a 5-minute server-side stress test with fixed 20ms Opus packets (`p99 encode ~= 0.509 ms`, channel-full drop rate `0.000000`).
- Android auto reconnect behavior has passed real-device validation:
  - abnormal disconnect retries up to 3 times;
  - after retries are exhausted, playback stops instead of retrying forever;
  - reopening the app restores the last successful streaming server.

Still pending for release sign-off:

- Android real-device revalidation for `windows_loopback + v2_header + opus`
- balanced-mode end-to-end latency confirmation on device
- long-duration, multi-device stability
- USB low-latency validation

v1.3 acceptance execution (`2026-04-21`, device `5391d451 / Xiaomi 24129PN74C`):

- Scenario A (Wi-Fi, `cargo run -p lan_audio_server --bin desktop_headless`):
  - `balanced` 10min marker: `buffered_ms=80` at t0, `buffered_ms=220` at t10m; `jitter_underrun` rose `0 -> 3`; `silence_fill_count=0`; `audio_track_reported_latency_ms=289`.
  - `low_latency` 5min marker: `buffered_ms=120` at t0 and t5m; `jitter_underrun` rose `3 -> 21`; `silence_fill_count=0`; `audio_track_reported_latency_ms=289`.
  - Criteria check: `underrun=0` not met, `low_latency buffered_ms < 40ms` not met.
- Scenario B (USB, `--transport usb --adb-serial 5391d451`):
  - marker t0: `tcp_rtt_ms=1`, `buffered_ms=180`, `jitter_underrun=0`, `silence_fill_count=0`.
  - marker t5m: `tcp_rtt_ms=3`, `buffered_ms=140`, `jitter_underrun=7`, `silence_fill_count=0`.
  - periodic range: `tcp_rtt_ms min/max = 1/65`, `buffered_ms min/max = 0/220`.
  - Criteria check: `RTT < 5ms` and `buffered_ms < 20ms` not consistently met; no disconnect error line observed.
- Scenario C (multi-device): skipped by release instruction for this round.

Latest implementation status (`2026-04-21`):

- Phase 2 (USB transport): implemented in code (`transport_mode=usb`, adb device listing, reverse setup/teardown, Android USB entry, TCP length-prefixed data path).
- Phase 4 (multi-client broadcast): implemented in code (`MAX_CLIENTS=8`, client registry, per-client mode state, `client_list/client_joined/client_left`).
- Local CI-equivalent validation is green; Android real-device USB and multi-device end-to-end listening remain in unified acceptance scope.

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
  └─ libopus JNI                 stable Opus decode path with PLC fallback
```

## Protocol And Codec Status

Recommended default path:

- Audio source: `windows_loopback`
- Data plane: `v2_header`
- Codec: `opus`

Maintained rollback path:

- `legacy_las1 + pcm16`

Additional validation paths:

- `synthetic + v2_header + pcm16`
- `synthetic + v2_header + opus`

Protocol v2 currently provides:

- `hello / hello_ack`
- `hello_ack.transport_type` (`wifi` / `usb`) for transport-aware playback tuning
- capabilities negotiation
- `set_audio_mode / audio_mode_changed`
- mode profile synchronization
- `config_changed / discontinuity` handling points
- `LAS1 / LAV2` packet recognition on the client side
- server-side `DataPlane` abstraction for `legacy_las1`, `v2_header`, and `usb_direct`
- runtime snapshot visibility for configured `data_plane` vs active `active_data_plane`

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
- Path: `synthetic + v2_header + opus`
- Initial playback: `playing`, `buffered_ms≈70-80`, `rx_frames_per_sec≈99-101`, underrun/late/silence all 0
- Disconnect test: observed `attempt=1/3`, `2/3`, `3/3`, then `auto reconnect exhausted`
- Reopen test: app restored `10.0.0.185:39991/39992` and returned to `playing`

## Build Artifacts

GitHub Release `v1.3` is expected to contain:

- Windows: `lan-audio-desktop-v1.3.exe`
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

Recommended system audio capture:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

USB transport mode (adb reverse, Android localhost control/data):

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --transport usb --adb-serial <serial> --audio-source windows_loopback
```

Rollback-safe legacy path:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback --data-plane legacy_las1 --codec pcm16
```

Synthetic V2 + Opus validation:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --data-plane v2_header --codec opus
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

Release is currently frozen for the rewrite track. The local and CI release entries now fail fast unless `artifacts/release/acceptance_gate.json` says `allow_release`.

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

If the recommended path is unstable, use one of these rollback paths:

- `legacy_las1 + pcm16`
- `windows_loopback + legacy_las1 + pcm16`
- `synthetic + v2_header + pcm16`
