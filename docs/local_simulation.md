# Local Simulation Guide

## Goal

验证 Windows loopback -> UDP PCM -> Android AudioTrack 能发声。

## Steps

1. Windows 端启动服务：

```bash
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

2. Android 端运行 Flutter App：

```bash
cd apps/android_flutter
flutter pub get
flutter run
```

3. App 内：

- 选择服务
- `Connect Selected`
- `Start Playback`

## Expected

- 状态显示 `buffering` 后进入 `playing`。
- `buffered ms`、`underrun` 等数值可见。
- 可听到来自 Windows 系统的声音。

## Debug

- 收不到包：检查同一局域网、防火墙、WS 连接。
- 缓冲不足：`underrun` 增长，先保证网络稳定。
- AudioTrack 初始化失败：查看 App `Audio log` 和 logcat。
