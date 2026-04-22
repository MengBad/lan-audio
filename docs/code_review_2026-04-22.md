# Code Review Report (2026-04-22)

## Scope

## Follow-up Status (2026-04-22)

- [x] Protocol payload length now uses checked conversion in encoder paths.
- [x] Android parser preserves V2 16-bit flags.
- [x] Android protocol hints now trigger playback pipeline resync.
- [x] Desktop start is guarded while service status is `Stopping`.

- crates/lan_audio_protocol
- crates/lan_audio_server
- apps/android_flutter
- apps/desktop/src-tauri

## Potential issues and bugs

### 1) Packet payload length is narrowed to `u16` without bounds checks

**Risk level:** High (data corruption / decode mismatch)

`UdpAudioPacket::encode` and `UdpAudioPacketV2::encode` both cast payload length to `u16` directly. If payload size exceeds `65535`, the encoded length field wraps/truncates and decode-side length checks can fail or misinterpret packets.

Files:
- `crates/lan_audio_protocol/src/lib.rs` (`self.payload.len() as u16` in both v1/v2 encode paths).

Recommended fix:
- Return an explicit encode error when payload length exceeds `u16::MAX`.
- Add regression tests for oversized payloads in both LAS1 and LAV2 encoders.

### 2) Android V2 flags are truncated to low 8 bits

**Risk level:** Medium-High (protocol semantics dropped silently)

V2 header carries a 16-bit `flags` field, but Android parser stores only `flags16 & 0xFF`, so any high-bit flags are lost. This will break future protocol extensions that rely on bits `8..15`.

Files:
- `apps/android_flutter/lib/audio/las_packet.dart`

Recommended fix:
- Keep a dedicated `flagsV2` 16-bit field for parsed packets.
- Keep legacy `flags` only for LAS1 compatibility if needed.

### 3) `config_changed` / `discontinuity` hints are detected but not applied

**Risk level:** Medium (audible glitches during mode/config transitions)

Android packet handler recognizes `hasConfigChanged` / `hasDiscontinuity`, but currently only writes `_audioLog`; it does not trigger jitter reset, decoder reset, or playback pipeline resync.

Files:
- `apps/android_flutter/lib/main.dart`

Recommended fix:
- On `config_changed`, rebuild playback config (`sample_rate`, `channels`, codec-specific state).
- On `discontinuity`, clear jitter buffer and reset decoder PLC/state before resuming.
- Add integration test coverage for mode switch sequence and discontinuity handling.

### 4) Desktop service allows `start` while status is `Stopping`

**Risk level:** Medium (restart races / bind failures)

`start_service_impl` only short-circuits on `Running | Starting`, not `Stopping`. If a user calls stop (non-blocking path) then quickly start, a new start may race with old service shutdown and fail to bind ports.

Files:
- `apps/desktop/src-tauri/src/lib.rs`

Recommended fix:
- Treat `Stopping` as non-startable state, or block until stop task completes.
- Optionally make `stop_service` always synchronous for UI-driven lifecycle transitions.
