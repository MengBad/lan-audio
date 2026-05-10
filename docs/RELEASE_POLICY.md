# Release Policy

## Current Release State

- Latest shipped release before the forced v1.5 tag: `v1.4.1`
- Current release target: `v1.5`
- Current tracked gate decision: `allow_release`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Release mode for `v1.4`: `FORCE_RELEASE=true` with human-override notes recorded in release-tracked docs and artifacts
- Release mode for `v1.4.1`: normal hotfix release after local packaging and GitHub Actions signing verification
- Release mode for `v1.5`: `FORCE_RELEASE=true`; long-run gate is human-overridden using the passed latency probe evidence (`low_latency=64ms`, `balanced=185ms`, `high_quality=505ms`)

Release decisions are artifact-driven. The source of truth is:

- `artifacts/release/acceptance_gate.json`
- `artifacts/release/device_acceptance.json`
- `artifacts/latency/latency_probe_latest.json` for structured latency revalidation

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

`package_release.ps1` normally requires the non-artifact gate fields to pass first. When `FORCE_RELEASE=true`, packaging still runs but gate enforcement is bypassed and the release-tracking artifacts are annotated for human override.

## Android Release Signing

Release APKs must be signed with a stable release keystore. Debug signing is only for debug builds and must not be used for GitHub Release artifacts.

Release APKs use a fixed keystore so Android can install later releases over earlier releases without requiring an uninstall. The keystore file must never be committed to git.

Local release packaging requires these environment variables:

- `LAN_AUDIO_KEYSTORE_PATH`
- `LAN_AUDIO_KEYSTORE_PASS`
- `LAN_AUDIO_KEY_ALIAS`
- `LAN_AUDIO_KEY_PASS`

GitHub Actions injects the keystore through the `LAN_AUDIO_KEYSTORE_B64` Repository Secret, decodes it as `lan-audio-release.jks`, then passes the four signing environment variables into the Android release build step. `package_release.ps1` warns when the variables are missing so local debug workflows are not blocked, but release APK signing is only considered valid when all four values are provided and point at the fixed release keystore.

Compatibility boundary: the shipped `v1.4` APKs were built through a path that used the build machine debug signing identity. If an installed APK was signed by a different debug key than the fixed release key, Android will require a one-time uninstall before installing `v1.4.1`. From `v1.4.1` onward, APKs are expected to support normal signed overwrite upgrades as long as the same keystore is retained.

## Android 签名

- Release APK 使用固定 keystore 签名（`lan-audio-release.jks`）。
- keystore 文件不进 git，`.gitignore` 已拦截 `*.jks`、`*.keystore` 和 `*.b64.txt`。
- GitHub Actions 通过 `LAN_AUDIO_KEYSTORE_B64` Repository Secret 注入 keystore，并在构建前解码。
- 本地发版需设置四个环境变量：`LAN_AUDIO_KEYSTORE_PATH` / `LAN_AUDIO_KEYSTORE_PASS` / `LAN_AUDIO_KEY_ALIAS` / `LAN_AUDIO_KEY_PASS`。
- keystore 有效期 100 年，alias: `lan-audio`。
- 丢失 keystore 后用户必须卸载重装，请妥善备份。

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

## FORCE_RELEASE Override

When `FORCE_RELEASE=true` is present in the release environment:

- release gate enforcement is bypassed in `scripts/assert_release_gate.ps1`, `scripts/package_release.ps1`, and `scripts/release.ps1`
- local validation still runs
- local packaging still runs
- release artifacts keep a `force_release_override` marker for workflow/release-note visibility
- incomplete checklist items should be documented as `[human-override]`, not silently treated as fully passed

`.github/workflows/release.yml` is intentionally tag-driven:

- `push.tags` is the normal publish path after `scripts/release.ps1`
- `workflow_dispatch` only accepts an existing tag and is meant for rebuild/re-publish scenarios
- manual dispatch can add release-note sections for:
  - fix summary
  - verified scope
  - known limitations
- the workflow must verify `VERSION` matches the requested tag before it publishes artifacts

## Conditions To Release

A standard release is allowed only when all of the following are true:

- `release_decision = allow_release`
- local validation has passed
- rewrite validation has passed
- device acceptance has passed
- rollback verification has passed
- latency probe/export has produced a current `artifacts/latency/latency_probe_latest.json`
- Android release APKs are present
- Windows release EXE is present
- there are no critical bugs
- there are no blocking failure codes

`FORCE_RELEASE=true` is the explicit exception path for an operator-approved release and does not change the requirement to keep the rollback path visible.

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
- latency probe measured values
- UI redesign summary, including Audio Console Dark
- human-override marker when FORCE_RELEASE is used
- verified scope
- known limitations
- rollback method
