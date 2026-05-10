---
name: Connection issue
about: Report discovery, reconnect, Wi-Fi, or USB connection problems
title: "fix: connection "
labels: connection
assignees: ""
---

## Connection Type

- Wi-Fi mDNS discovery
- Wi-Fi manual IP
- USB adb reverse
- USB tethering

## Symptoms

- Cannot discover the Windows sender
- Can discover but cannot connect
- Disconnects during playback
- Reconnect does not recover
- Other:

## Environment

- Windows IP address:
- Android device and Android version:
- Router/AP model if relevant:
- Same Wi-Fi / guest network / hotspot:
- VPN or firewall enabled:

## Log Collection

Android logcat:

```powershell
adb logcat -c
adb logcat -v time | findstr /i "lan_audio PlaybackForegroundService PlaybackSessionRuntime NsdManager"
```

Desktop/server logs:

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

## Checks

- Does manual IP connect work?
- Does mDNS discovery show the sender?
- Does rollback `legacy_las1 + pcm16` connect?
- Does restarting the app preserve connection history?
