# Changelog

All notable changes to LAN Audio are documented in this file.

The format follows Keep a Changelog, and this project uses `v<major.minor>` release tags.

## [1.13] - 2026-05-18

### Added — Ultra Low Latency Mode

- **Ultra-low-latency mode** (`ultra_low_latency`). Fourth playback mode targeting ≤30ms end-to-end latency for gaming and video sync scenarios.
- **5ms capture frames**. WASAPI capture period reduced from 10ms to 5ms when ultra-low-latency mode is active, halving capture latency.
- **PCM16 passthrough by default**. Ultra-low-latency mode skips Opus encoding entirely (0ms encode latency) and transmits raw PCM16 at 5ms frame cadence (200 packets/sec).
- **Android playback profile**. `startBufferMs=10`, `maxBufferMs=60`, `frameDurationMs=5`, `dropThresholdMs=40` — aggressive jitter buffer for minimal playback delay.
- **Auto-degradation**. When network jitter p95 exceeds 10ms over a sliding window, the mode automatically falls back to `low_latency` with reason `auto_degraded_jitter`.
- **Protocol capability**. `supports_ultra_low_latency` field added to `ProtocolCapabilities`. Both peers must declare support for the mode to be activatable.
- **UI**. Mode selector now shows 4 options: "极速 ≤30ms" / "低延迟 ~64ms" / "均衡" / "高质量".

### Latency Budget

| Stage | Before (low_latency) | Now (ultra_low_latency) |
|-------|---------------------|--------------------------|
| Capture | 10ms | 5ms |
| Encode | 20ms (Opus) | 0ms (PCM16 passthrough) |
| Transport | ~2ms | ~2ms |
| Jitter buffer | 40ms | 10ms |
| Playback | ~10ms | ~5ms (Oboe Exclusive) |
| **Total** | **~64ms** | **~22ms** |

## [1.12] - 2026-05-18

### Fixed

- **Windows window not appearing on startup**. The `tauri.conf.json` contained an invalid `"focus": true` field that caused Tauri to panic silently in release builds. Removed the invalid field; window now appears correctly with `visible: true` + `center: true`.
- **ICO icon generation**. The icon generation script produced a 247-byte malformed ICO. Fixed to produce proper multi-size ICO (4962 bytes).

### Changed

- **System tray optimization**. Added "显示窗口" (Show Window), "退出" (Quit) menu items. Left-click tray icon now shows/focuses the window. Added tooltip "LAN Audio".
- **New app icons**. Custom-designed icons for both Windows (.ico) and Android (mipmap) — dark rounded square with audio waveform bars and WiFi signal arc.
- **Window dimensions**. Reduced from 1180×760 to 680×760 to match actual content width. Added `center: true` for centered startup.
- **Product name**. Simplified from "LAN Audio Desktop Client" to "LAN Audio" across all surfaces.

### Cleaned Up

- **Version consistency**. Fixed `kAppVersion` from stale '1.8.3' to match VERSION file.
- **Package rename**. `lan_audio_android_mvp` → `lan_audio_android` in pubspec.yaml.
- **App label**. Changed from Chinese-only "LAN Audio 控制台" to universal "LAN Audio".
- **Removed CAMERA permission**. Unnecessary for an audio streaming app.
- **Removed PCM24 from UI**. Codec picker now shows only Auto / Opus / PCM 16 (protocol layer retained for future use).
- **Debug section renamed**. "调试指标" → "高级信息" for less developer-facing appearance.
- **Mic card hidden by default**. Only shown when virtual audio device is detected or mic is active.
- **Stale docs removed**. Deleted `docs/roadmap-v1.4.md`, rewrote `docs/roadmap.md` for current state.

## [1.11] - 2026-05-18

### Changed — UI Polish & Latency Chart Redesign

- **Latency chart Scheme A**. Replaced dual area-fill chart with: smooth Catmull-Rom curve (current) + dashed horizontal baseline reference + real-time ms value in top-right corner. Y-axis simplified to top/bottom labels only. Latest point highlighted with glowing dot.
- **Windows desktop layout optimization**. Container widened to 560px, card padding/spacing tightened, QR code shrunk to 64px, latency chart height reduced to 70px.
- **Android UI fixes**. Fixed mode selector button overflow (shortened labels), removed PCM24 codec option, removed mysterious white Divider line in mic section.

## [1.10.2] - 2026-05-16

### Fixed

