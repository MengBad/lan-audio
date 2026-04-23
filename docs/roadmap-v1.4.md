# v1.4 Roadmap / Post-Release Follow-Up

## Current State

- Latest shipped release: `v1.4`
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

This follow-up does not trigger a new release by itself. The repository should remain in `continue_fixing / controlled validation` until local validation, packaging, signing configuration, and any required manual device checks are complete.
