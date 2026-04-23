# Project Roadmap

## Current Position

- Latest shipped release: `v1.3.1`
- Default runtime path: `windows_loopback + v2_header + opus`
- Maintained rollback path: `legacy_las1 + pcm16`
- Supported transports: `wifi`, `usb`
- Shared contract source: `crates/lan_audio_domain`

`v1.3` closed the release gate and shipped the current core Windows-to-Android streaming path. The next cycle is not about changing the default path again. It is about making the existing path easier to maintain, easier to diagnose, and safer to evolve.

## Long-Term Product Goals

1. Stable Windows-to-Android playback
2. Lower and more predictable end-to-end latency
3. Mode-aware transport and playback strategy
4. Protocol v2 as the maintained mainline
5. Productized desktop and Android experience

## What Shipped In v1.3

- Release gate is now contract-driven through `artifacts/release/acceptance_gate.json`
- Mainline path is `windows_loopback + v2_header + opus`
- Rollback verification exists through `desktop_headless --force-rollback`
- Data plane routing exists on the server side:
  - `legacy_las1`
  - `v2_header`
  - `usb_direct`
- Android and desktop consume the shared service snapshot contract
- USB synthetic and Wi-Fi loopback device acceptance evidence is tracked in `artifacts/release/device_acceptance.json`
- GitHub release workflow produces:
  - split Android release APKs
  - Windows desktop `.exe`
  - `SHA256SUMS.txt`

## Post-v1.3 Priorities

### 1. Android Runtime Follow-Up

- Refactor the Android playback runtime around clearer data-plane boundaries
- Continue reducing underrun risk under real device background and power-saving conditions
- Improve diagnostics around buffering, reconnect, and sink behavior
- Keep Oboe callback playback as the maintained direction

### 2. Desktop Refactor

- Reduce coupling between desktop controls and service internals
- Keep the desktop UI driven by the stable service snapshot contract
- Improve diagnostics export and operator-facing troubleshooting flows
- Keep rollback visible and easy to trigger

### 3. Protocol And Data Plane Evolution

- Extend negotiation so runtime path selection is less config-driven and more capability-driven
- Keep `legacy_las1 + pcm16` available as a durable rollback path
- Evolve `usb_direct` without weakening the current Wi-Fi main path
- Avoid bypassing the domain-owned contracts

### 4. Productization

- Better onboarding for Wi-Fi and USB setup
- Clearer desktop status presentation
- Better firewall, network, and power-management guidance
- More structured release notes and diagnostics exports

## Next Release Gate Expectations

Before the next release train is considered:

- local validation must stay green
- rewrite validation must stay green
- rollback verification must stay green
- device acceptance evidence must be updated when behavior changes
- main-path and rollback-path documentation must stay aligned

## Not In Immediate Scope

These are explicitly not the next-step priority:

- phone-as-mic
- cloud relay
- broad multi-room playback productization
- replacing rollback with silent fallback behavior

## Working Rule

Roadmap changes should preserve these repo-wide truths:

- main path stays explicit
- rollback stays explicit
- failure paths keep explicit codes
- UI/runtime coupling should go through shared contracts
- release decisions come from gate artifacts, not prose
