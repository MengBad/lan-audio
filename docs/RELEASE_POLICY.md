# Release Policy

## Current Release State

- Latest shipped release: `v1.7.1`
- Current tracked gate decision: `allow_release`
- FORCE_RELEASE: `false`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`

Release decisions are artifact-driven. The source of truth is:

- `artifacts/release/acceptance_gate.json`
- `artifacts/release/device_acceptance.json`

## Version Source

The single version source is the repository root `VERSION` file.

Rules:

- short version: `major.minor` or `major.minor.patch`
- git tag: `v<major.minor>` or `v<major.minor.patch>`
- Rust/Tauri semver: `<major.minor>.0` or `<major.minor.patch>`
- Android `versionCode`: `2000 + major * 1000 + minor * 10 + patch` (`patch` defaults to `0`)

## Required Local Validation

Use:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

This runs:

1. `cargo fmt --all -- --check`
2. `cargo check`
3. `cargo test -p lan_audio_protocol -p lan_audio_server`
4. `cargo check -p lan_audio_desktop`
5. `flutter analyze`
6. `flutter test`
7. `android/gradlew.bat assembleDebug`

## Packaging

Use:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

Outputs:

- `dist/release/android/`
- `dist/release/windows/`
- `dist/release/SHA256SUMS.txt`

`package_release.ps1` is allowed to build artifacts only when the non-artifact gate fields already pass. After a successful package run it updates local artifact-presence fields in the tracked gate file.

## Release Entry

Use:

```powershell
 powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version <major.minor-or-patch>
```

The release flow:

1. asserts the release gate
2. validates the local workspace
3. bumps version metadata
4. packages local release artifacts
5. commits release-tracked files
6. creates the git tag
7. pushes branch and tag
8. lets GitHub Actions publish the GitHub Release

`.github/workflows/release.yml` is intentionally tag-driven:

- `push.tags` is the normal publish path after `scripts/release.ps1`
- `workflow_dispatch` only accepts an existing tag and is meant for rebuild/re-publish scenarios
- manual dispatch can add release-note sections for:
  - fix summary
  - verified scope
  - known limitations
- the workflow must verify `VERSION` matches the requested tag before it publishes artifacts

## Conditions To Release

A release is allowed only when all of the following are true:

- `release_decision = allow_release`
- local validation has passed
- rewrite validation has passed
- device acceptance has passed
- rollback verification has passed
- Android release APKs are present
- Windows release EXE is present
- there are no critical bugs
- there are no blocking failure codes

## v1.7 Release Record

`v1.7` is a standard release, not a FORCE_RELEASE.

- Date: `2026-05-10`
- Theme 1 gate: passed
- Theme 2 gate: passed
- Theme 3 gate: passed
- Theme 4 gate: passed
- Latency probe: `low_latency p95=64ms`, `balanced p95=185ms`, `high_quality p95=505ms`
- Known issue: desktop per-device disconnect command is deferred to v1.8.
- Required release assets:
  - `lan-audio-android-arm64-v8a-v1.7.apk`
  - `lan-audio-android-armeabi-v7a-v1.7.apk`
  - `lan-audio-android-x86_64-v1.7.apk`
  - `lan-audio-desktop-v1.7.exe`
  - `SHA256SUMS.txt`

## v1.7.1 Patch Release Record

`v1.7.1` is a standard patch release for the v1.7 line, not a FORCE_RELEASE.

- Date: `2026-05-11`
- Scope: update-check correctness and release pipeline hardening.
- Fixed update checker GitHub API owner/repo: `MengBad/lan-audio`.
- Android update checks run on `Dispatchers.IO` and return results to `Dispatchers.Main`.
- Desktop manual update checks use the asynchronous/non-blocking path.
- Android release signing in GitHub Actions uses the fixed keystore-backed `local.properties` contract.
- Required release assets:
  - `lan-audio-android-arm64-v8a-v1.7.1.apk`
  - `lan-audio-android-armeabi-v7a-v1.7.1.apk`
  - `lan-audio-android-x86_64-v1.7.1.apk`
  - `lan-audio-desktop-v1.7.1.exe`
  - `SHA256SUMS.txt`

## Rollback Rule

The release process must not weaken or hide the rollback path.

Maintained rollback path:

- `legacy_las1 + pcm16`

Rollback verification depends on:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --force-rollback
```

This must produce evidence showing:

- `active_data_plane = legacy_las1`
- `codec = pcm16`
- `rollback_state = active`

## Release Notes Minimum Content

Release notes should include:

- current Protocol v2 status
- default main path
- rollback path
- latency probe values
- v1.7 feature list
- verified scope
- known limitations
- APK signing note
- rollback method