- **Resampler edge artifact ("电音/爆音") on non-48 kHz USB DACs**. The Opus / PCM16 encode path created a fresh `rubato::SincFixedIn` resampler instance for every 10 ms audio frame. Each new instance starts with empty filter history, so the sinc convolution at frame boundaries produced edge artifacts — about 12% of samples at each frame transition were corrupted. With a 96 kHz USB DAC (HiBy FC1, K3, etc.) the artifacts manifested as a continuous high-frequency crackle audible across all codecs.
- The fix moves the resampler to a persistent field on `AudioFrameEncoder`. The instance is built once per (in_rate, out_rate, in_frames) combination and reused across frames, preserving the sinc filter's internal history buffer for continuous sample reconstruction. Same-rate fast path and nearest-neighbor fallback are unchanged.
- Affects all capture inputs where `mix_format_hz != output_sample_rate` (typically Hi-Res USB DACs running at 96/192 kHz, but also any device with a non-48 kHz Windows mix format).

### Reverted

- Android-side PCM24 Hi-Res passthrough has been **temporarily reverted** to the v1.9.4 baseline. The Oboe Float callback path was unstable across test devices (Shared mode consumed audio at the device native rate instead of the requested 48 kHz). PCM24 support will return as a fully isolated playback path in a future release; the wire protocol (v3, fragmentation, `supports_hires_pcm24`) remains intact server-side for forward compatibility.

### Notes

- This release fixes the long-standing crackle that affected **all encoding modes** when used with Hi-Res USB DACs as the Windows default output. Users on the standard 48 kHz mix format were unaffected and see no behavior change.
- No protocol changes. Existing v1.10.x clients will benefit from the server fix without an Android update.

## [1.10.0] - 2026-05-14

### Added — Hi-Res PCM24 Passthrough (Phase 6)

- **Protocol v3** wire format (`PROTOCOL_VERSION_V3=3`, magic still `LAV2`). Adds `frag_index/total_frags/logical_seq` fields by repurposing the v2 `reserved` slot and appending a 4-byte sequence number. Total v3 header = 37 bytes.
- **PCM24 codec** (`UdpAudioCodecV2::Pcm24=4`). 24-bit signed integer, big-endian, native-rate (no resampling). Server emits one EncodedFrame per native input frame; transport layer fragments above 1392-byte boundary.
- **Application-layer fragmentation**. 96 kHz / 5 ms / stereo PCM24 = 2880 B per logical frame, automatically split into 2 v3 packets sharing a logical_seq. Android client reassembles via `LasPacketReassembler` (LRU 8 slots).
- **`supports_hires_pcm24` capability**. Defaults to `false` — only when both peers advertise true does the server emit v3 + Pcm24 packets. Older peers never see v3 traffic.
- **PCM24 in `AudioCodecPreference` enum**. Clients can request `preferred_codec: "pcm24"` in `SetAudioMode`. Server downgrades to Opus when adaptive watchdog enters Red tier or when the data plane can't carry the codec.

### Changed

- **Resampler upgrade**. Replaced the Opus path's nearest-neighbor stereo downmix with a polyphase Sinc resampler (`rubato 0.16`, Cubic interpolation, 64-lobe sinc, Blackman-Harris2 window). When Windows mix format is not 48 kHz the prior aliasing distortion disappears. Same-rate fast path is preserved (zero copy + one pass).
- `CodecSelection::Pcm24` added to `lan_audio_server::config`. CLI accepts `--codec pcm24` / `--codec hires` / `--codec hires_pcm24`.
- Android `LasPacketParser` extended to parse v3 packets (`protocol_version=3`, header_size=37) alongside v2.
- Android `OboeAudioTrackController` keeps PCM16 output; PCM24 is downconverted BE→LE i16 in `PlaybackSessionRuntime` (drops the bottom byte). Float-aware Oboe sink remains a future task.

### Notes

- Bandwidth: PCM24 48 kHz/stereo ≈ 2.3 Mbps; PCM24 96 kHz/stereo ≈ 4.6 Mbps. LAN-only feature.
- UI: the codec picker on the More page now treats PCM 24 as a real selection (not a placeholder). The amber hint reminds users to set Windows mix format to 96 kHz to actually hear the Hi-Res benefit.
- Tests: 108/108 Rust tests pass (8 domain + 28 protocol + 70 server lib + 1 multi_client + 1 opus_stress); 31/31 Flutter tests pass; gradle assembleDebug succeeds.
- Rollback: `--codec opus` (server CLI) or selecting Opus in the UI immediately falls back to v2_header + Opus. The legacy `--force-rollback` (`legacy_las1 + pcm16`) path is preserved.

