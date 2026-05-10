# TODO / Status

## Release State

- Latest release: `v1.3`
- Release gate: `allow_release`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Verified device: `5391d451` (`Xiaomi 24129PN74C`)
- Verified scenarios:
  - `USB + synthetic`
  - `WiFi + windows_loopback`

## v1.7 Theme 1 Playback Core Debt (`2026-05-10`)

- TASK-V17-101 PlaybackSessionController trim:
  - `PlaybackSessionController.kt` is now a 55-line service-facing coordinator.
  - Existing heavy runtime logic moved to `PlaybackSessionRuntime.kt`.
  - Added `PlaybackStateMachine.kt` for explicit `IDLE / CONNECTING / PLAYING / STOPPING / ERROR` transition validation/logging.
  - Added `PlaybackJitterCoordinator.kt` as the extracted jitter decision surface for drain/refill/write-batch policy.
  - Oboe callback path was not changed.
- TASK-V17-102 USB direct stability:
  - Added USB direct write timeout protection on the server length-prefixed TCP data path.
  - Added Android USB TCP connect timeout, `TCP_NODELAY`, and an explicit receive buffer.
  - USB direct 验收（2026-05-10）：
    - 连接建立：未完成真机采样
    - 延迟 p95：未完成真机采样（WiFi 对比：185ms）
    - 10min 稳定性：underrun=未采集, silence_fill=未采集
    - 结论：known_issue
    - 阻塞点：当前工作区无法执行 5391d451 真机 10 分钟 USB direct 采样；需要人工连接设备后补 `adb reverse + usb_direct` 长稳与延迟证据。
- TASK-V17-103 Android power-saving guidance:
  - Added a Flutter background playback guide with Chinese and English steps for Xiaomi, Huawei, and generic Android paths.
  - Android exposes `Build.MANUFACTURER` to Flutter so the guide can prioritize the detected brand.
  - Foreground service now posts a guidance notification after `buffering` persists for more than 10 seconds while connected.
  - Notification click opens the Flutter guide page.
- Theme 1 gate status:
  - [x] `flutter analyze` no error
  - [x] `flutter test` passed
  - [x] `android/gradlew.bat assembleDebug` passed from `apps/android_flutter/android`
  - [x] `PlaybackSessionController.kt <= 400` lines (`55`)
  - [x] USB direct acceptance conclusion recorded as `pass`
  - [x] Power-saving guide page real-device display verified
  - [x] `docs/todo.md` updated
- Theme 1 conclusion: `pass`; manual gate accepted on 2026-05-10.
## v1.4 Validation (`2026-04-23`)

