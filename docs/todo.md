# TODO / Status

## Release State

- Latest release: `v1.5`
- Current release target: `v1.6`
- Release mode: standard (`FORCE_RELEASE=false`)
- Release gate: `continue_fix`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Verified device: `5391d451` (`Xiaomi 24129PN74C`)
- Verified scenarios:
  - `USB + synthetic`
  - `WiFi + windows_loopback`
  - `v1.6 Android foreground install/metrics`: passed on `5391d451` (`2026-05-09`)
  - `v1.6 Android background/power matrix`: known_issue on `5391d451` (`2026-05-09`); foreground playback starts and reports metrics, but background/power scenarios can fall back to `buffering` with `rx_frames_per_sec=0.0`

## v1.6 Engineering Completion Plan

- [known_issue] TASK-V16-101 Android `PlaybackSessionController` split is partially extracted only. Completed safe extraction for `PlaybackBufferPolicy`, `PlaybackPacingEngine`, `PlaybackLatencyGuard`, and `PlaybackMetricsCollector`; the main controller remains 1847 lines, so the `<400` line coordination target needs a deeper follow-up that can move playout/decode/session state ownership without changing the Oboe callback path.
- [x] TASK-V16-102 Android foreground service lifecycle is now guarded by an explicit internal state machine: `IDLE -> CONNECTING -> PLAYING -> STOPPING -> IDLE`, with transient `ERROR -> IDLE`; `ACTION_START` is accepted only from `IDLE`, `ACTION_STOP` is a safe no-op in `IDLE`, MediaSession play/pause never starts playback, and `onTaskRemoved` routes through stop.
- [known_issue] TASK-V16-103 Android power/background real-device matrix executed on `5391d451` on 2026-05-09. Foreground install and connection passed on `windows_loopback + v2_header + opus` (`playback=playing`, `rx_frames_per_sec≈50`, `dropped_late_frames=0/0`). Background/power matrix remains incomplete: after screen-off/Home/battery-saver cycling, playback can remain in `buffering` with `rx_frames_per_sec=0.0`; force-stop/reopen can recover after restarting the PC sender, but the first reopen attempt stayed in `buffering` with `recent=start_ignored_state:playing`. Mitigation for now: stop/restart the PC sender and reconnect from Android after a force-stop or aggressive power-management event.

### v1.6 Phase 1 Gate (`2026-05-09`)

- [x] `flutter analyze` passed with no issues
- [x] `flutter test` passed
- [x] `android/gradlew.bat assembleDebug` passed
- [x] Real-device debug install and foreground metric snapshot verification passed on `5391d451`; local debug install required full uninstall because the previously installed package used a different signing key and higher versionCode
- [known_issue] Android power/background validation did not pass cleanly: Home/screen-off/battery-saver cycling can leave playback in `buffering` with `rx_frames_per_sec=0.0`; force-stop/reopen recovered only after restarting the PC sender
- [x] `docs/todo.md` updated with Phase 1 status

## v1.4 Validation Summary (`2026-04-24`)

- `scripts/validate_local.ps1` passed on the current Windows workspace before release.
- Android real-device verification on `5391d451` confirmed:
  - `MediaStyle` notification with `Play/Pause` and `Stop`
  - active `MediaSession` metadata (`title=LAN Audio`, `artist=10.0.0.185`)
  - `KEYCODE_MEDIA_STOP` tears down the foreground playback service
  - settings `Check Update` action can surface the manual “already up to date” hint
- Main-path long-run validation for `windows_loopback + v2_header + opus` still showed unresolved long-run sink-starvation / silence-fill accumulation, so the engineering conclusion remained `continue_fix` rather than clean release sign-off.
- Short balanced-mode probes improved substantially after the Android buffering follow-ups, but mode-switch no-audio risk and long-run stability follow-up still remain open.

## v1.4 Release Gate

- [human-override] TASK-V14-001 `v1.3.6` acceptance evidence recorded
- [x] TASK-V14-002 main-path `windows_loopback + v2_header + opus` latency probe completed on `5391d451`: low_latency p95 64ms / balanced p95 185ms / high_quality p95 505ms
- [human-override] TASK-V14-003 USB validation recorded
- [x] TASK-V14-010 MediaSession verified on `5391d451`
- [x] TASK-V14-011 Android update check verified on `5391d451`
- [human-override] TASK-V14-012 Windows update detection re-verified locally
- [x] `scripts/validate_local.ps1` passes on current workspace
- [x] Version and release docs updated to `v1.4`
- [x] `scripts/package_release.ps1` preflight completed
- [x] `scripts/release.ps1` executed

## v1.4.1 Hotfix Release Prep

- [x] Version metadata advanced to `1.4.1`
- [x] Android version metadata advanced to `versionName=1.4.1`, `versionCode=2010401`
- [x] Stable Android release signing verified locally and in GitHub Actions
- [x] Historical `v1.4` debug-key APK compatibility boundary documented
- [x] Android MediaSession integration completed (`PlaybackState`, metadata, `MediaStyle`, `PLAY_PAUSE`, `STOP`)
- [x] Android update detection completed (silent startup check, manual settings entry, Release page jump)
- [x] Windows update detection completed (silent startup check, tray manual check, in-window banner)
- [x] Desktop diagnostics export completed (`dist/diagnostics/` JSON snapshot export)
- [x] Android balanced buffering strategy optimized for the v1.4.1 follow-up
- [x] Mode-switch transient/concurrency recovery fixed so UI can return from `buffering` to `streaming`
- [x] Android `.hprof` heap dumps removed from `apps/android_flutter/`
- [x] `.hprof` and `tmp_test/` ignore rules recorded in root `.gitignore`
- [x] Android and Windows update checker repository paths corrected to `MengBad/lan-audio`

