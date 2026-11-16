# Project Roadmap

## Current Position (v1.15)

- Current version (short version): `1.16`
- Latest release: `v1.16`
- Default runtime path: `windows_loopback + v2_header + opus`
- Maintained rollback path: `legacy_las1 + pcm16`
- Supported transports: `wifi`, `usb`
- Supported codecs: `opus` (default), `pcm16` (rollback)
- Shared contract source: `crates/lan_audio_domain`

## What's Shipped

- Stable Windows-to-Android audio streaming over Wi-Fi and USB
- Opus 48kHz VBR as default codec with sinc resampler
- Four playback modes: ultra_low_latency / low_latency / balanced / high_quality
- Adaptive runtime with CPU + queue watchdog
- mDNS auto-discovery + QR code connection
- Multi-device support (up to 4 clients)
- System tray with volume control, show/quit
- Native 3-band EQ on Oboe path
- Latency chart with smooth Catmull-Rom curve
- MediaSession + ForegroundService for background playback
- Reverse mic channel (Android → PC via VB-Cable)
- USB adb direct connection mode
- GitHub release workflow (APK + EXE + checksums)

## Next Priorities

### 1. Stability & Polish

- Continue reducing underrun risk under real device power-saving conditions
- Improve reconnect behavior on network transitions (Wi-Fi → mobile → Wi-Fi)
- Better error messages and guided troubleshooting in both UIs

### 2. User Experience

- Onboarding flow for first-time users (firewall, Wi-Fi setup)
- Notification controls on Android (play/pause/disconnect from notification)
- Remember last connected server and auto-reconnect option
- Dark/light theme option on Android

### 3. Protocol Evolution

- Capability-driven negotiation (less config-driven)
- Keep `legacy_las1 + pcm16` as durable rollback
- Evolve `usb_direct` without weakening Wi-Fi main path

### 4. Platform Expansion (Future)

- macOS audio capture support
- iOS client
- Multi-room synchronized playback

## Not In Immediate Scope

- Cloud relay / internet streaming
- Phone-as-mic productization (exists as experimental)
- Replacing rollback with silent fallback
- App store distribution

## Working Rules

- Main path stays explicit
- Rollback stays explicit and always available
- UI/runtime coupling goes through shared contracts
- Release decisions come from gate artifacts, not prose