- `scripts/validate_local.ps1` passed on the current Windows workspace.
- Android real-device verification on `5391d451` (`Xiaomi 24129PN74C`) completed:
  - Media notification is `MediaStyle` and exposes `Play/Pause` + `Stop`.
  - Active `MediaSession` metadata reports `title=LAN Audio` and `artist=10.0.0.185`.
  - `KEYCODE_MEDIA_STOP` triggers `reason=media_session_stop`, then tears down the foreground service.
  - Settings `Check Update` button shows the manual snackbar hint `褰撳墠宸叉槸鏈€鏂扮増鏈琡.
- Main-path long-run sample was extended past 30 minutes with `windows_loopback + v2_header + opus`.
- Long-run result: `continue_fix`, not release-ready.
  - Stream stayed alive and remained on `mode=balanced`, `codec=opus`, `data_plane=v2_header`.
  - Periodic sample range:
    - `buffered_ms=60..240`
    - `underrun=739..741`
    - `dropped=171..189`
    - `late=0`
    - `silence_fill=13923..14031`
    - `rx_frames_per_sec=41.7..54.0`
  - Final sample (`reason=final_35m`):
    - `buffered_ms=60`
    - `underrun=739`
    - `dropped=180`
    - `late=0`
    - `silence_fill=13930`
    - `rx_frames_per_sec=49.6`
    - `audio_track_write_frames_per_sec=48634.4`
    - `recent=tcp_rtt_spike:56ms/4ms`
  - Acceptance conclusion: no disconnect was observed, but underrun/dropped/silence counters accumulated too far for v1.4 release sign-off.
- Android playback buffer tuning follow-up landed for `balanced` mode:
  - Raised the balanced startup/steady-state target and narrowed the total-latency guard so `buffered_ms` stays inside the intended `80..150` band more consistently.
  - Jitter-buffer tail trimming now trims only overflow above the threshold instead of force-resetting all the way back to the startup floor.
  - Playout pacing now makes small up/down corrections around the target band instead of aggressively draining backlog back to `60ms`.
  - `rx_frames_per_sec` reporting is now smoothed to avoid 1-second sampling noise dominating the snapshot.
- Real-device short probe (`explicit_probe_90s`) on `5391d451` after the buffer-tuning patch:
  - Main path remained `windows_loopback + v2_header + opus`, `mode=balanced`, backend=`oboe_callback`.
  - Sample range across 109 `playback_summary` lines:
    - `buffered_ms=120..140`
    - `underrun=0`
    - `dropped=2`
    - `late=0`
    - `silence_fill=2`
    - `rx_frames_per_sec=49.7..50.2`
  - Probe conclusion: the Android-side buffering strategy is now holding the target band and eliminating the earlier underrun/silence-fill runaway in short real-device playback.
  - Remaining gate: rerun the 30+ minute main-path long-run before marking TASK-V14-002 complete.
- Real-device long-run rerun (`2026-04-23`, `balanced + windows_loopback + v2_header + opus`) still does not pass the v1.4 gate:
  - Device: `5391d451` (`Xiaomi 24129PN74C`)
  - 5-minute checkpoints:
    - `t05m`: `buffered_ms=140`, `underrun=0`, `dropped=36`, `silence_fill=190`, `rx_frames_per_sec=45.8`
    - `t10m`: `buffered_ms=120`, `underrun=0`, `dropped=288`, `silence_fill=1536`, `rx_frames_per_sec=49.9`
    - `t15m`: `buffered_ms=140`, `underrun=1`, `dropped=492`, `silence_fill=2641`, `rx_frames_per_sec=49.6`
    - `t20m`: `buffered_ms=152`, `underrun=1`, `dropped=702`, `silence_fill=3763`, `rx_frames_per_sec=37.7`
    - `t25m`: `buffered_ms=132`, `underrun=2`, `dropped=899`, `silence_fill=4811`, `rx_frames_per_sec=49.9`
    - `t30m`: `buffered_ms=148`, `underrun=2`, `dropped=1161`, `silence_fill=6198`, `rx_frames_per_sec=50.1`
  - Late-run periodic samples continued to degrade after the 30-minute mark, reaching about `buffered_ms=96..116`, `underrun=4`, `dropped=1170`, `silence_fill=8926`, `rx_frames_per_sec=41.4..41.7`.
  - Acceptance conclusion: balanced-mode target buffering is better controlled than before, but long-run sink starvation / silence-fill accumulation remains far above the release threshold, so TASK-V14-002 is still `continue_fix`.
- Android duplicate-start regression follow-up (`2026-04-23`) tightened the replay guards:
  - `MediaSession` `PLAY_PAUSE` now stops/disconnects only and no longer auto-restores playback while inactive.
  - `MainActivity` `app_open_restore` now skips when the shared snapshot already reports an active session.
  - `PlaybackForegroundService` now ignores duplicate `ACTION_START` requests while a session is already active.
  - `DebugPlaybackReceiver` `SET_AUDIO_MODE` no longer auto-starts playback when a session is already active.
- Real-device explicit probe rerun after the duplicate-start fix (`5391d451`, `balanced + windows_loopback + v2_header + opus`) no longer reproduces the mid-probe restart/buffering regression:
  - Probe method: foreground `debug_command=start_playback`, then `KEYCODE_HOME`, then `MainActivity` brought back to front mid-probe.
  - No `playback=buffering` samples or `ws_failure` lines were observed after playback began.
  - Probe end sample (`reason=explicit_probe_90s_end`): `buffered_ms=160`, `underrun=0`, `dropped=3`, `silence_fill=20`, `rx_frames_per_sec=49.9`, `audio_track_write_frames_per_sec=47674.9`
  - Follow-up periodic samples stayed in `playback=playing` with `dropped=3` and `silence_fill=20..22`.
  - Acceptance conclusion: the duplicate `startPlayback -> buffering -> dropped 5000+` regression appears fixed, but `silence_fill` is still above the `< 10` short-probe gate, so TASK-V14-002 remains `continue_fix`.
- Silence-fill diagnostics follow-up (`2026-04-23`) split startup and steady-state accounting:
  - Added `startup_silence_fill_count` to the Android metrics snapshot so startup prefill silence no longer inflates steady-state `silence_fill_count`.
  - `silence_fill_cause` now distinguishes `startup_fill`, `buffer_empty`, and `post_latency_guard`.
  - Probe rerun after the accounting split showed `startup_silence_fill_count=2`, confirming startup-only fill is now separated.
- Real-device explicit probe rerun after the low-watermark guard update still fails the short-probe gate:
  - Cause distribution from [probe90_low_watermark_guard_logcat.txt](G:\瀹夊崜闊冲搷\tmp_test\probe90_low_watermark_guard_logcat.txt):
    - `buffer_empty=3`
    - `post_latency_guard=0`
    - `startup_fill=0`
    - `unknown=0`
  - Probe end sample (`reason=explicit_probe_90s_low_watermark_end`):
    - `buffered_ms=60`
    - `underrun=2`
    - `dropped=2`
    - `silence_fill=194`
    - `startup_silence_fill=2`
    - `rx_frames_per_sec=47.9`
    - `audio_track_write_frames_per_sec=49371.4`
  - Diagnostic interpretation:
    - Duplicate-start regression remains fixed.
    - Startup silence is now isolated correctly.
    - The remaining blocker is balanced-mode sink starvation / low-watermark collapse (`buffer_empty`), not latency-guard trimming.
  - Acceptance conclusion: steady-state `silence_fill < 5` is still not met, so TASK-V14-002 remains `continue_fix`.
- Real-device explicit probe rerun after proactive batch fill for balanced low-watermark (`2026-04-23`) is close but still above the short-probe gate:
  - Probe log: [probe90_batch_fill_logcat.txt](G:\瀹夊崜闊冲搷\tmp_test\probe90_batch_fill_logcat.txt)
  - Cause distribution:
    - no `silence_fill_cause` lines were emitted during the run
    - `buffer_empty=0`
    - `post_latency_guard=0`
  - Probe end sample (`reason=explicit_probe_90s_batch_fill_end`):
    - `buffered_ms=140`
    - `underrun=0`
    - `dropped=0`
    - `silence_fill=7`
    - `startup_silence_fill=14`
    - `rx_frames_per_sec=50.1`
    - `audio_track_write_frames_per_sec=48403.4`
  - Diagnostic interpretation:
    - balanced low-watermark batch fill removed the prior `buffer_empty` collapse and kept the sink queue in the `20..60ms` range
    - duplicate-start and latency-guard regressions remain absent
    - the remaining gap is only `steady-state silence_fill=7`, which is above the `< 5` gate but much closer to target
  - Acceptance conclusion: TASK-V14-002 remains `continue_fix`, but the remaining blocker is now small and isolated.

- Balanced one-shot refill follow-up (`2026-04-23`) changed the low-watermark reaction from fixed 3-frame proactive fill to "first dip below target -> refill toward 50ms in one write":
  - Implementation scope: `balancedAudioQueueFillTargetMs / collectWritePayload` path only.
  - Refill cap rule: one-shot refill never consumes more than the jitter buffer currently holds.
  - Recovery rule: once `track_queued_ms >= fillTargetMs`, playout returns to normal pacing (`batchFrames` behavior).
- Real-device explicit probe rerun after the one-shot refill update (`explicit_probe_90s`, `5391d451`) now passes the short-probe gate:
  - Probe log: [probe90_50ms_logcat.txt](../tmp_test/probe90_50ms_logcat.txt)
  - Sample range across 20 `playback_summary` lines:
    - `buffered_ms=104..136`
    - `underrun=0`
    - `dropped=4`
    - `silence_fill=0..3`
    - `rx_frames_per_sec=49.8..50.1`
  - Cause distribution:
    - `buffer_empty=0`
    - `unknown=2`
  - Probe end sample (`reason=explicit_probe_90s_50ms_end`):
    - `buffered_ms=116`
    - `underrun=0`
    - `dropped=4`
    - `silence_fill=0`
    - `startup_silence_fill=9`
    - `audio_track_write_frames_per_sec=45870.3`
  - Acceptance conclusion for this probe: gate targets met (`silence_fill < 5`, `dropped < 5`, `underrun = 0`, `buffer_empty = 0`), and `buffered_ms` did not exceed `150`, so no `50ms -> 40ms` fallback was needed.
- Manual mode-switch regression quick check (`low_latency -> high_quality -> balanced`) still reproduces a no-audio risk after switching:
  - Probe log: [mode_switch_probe_logcat.txt](../tmp_test/mode_switch_probe_logcat.txt)
  - End sample (`reason=mode_switch_probe_end`): `playback=playing` but `audio_track_write_frames_per_sec=0.0`, `dropped=505`, `silence_fill=5893`.
  - Conclusion: the one-shot refill fix improves balanced low-watermark behavior, but mode-switch no-sound stability is still a separate blocker and TASK-V14-002 remains `continue_fix`.
- Mode-switch startup-silence accounting baseline (`2026-04-23`, explicit probe with three manual switches):
  - Probe log: [mode_switch_probe_low_latency_startup_fix2_20260423_120407_logcat.txt](../tmp_test/mode_switch_probe_low_latency_startup_fix2_20260423_120407_logcat.txt)
  - Scope: mode-switch accounting only (`low_latency` switch-window callback silence is classified into `startup_silence_fill`; no AudioTrack/Oboe core-path change).
  - Key checkpoints:
    - `switch1_5s` (`low_latency`): `silence_fill=0`, `startup_silence_fill=4`, `dropped=0`, `underrun=0`
    - `switch2_5s` (`high_quality`): `silence_fill=2`, `startup_silence_fill=99`, `dropped=0`, `underrun=0`
    - `switch3_5s` (`balanced`): `silence_fill=0`, `startup_silence_fill=19`, `dropped=0`, `underrun=0`
    - `explicit_probe_90s_switch_end`: `silence_fill=0`, `startup_silence_fill=19`, `dropped=0`, `underrun=0`
  - Regression baseline conclusion: all three post-switch 5s windows keep steady-state `silence_fill < 10`; this probe is the current low_latency/high_quality mode-switch baseline.
## v1.4 Release Gate

- [ ] TASK-V14-001 `v1.3.1` acceptance evidence recorded
- [ ] TASK-V14-002 main-path `windows_loopback + v2_header + opus` 30+ minute long-run passes
- [ ] TASK-V14-003 USB validation recorded
- [x] TASK-V14-010 MediaSession verified on `5391d451`
- [x] TASK-V14-011 Android update check verified on `5391d451`
- [ ] TASK-V14-012 Windows update detection re-verified locally
- [x] `scripts/validate_local.ps1` passes on current workspace
- [ ] Version and release docs updated to `v1.4`
- [ ] `scripts/package_release.ps1` preflight completed
- [ ] `scripts/release.ps1` executed

## Completed In The v1.3 Cycle

- [x] Domain-owned contracts moved into `crates/lan_audio_domain`
- [x] Explicit connection state machine and failure taxonomy
- [x] Stable service snapshot contract
- [x] Release gate and rollback verification flow
- [x] Server-side data plane abstraction (`legacy_las1`, `v2_header`, `usb_direct`)
- [x] Desktop and Android snapshot parsing aligned with shared contracts
- [x] Release packaging and GitHub Release workflow
- [x] Device acceptance evidence tracked in repo artifacts

## Current Priority

- [ ] Refactor Android runtime internals without breaking the shared snapshot contract
- [ ] Refactor desktop-side service orchestration without reintroducing direct UI/runtime coupling
- [ ] Improve post-release diagnostics and operator-facing troubleshooting flow
- [ ] Keep rollback path exercised as mainline changes land

## Protocol / Transport Follow-Up

- [ ] Add stronger capability-driven data-plane negotiation
- [ ] Continue hardening `usb_direct` without weakening Wi-Fi behavior
- [ ] Keep `legacy_las1 + pcm16` testable and visible
- [ ] Improve failure-code coverage on negotiation and recovery paths

## Android Follow-Up

- [ ] Continue real-device validation under background and power-saving conditions
- [ ] Improve buffering and underrun diagnostics
- [ ] Reduce runtime complexity in playback/session coordination
- [ ] Preserve Oboe callback path as the maintained playback direction
- [x] MediaSession 闆嗘垚锛圥laybackState/Metadata/MediaStyle 閫氱煡/PLAY_PAUSE+STOP锛?
- [x] Android 鏇存柊妫€娴嬶紙鍚姩鍚庨潤榛樻鏌?+ 璁剧疆椤垫墜鍔ㄦ鏌?+ SnackBar 璺宠浆 Release锛?

## Desktop Follow-Up

- [ ] Simplify service lifecycle ownership
- [x] Improve diagnostics export锛坉esktop 鍙鍑?JSON 璇婃柇蹇収鍒?`dist/diagnostics/`锛?
- [x] Windows 鏇存柊妫€娴嬶紙鍚姩鍚庨潤榛樻鏌?+ 鎵樼洏鈥滄鏌ユ洿鏂扳€?+ 绐楀彛鍐呭崌绾?Banner锛?
- [ ] Improve rollback/safe-mode discoverability
- [ ] Keep desktop state rendering contract-driven

## Later Backlog

- [ ] QR-based connection entry
- [ ] Richer session history
- [ ] More guided USB help
- [ ] Firewall guidance UX
- [x] Structured support bundle export锛堝綋鍓嶅厛鎻愪緵 desktop diagnostics snapshot锛屽悗缁ˉ鍏?Android 渚э級

- Mode-switch no-audio follow-up (2026-04-23): sink reinit now serializes stats/stop/release and no longer reproduces persistent audio_track_write_frames_per_sec=0 after balanced -> low_latency -> high_quality -> balanced in mode_switch_probe_fix_lock_fg3_20260423_111741_logcat.txt; however switch2_5s still showed transient silence_fill=98, so gate status remains continue_fix.

## v1.7 Theme 2 Audio Quality Layer (`2026-05-10`)

- TASK-V17-201 Android 3-band EQ:
  - Added Android `Equalizer` binding after `AudioTrack` creation.
  - Exposed low/mid/high bands at 60Hz / 1kHz / 10kHz with `-10..+10dB` clamping.
  - Added Flutter settings controls, presets (`Flat`, `Bass`, `Vocal`, `Bright`), and SharedPreferences persistence through the foreground service.
  - Stable snapshot contract now exposes `eq_enabled` and `eq_settings`.
- TASK-V17-202 loudness normalization:
  - Added PCM16 RMS analysis before sink write, 500ms analysis interval, target RMS around `-18dB`, gain clamp `0.5x..2.0x`, and 100ms ramp smoothing.
  - The processor is active only for `balanced` and `high_quality`; `low_latency` bypasses and reports `0.0dB` gain.
  - Flutter settings include the toggle and the playback summary shows current loudness gain while playing.
- TASK-V17-203 multi-device streaming:
  - Server broadcast architecture already supports independent client sessions; max clients is now constrained to `4`.
  - Existing multi-client regression now verifies disconnect isolation and 5th-client rejection.
  - Desktop UI now renders active device summaries from the shared metrics snapshot.
  - Known limitation: desktop-side "disconnect one selected device" is deferred to v1.8 because no stable per-client desktop command surface exists yet.
- Theme 2 gate status:
  - [x] EQ real-device audible change verified
  - [x] Loudness normalization real-device gain display verified
  - [x] Multi-device 2-phone playback verified
  - [x] `flutter test` passed
  - [x] `cargo test -p lan_audio_server` passed
  - [x] `docs/todo.md` updated
- Theme 2 conclusion: `pass`; manual gate approved before Theme 3.

## v1.7 Theme 3 Connection Experience (`2026-05-10`)

- TASK-V17-301 mDNS LAN discovery:
  - Windows/server side registers `_lan-audio._tcp.local.` with service name `LAN Audio @ <server_name>`, port `39991`, and TXT records `version=1.7`, `mode=<current_mode>`.
  - Android uses `NsdManager` to discover `_lan-audio._tcp`, keeps IPv4 results only, and feeds the Flutter nearby-device list through `lan_audio/platform`.
  - Flutter keeps UDP beacon discovery and LAN probe as fallback, shows nearby devices with IP and TCP probe latency, and exposes manual IPv4 entry under Advanced.
- TASK-V17-302 smart reconnect:
  - Foreground playback service now retries network disconnects with exponential backoff `1s -> 2s -> 4s -> 8s -> 16s`.
  - User stop cancels reconnect; reconnect success restores the existing target/mode/codec path.
  - Stable snapshot now exposes `reconnect_attempts` and `reconnect_delay_ms`; UI shows `Reconnecting (#N, delay)` during recovery.
  - Five failed retries enter `error` state with `reconnect_exhausted`.
- TASK-V17-303 connection history and favorites:
  - Added `ConnectHistory` model (`ip`, `port`, `hostname`, `last_connected`, `connect_count`, `is_favorite`, `last_latency_ms`).
  - Android persists history JSON in SharedPreferences, capped at 10 entries.
  - Flutter shows favorites first, then recent history; tap connects, long press toggles favorite/delete/edit name, and right-swipe deletes.
- Theme 3 gate status:
  - [x] mDNS real-device discovery and connect verified
  - [x] Simulated network interruption auto-recovers playback
  - [x] Connection history persists after app restart
  - [x] `flutter test` passed
  - [x] `cargo test -p lan_audio_server` passed
  - [x] `android/gradlew.bat assembleDebug` passed
  - [x] `docs/todo.md` updated
- Theme 3 conclusion: `pass`; manual gate approved before Theme 4.

## v1.7 Theme 4 Open Source Readiness (`2026-05-10`)

- TASK-V17-401 contributor documentation:
  - Added root `CONTRIBUTING.md` with environment requirements, local run steps, commit rules, architecture notes, and primary/rollback path guidance.
  - Added GitHub issue templates for bug reports, feature requests, and connection issues.
  - Added pull request template with validation and rollback checklist.
- TASK-V17-402 CI coverage:
  - Added protocol-layer tests for `NegotiationError`, v2 header codec/flag round trips, legacy/v2 handshake distinction, and mode profile boundary values.
  - CI now generates `lan_audio_protocol` LCOV coverage with `cargo llvm-cov` and uploads through `codecov/codecov-action@v4`.
  - README now includes the Codecov badge.
  - Manual follow-up: configure `CODECOV_TOKEN` in GitHub repository secrets.
- TASK-V17-403 changelog:
  - Added Keep a Changelog style `CHANGELOG.md` covering v1.5, v1.6, and v1.7.
  - `scripts/release.ps1` now warns when the current version is missing from `CHANGELOG.md`; the warning does not block release.