## [1.9.4] - 2026-05-14

### Added

- **Codec picker.** A new "编码器 / Codec" card on the More page lets users pick between Auto / Opus / PCM 16 / PCM 24. PCM 24 is intentionally still selectable but the server falls back to Opus until the Hi-Res passthrough path lands; the card surfaces a hint explaining that.
- Protocol: `SetAudioMode` and `AudioModeChanged` gained an optional `preferred_codec` / `effective_codec` field. Both default-skip on serialize and back-compat decode (older clients omitting them keep working; older servers ignore them).

### Changed

- `ClientRegistry::set_client_mode` now accepts a `Option<CodecSelection>` so the codec can be swapped at runtime without renegotiating the session. `Opus` requests on `legacy_las1` are downgraded to `Pcm16` server-side.
- `MorePage` constructor adds `preferredCodec` + `onPreferredCodecChanged` to thread the user's choice through `MainShell`.

### Notes

- Per-client codec change resets `pending_first_packet` so the encode worker rebuilds its encoder bank lazily on the next frame. No glitch is expected, but expect a ~20 ms pause on transition.
- `effective_codec` is echoed back to the client so the UI can show the resolved codec (independent of what the user requested).

## [1.9.3] - 2026-05-14

### Fixed

- **EQ now actually works on Oboe playback path.** Previously the 3-band equalizer in the Flutter UI (`Low 60Hz / Mid 1kHz / High 10kHz`) was silently ignored on Android 8.1+ devices because `OboeAudioTrackController` had no `setEqSettings` override and inherited the default no-op from `PlaybackAudioSink`. Since Oboe is the default backend on `Build.VERSION.SDK_INT >= O_MR1` (~99% of active devices), the EQ was effectively dead code in production.
- The `AudioTrackController` (legacy fallback) was unaffected because it overrides `setEqSettings` to drive the platform `android.media.audiofx.Equalizer`.

### Added

- Native 3-band biquad peaking EQ in `oboe_sink.cpp`. RBJ Audio EQ Cookbook formulas, hard-coded to 60 Hz / 1 kHz / 10 kHz at Q=0.7 to match the existing UI labels. PCM samples are processed in-place in chunks of up to 1024 frames before being written into the ring buffer.
- New JNI binding `nativeSetEqSettings(enabled, lowDb, midDb, highDb)` exposed to Kotlin.
- `OboeAudioTrackController.setEqSettings` override that calls into the native sink when the stream is open. The Kotlin sink is recreated on every `init`, and the runtime calls `setEqSettings` after `init`, so settings are correctly re-applied across mode switches and reconnects.

### Implementation Notes

- Filter coefficients are read-only on the producer thread. The method-channel thread writes new gains under `eq_state_mutex_` and flips `eq_pending_dirty_`; the producer thread drains the pending update at the start of every `pushPcm` call and rebuilds coefficients in-place. Delay state (z1/z2/y1/y2) stays attached to the producer's filter bank across calls so a small gain change does not introduce a discontinuity.
- When EQ is disabled, the cascade is bypassed entirely (no per-sample math, no working-buffer copy). Toggling on/off is therefore zero-cost.

## [1.9.2] - 2026-05-14

### Fixed

- Connection list reshuffle: server-side `build_client_list_json` now sorts entries by client UUID before broadcasting, so `client_list` JSON is deterministic across ticks. The desktop / Android UI no longer reorders the device list every time a beacon arrives.
- Android nearby-device duplicates: the discovery list now dedupes across mDNS / UDP / probe sources by `host:wsPort`, with a priority ranking (`mdns > udp > probe`) so a higher-confidence source displaces a lower-confidence one for the same physical server. Fixes the "two devices" symptom where probe-IP and mDNS UUID both appeared simultaneously.
- Android list secondary-sort tiebreaker switched from `lastSeen` (volatile, updates every beacon) to `host` (stable). Servers without a recent-connected entry no longer reshuffle on every refresh.

### Added

- Audio quality strip on the Play page: under the latency chart, the app now shows the negotiated codec, sample rate, and channel count (e.g. `Opus · 48 kHz · 立体声`). Hidden until the WebSocket session is established. Pure passive readout — no controls.

### Tests

- Added `client_list_json_is_deterministically_ordered_by_id` to lock in the new server-side sort.
- 98/98 Rust tests passing (67 server lib + 22 protocol + 8 domain + 1 multi_client + 1 opus_stress).