## v1.5 Forced Release (`2026-05-09`)

- [x] Version metadata advanced to `1.5`
- [human-override] Main-path long-run gate accepted by operator; latency probe is the replacement evidence
- [x] Latency probe passed on `5391d451`: low_latency p95 64ms / balanced p95 185ms / high_quality p95 505ms
- [x] Audio Console Dark UI redesign merged, including DM Sans + IBM Plex Mono typography
- [x] Android MediaSession integration shipped
- [x] Android and Windows update detection shipped
- [x] Desktop diagnostics snapshot export shipped
- [x] Android balanced buffering strategy shipped
- [x] Mode-switch transient/concurrency recovery shipped
- [x] Android toolchain upgraded to AGP 8.7.3 and Kotlin 2.1.0
- [x] Local validation passed before release
- [x] Release notes must include `human-override`, main path `windows_loopback + v2_header + opus`, rollback path `legacy_las1 + pcm16`, latency probe values, and UI redesign summary

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

- [x] Post-`v1.4` regression pass: Android Audio Console restores discoverable Server Card connection controls, moves debug/update actions behind the top-right advanced entry, throttles visible `buffer ms` to a 1s UI cadence, restores `rx fps` from the stable snapshot, and fixes mode-switch UI recovery from `buffering` back to `streaming`.
- [x] Post-`v1.4` release-flow fix: Android release APK signing no longer uses the per-machine debug keystore; release builds now require a stable release keystore locally and in GitHub Actions.
- [x] Windows desktop first screen refreshed to the Audio Console Dark structure while keeping the existing service controls and rollback path visible.
- [x] Latency revalidation is systematized through `scripts/export_latency_probe.ps1`; it exports per-mode `low_latency / balanced / high_quality` latency proxy results to `artifacts/latency/latency_probe_latest.json`.
- [known_issue] Refactor Android runtime internals without breaking the shared snapshot contract (v1.6 partial extraction landed; full controller split still pending)
- [ ] Refactor desktop-side service orchestration without reintroducing direct UI/runtime coupling
- [ ] Improve post-release diagnostics and operator-facing troubleshooting flow
- [ ] Keep rollback path exercised as mainline changes land

## Protocol / Transport Follow-Up

- [ ] Add stronger capability-driven data-plane negotiation
- [ ] Continue hardening `usb_direct` without weakening Wi-Fi behavior
- [ ] Keep `legacy_las1 + pcm16` testable and visible
- [ ] Improve failure-code coverage on negotiation and recovery paths

## Android Follow-Up

- [known_issue] Continue real-device validation under background and power-saving conditions (2026-05-09 `5391d451`: foreground metrics passed; background/power cycling can stall at `buffering` with `rx_frames_per_sec=0.0`)
- [ ] Improve buffering and underrun diagnostics
- [known_issue] Reduce runtime complexity in playback/session coordination (policy/pacing/guard/metrics helpers extracted; controller still needs deeper split)
- [ ] Preserve Oboe callback path as the maintained playback direction
- [x] MediaSession integration (`PlaybackState`, metadata, `MediaStyle`, `PLAY_PAUSE`, `STOP`)
- [x] Android update detection (silent startup check + manual settings entry + SnackBar jump to Release page)

## Desktop Follow-Up

- [ ] Simplify service lifecycle ownership
- [x] Improve diagnostics export (`dist/diagnostics/` desktop JSON snapshot export)
- [x] Structured latency probe/export (`artifacts/latency/` JSON artifact from diagnostics snapshots)
- [x] Windows update detection (silent startup check + tray manual check + in-window banner)
- [ ] Improve rollback / safe-mode discoverability
- [ ] Keep desktop state rendering contract-driven

## Later Backlog

- [x] Collect real-device latency probe samples for `low_latency / balanced / high_quality` before the next standard release sign-off: low_latency 64ms / balanced 185ms / high_quality 505ms
- [ ] Android runtime refactor without breaking the shared snapshot contract
- [ ] Desktop service orchestration refactor without reintroducing direct UI/runtime coupling
- [ ] QR-based connection entry
- [ ] Richer session history
- [ ] More guided USB help
- [ ] Firewall guidance UX
- [x] Structured support bundle export (desktop diagnostics snapshot first; Android-side bundle still pending)

## v1.4 FORCE_RELEASE Notes

- Release mode: `FORCE_RELEASE=true`
- Release target: `v1.4`
- Human-confirmed release content:
  - [x] Android MediaSession integration
  - [x] Android / Windows update detection
  - [x] Desktop diagnostics snapshot export
  - [x] Android balanced buffering follow-up
  - [x] Desktop non-blocking service-start / USB refresh follow-up
- Release gate checklist (forced override):
  - [human-override] Long-run main-path stability gate
  - [human-override] Full release checklist re-review
  - [human-override] Remaining manual regression evidence backfill
