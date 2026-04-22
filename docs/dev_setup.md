# Local Development Setup

## Required Tools

- Rust stable (cargo, rustc)
- Flutter SDK
- Android SDK + real device (recommended)

## Commands

```bash
cargo check
cargo test
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback

cd apps/android_flutter
flutter pub get
flutter run
```

## Real-device validation (Windows + Android)

1. 在 Windows 端播放任意系统声音（浏览器音乐/视频）。
2. Android App 选择服务后点击 `Connect Selected`。
3. 点击 `Start Playback`。
4. 观察状态从 `buffering` 到 `playing`，并确认扬声器有声音。
5. 如需采集侧证据，可在桌面端加 `--capture-dump-wav`。

## Notes

- 若 Windows 防火墙阻止 UDP/WS，请放行 39990/39991/39992。
- 当前推荐 Android 播放链路为 `v2_header + opus + jitter buffer + AudioTrack`，`legacy_las1 + pcm16` 保留为回滚路径。
- `v2_header + opus` 已接入 Android 后台服务：使用 `libopus` JNI 解码并在失败时走 PLC concealment；`windows_loopback + v2_header + opus` 仍需单独补齐真机长稳验收。
- 当前仓库代码已接入真播放路径，但仍需你在真实设备上完成最终验收。