## [1.9.1] - 2026-05-14

### Changed

- Phase 5 MMCSS: encode pipeline migrated from inline async block to a dedicated `std::thread` (`lan-audio-encode`). The thread registers MMCSS "Audio" on Windows and holds the registration for the worker's lifetime, so the boost is now actually applied.
- `BroadcastTransport::run` is now a 3-stage pipeline (capture → encode worker → dispatch) communicating via `std::sync::mpsc` (async → worker) and `tokio::sync::mpsc::unbounded` (worker → async).
- The shared sequence counter is now an `Arc<AtomicU32>` advanced by the worker per encoded packet — the async dispatch no longer needs to predict packet count.

### Added

- `EncodeJob` / `WireFrameOut` / `EncodeResult` types and `spawn_encode_worker` helper in `transport.rs`.
- Smoke test `encode_worker_emits_one_wire_frame_per_recipient` exercising a real worker thread end-to-end (no test runtime needed).

### Notes

- No protocol change, no CLI change. Pure server-internal refactor.
- Behaviour is identical on non-Windows hosts (MMCSS registration is a no-op there).
- Existing `--no-adaptive-runtime` rollback path still works; the encode worker is unconditional because it's now the only encode path.

## [1.9.0] - 2026-05-14

### Added

- Phase 3 client watermark feedback channel: Android client now reports buffer level, ring-buffer depth, silence-fill / underrun deltas, and jitter p95 once per second on the existing WebSocket control channel as `client_watermark`.
- Phase 4 server-side adaptive runtime: CPU + queue-pressure watchdog ticking at 500 ms decides a Green / Yellow / Red tier and reconfigures live `AudioFrameEncoder`s; Red tier forces PCM16 fallback for predictable CPU cost.
- Phase 4 metrics: `MetricsSnapshot` now exposes `adaptive_tier`, `adaptive_predicted_cpu_percent`, and `adaptive_queue_ratio`.
- Server CLI flag `--no-adaptive-runtime` (and `--adaptive-runtime`) to disable / re-enable the watchdog as a rollback path.
- Protocol crate: new `WatermarkReport` struct + `ClientControlMessage::ClientWatermark` variant; older servers safely ignore the unknown tag.

### Changed

- v1.8.7 already landed Phase 1 (Soft Limiter + Peak-Ahead Guard + 高增益迟滞) and Phase 2 (Android + desktop latency chart). v1.9.0 wires the previously-unwired Phase 3 / 4 modules into the live broadcast hot path.
- Phase 5 MMCSS module (`thread_priority`) remains kept as a public API but is **not** registered from tokio tasks (registration is unreliable across worker migrations); a follow-up to migrate capture / encode / send to dedicated `std::thread` is tracked.

### Rollback

- Run the server with `--no-adaptive-runtime` to disable Phase 4 (encoder stays at default per-mode bitrate, Phase 3 watermark messages are still ignored gracefully).
- The legacy `legacy_las1 + pcm16` path is preserved.

## [1.8.0] - 2026-05-12

### Added

- Android mic → PC reverse audio channel: Opus-encoded TCP stream on port 7878, with named-pipe output on Windows and per-frame level metering.
- Real-time jitter visualization sparkline in Audio Console Dark, showing per-segment coloring and p50/p95 readout.
- PC-side volume control of Android via TCP control channel on port 7879, with desktop tray volume presets and Android volume pill indicator.
- Reverse channel control JSON protocol (ports 7878/7879) with mic gain, volume, and device-name messages.
- `ReverseChannelServer` in Rust server with concurrent audio and control listeners.
- `ControlChannelService` on Android for receiving and applying remote volume commands, with auto-reconnect.
- `JitterGraphWidget` (Flutter CustomPainter) with three-zone coloring, dashed reference lines, and empty-buffer safety.

### Changed

- Audio Console Dark UI restructured: Hero orb card for primary status, technical metrics moved into expandable collapsible section, Chinese labels for metrics fields.
- Playback card now shows only mode selector, status, and underrun count; conditional audio log line.
- `PlaybackSessionRuntime` collects per-packet jitter timing and exposes a 120-sample circular buffer with p50/p95 computation.
- `PlaybackServiceSnapshot` (Dart) exposes jitter helpers and underrun count from metrics.
- `DesktopSnapshot` (Tauri) surfaces reverse channel mic state and Android volume.

## [1.7.2] - 2026-05-11

### Fixed

