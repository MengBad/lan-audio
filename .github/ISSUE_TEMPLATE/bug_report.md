---
name: Bug report
about: Report a reproducible LAN Audio bug
title: "fix: "
labels: bug
assignees: ""
---

## Summary

Describe what went wrong and what you expected to happen.

## Environment

- Windows version:
- Android device and Android version:
- LAN Audio version:
- Transport: Wi-Fi / USB
- Mode: low_latency / balanced / high_quality
- Codec/path shown in the app:

## Reproduction Steps

1.
2.
3.

## Logs And Diagnostics

Please attach a diagnostics bundle when possible:

- Desktop: export the diagnostics JSON from the desktop app or include files from `dist/diagnostics/`.
- Android: attach relevant logcat output.
- Release artifact used: APK filename or desktop executable filename.

## Impact

- Does the rollback path `legacy_las1 + pcm16` work?
- Does the issue reproduce with `synthetic` source?
- Does the issue reproduce after restarting both apps?
