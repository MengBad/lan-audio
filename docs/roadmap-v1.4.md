# v1.4 Roadmap / Post-Release Follow-Up

## Current State

- Latest shipped release before the hotfix tag: `v1.4`
- Current release target: `v1.4.1`
- Main path: `windows_loopback + v2_header + opus`
- Rollback path: `legacy_las1 + pcm16`
- Current phase: post-release regression fix and controlled follow-up

## Post-Release Fix Scope

- Restore Android Server Card discoverability for connection control.
- Move Android debug metrics, update check, and secondary actions behind the top-right advanced entry.
- Sample visible Android `buffer ms` at a 1 second UI cadence.
- Export and display real Android `rx fps` from the stable snapshot.
- Ensure mode-switch UI exits transient buffering once playback resumes or enters a real error/recoverable state.
- Refresh the Windows first screen toward Audio Console Dark without changing service orchestration.
- Require a stable Android release keystore for local and GitHub release APK builds.

## Release Position

This follow-up is being prepared as the `v1.4.1` hotfix release once local validation, packaging, signing configuration, and any required manual device checks are complete. The hotfix keeps the protocol and playback path unchanged from `v1.4`.
