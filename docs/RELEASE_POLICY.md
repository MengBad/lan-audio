# Release Policy

## Current Release State

- Latest shipped release: `v1.3.1`
- Current tracked gate decision: `allow_release`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`

Release decisions are artifact-driven. The source of truth is:

- `artifacts/release/acceptance_gate.json`
- `artifacts/release/device_acceptance.json`

## Version Source

The single version source is the repository root `VERSION` file.

Rules:

- short version: `major.minor` or `major.minor.patch`
- git tag: `v<short version>`
- Rust/Tauri semver: for `major.minor` use `<major.minor>.0`; for `major.minor.patch` use the same three-part semver
- Android `versionCode`: `2000000 + major * 10000 + minor * 100 + patch`, where `patch` defaults to `0`

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
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version <major.minor or major.minor.patch>
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
- verified scope
- known limitations
- rollback method