- Restored the Android production entry to Audio Console Dark after the v1.7 merge regression.
- Reconnected `buildAudioConsoleTheme()`, `HeroStatusWidget`, `ServerCardWidget`, `ModeSelectorWidget`, and `DangerActionButton` in `main.dart`.
- Preserved the v1.7 connection and audio quality logic while restoring the newer UI shell: mDNS discovery, smart reconnect, history/favorites, EQ, and loudness normalization remain available.
- Fixed narrow-screen overflow in the discovery status rows.

### Tests

- Added `app_entry_smoke_test.dart` to fail if the app entry falls back to the old MVP UI again.
- Added merge validation checklist items to `AGENTS.md` for Audio Console Dark entry checks.

## [1.7.1] - 2026-05-11

### Fixed

- Corrected the desktop and Android update checker GitHub API endpoint to `MengBad/lan-audio`.
- Moved Android update checks off the main thread to avoid `NetworkOnMainThreadException`.
- Kept desktop manual update checks on a non-blocking path so release lookup cannot freeze the Tauri UI thread.
- Aligned Android release signing in GitHub Actions with the fixed keystore-backed `local.properties` flow.
- Kept Android `versionCode` monotonic for the v1.7 patch release (`1.7.1` -> `3071`).

### Verification

- `scripts/validate_local.ps1` passed.
- `cargo test -p lan_audio_desktop -- update` passed.
- `flutter test` passed.
- Main branch Android APK workflow passed after the release signing fix.

## [1.7.0] - 2026-05-10

### Added

- Android 3-band EQ using Android `Equalizer`, with low/mid/high controls, presets, persistence, and stable snapshot fields.
- Loudness normalization on the Android PCM write path with RMS analysis, bounded software gain, ramp smoothing, and `low_latency` bypass.
- Multi-device server streaming support for up to 4 Android clients with independent sessions and disconnect isolation.
- Desktop active-device list summaries for multi-client sessions.
- mDNS LAN service registration on the Windows sender and Android `NsdManager` discovery for nearby devices.
- Smart reconnect with 1s, 2s, 4s, 8s, and 16s backoff, plus `reconnect_attempts` and `reconnect_delay_ms` snapshot fields.
- Connection history and favorites on Android, persisted with SharedPreferences and capped at 10 entries.
- Android power-saving guidance page and buffering-timeout notification for Xiaomi, Huawei, and generic Android battery policies.
- Contributor documentation, issue templates, pull request template, Codecov badge, protocol coverage workflow, and this changelog.

### Changed

- `PlaybackSessionController.kt` is now a small coordinator; playback state and jitter coordination were split into dedicated modules.
- Protocol tests now cover negotiation errors, v2 header round trips, legacy/v2 handshake distinction, and mode profile boundaries.
- CI can generate LCOV coverage for `lan_audio_protocol` and upload it to Codecov.

### Fixed

- USB direct stability was improved with timeout and buffer handling on the server and Android connection path.
- Reconnect exhaustion now enters an explicit error state instead of silently falling back to disconnected.

### Known Limitations

- Desktop per-device disconnect command is deferred to v1.8 until a stable per-client command contract exists.

## [1.6.0] - 2026-05-10

### Added

- Playback runtime modules for buffer policy, pacing, latency guard, and metrics collection.
- USB direct groundwork using `adb reverse` and a length-prefixed TCP data path.
- Shared playback diagnostics used by Android and desktop snapshots.

### Changed

- The recommended main path remains `windows_loopback + v2_header + opus`.
- The permanent rollback path remains `legacy_las1 + pcm16`.

### Fixed

- Improved Android playback diagnostics around buffering, underrun, silence fill, and sink write gaps.
- Reduced coupling between playback runtime internals and UI-facing snapshot state.

## [1.5.0] - 2026-04-23

### Added

- Android foreground MediaSession integration with MediaStyle notification controls.
- Manual update-check entry for Android and desktop update banner support.
- Desktop diagnostics export to JSON.
- Release packaging and validation refinements for Android APK and Windows executable outputs.

### Changed

- Protocol v2 and Opus were promoted as the recommended product path after synthetic and loopback validation work.
- Mode profiles for `low_latency`, `balanced`, and `high_quality` became the shared source of playback strategy.

### Fixed

- Duplicate-start and app-open restore regressions in Android foreground playback.
- Balanced-mode buffering behavior through one-shot refill and startup/steady-state silence accounting.
- Mode-switch diagnostics and startup silence classification.
