# Architecture (MVP)

## Goals

最小链路：Windows 桌面端 -> Android 客户端，完成发现、会话、UDP 数据收发与调试统计，并实现 Android 真播放。

## Modules

- `lan_audio_protocol`
  - 控制协议（WebSocket JSON）
  - 数据协议（UDP 二进制帧）
- `lan_audio_server`
  - `audio_capture`: 可插拔音频输入（synthetic / windows_loopback）
  - `audio_capture::pcm_accumulator`: packet -> fixed 10ms frame
  - `discovery` / `session` / `transport` / `metrics`
- `apps/android_flutter`
  - Flutter UI（发现、连接输入、状态展示）
  - 旧链路（legacy）：Dart 前台实时链路（默认保留，迁移兜底）
  - 新链路（v25）：`PlaybackForegroundService`（Media3 `MediaSessionService`）后台承载 WS/UDP/jitter/AudioTrack
  - Flutter <-> Native 通信：
    - `MethodChannel('lan_audio/playback_service')`
    - `EventChannel('lan_audio/playback_events')`

## End-to-End Path

1. Windows loopback 采集 PCM（10ms frame 输出）。
2. 服务端 passthrough 打包 UDP payload（PCM16）。
3. Android 收包并入 jitter buffer。
4. playout 线程从 jitter buffer 取帧写入 AudioTrack。

## Current Truth

- 已实现代码路径：Android AudioTrack 真播放链路。
- v26 已将后台播放服务切为默认路径（`kUseBackgroundPlaybackService=true`），legacy 路径仍保留用于回退调试。
- v26 新增 Android `PARTIAL_WAKE_LOCK + WifiLock` 保活基础能力（服务启动时获取、停止时释放）。
- 未完成项：Opus 真机稳定性验收、复杂重采样、自适应 jitter、多设备同步。
- 未在当前提交环境完成真实机型回放验收（需按 README 步骤实测）。
