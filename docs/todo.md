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

## Desktop Follow-Up

- [ ] Simplify service lifecycle ownership
- [ ] Improve diagnostics export
- [ ] Improve rollback/safe-mode discoverability
- [ ] Keep desktop state rendering contract-driven

## Later Backlog

- [ ] QR-based connection entry
- [ ] Richer session history
- [ ] More guided USB help
- [ ] Firewall guidance UX
- [ ] Structured support bundle export
