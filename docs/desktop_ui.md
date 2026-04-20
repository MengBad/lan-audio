# Windows Desktop UI (Tauri)

## Scope

Windows desktop is the primary user entry for starting and controlling the LAN audio sender. `desktop_headless` remains a debug and regression entry, but it is no longer the normal user path.

The desktop app should feel like an audio streaming console, not a developer panel.

## First Screen

The first screen keeps four pieces of information visible:

- Service status
- Current audio source
- Local connection address
- Connected device count

Layout:

- Left: main status and service control
- Right: connection information and recent devices
- Bottom: collapsed debug area

## User Actions

- Start service
- Stop streaming
- Select audio source: `windows_loopback` or `synthetic`
- Select audio mode: `low_latency`, `balanced`, `high_quality`
- Select codec: default `pcm16`, experimental `opus_experimental` only on V2 header
- Copy connection address
- Open diagnostics/logs when needed

Only one primary button should be visually dominant:

- Service stopped: Start service
- Service running: Stop streaming

Restart and debug actions must stay secondary.

## State Sources

- Service status: desktop lifecycle state (`not_started`, `starting`, `running`, `stopping`, `error`)
- Connected devices: `metrics.active_sessions`
- Recent clients: `metrics.recent_clients`
- Local address: runtime IPv4 detection
- Audio mode: Protocol v2 `current_audio_mode`, synchronized through `set_audio_mode/audio_mode_changed`
- Mode strategy: `AudioModeProfile` summary, including start/max buffer and batch size
- Data plane: `legacy_las1` or `v2_header`
- Codec: requested codec and effective codec; Opus only applies when V2 header and client capability are both present
- Gray state: V2 header / Opus / loopback V2 must be explicit and visible
- Recommended connection: Wi-Fi by default, USB tethering for lower latency testing

## V2 Product Display

The UI should explain V2 as product state rather than raw protocol fields:

- Current service state: whether the PC is streaming
- Current connection state: whether a phone is connected
- Protocol path: safe default path or explicit V2 gray path
- Current mode: `low_latency`, `balanced`, `high_quality`
- Current source: `synthetic`, `windows_loopback`
- Current codec: PCM16 by default; Opus is experimental and explicit
- Rollback hint: switch back to `legacy_las1 + pcm16` if V2 gray path is unstable
- Recommended connection: same Wi-Fi, 5GHz Wi-Fi, or USB tethering

These fields should be shown as compact text rows, not as many separate cards.

## Diagnostics

Diagnostics are collapsed by default and should not compete with the main controls.

When expanded, group metrics by user-friendly categories:

- Sending: packets, bytes, active sessions
- Capture: frames, errors, source state, peak/RMS
- Playback/control: mode, codec, protocol path, recent events
- Logs: timestamped scrollable log lines

Raw engineering labels may remain available, but the visible label should be readable. Example: `tx_packets` is displayed as `Sent packets`.

## Release Packaging

Current GitHub release strategy for Windows is exe-only:

- Local: `scripts/package_release.ps1`
- CI: `.github/workflows/build-windows-client.yml`
- Release: `.github/workflows/release.yml`

Windows release artifact:

- `lan-audio-desktop-<version>.exe`

MSI/NSIS packaging is intentionally not part of the current release path. If installer packaging returns later, it should be added as a separate, explicit release track.

## i18n

Current languages:

- Chinese
- English

Language defaults should follow the system locale (`zh*` -> Chinese, otherwise English), with a visible switch in the UI.

## TODO

- QR code connection entry
- Richer session detail
- More guided USB tethering help
- Firewall help text
- Structured diagnostics export
- Opus loopback gray validation before presenting it as recommended
