# LAN Audio

[![codecov](https://codecov.io/gh/MengBad/lan-audio/branch/main/graph/badge.svg)](https://codecov.io/gh/MengBad/lan-audio)

LAN Audio 可以把 Windows 电脑变成音频发送端，把 Android 手机变成局域网音响。

项目由 Rust 服务端、Flutter Android 客户端和 Tauri 桌面端组成，默认主路径为 `windows_loopback + v2_header + opus`，并永久保留 `legacy_las1 + pcm16` 回滚路径。

## 当前版本

- Current version: `1.8`
- 最新发布：`v1.8`
- 主路径：`windows_loopback + v2_header + opus`
- 回滚路径：`legacy_las1 + pcm16`
- 传输：Wi-Fi / USB direct
- 模式：`low_latency` / `balanced` / `high_quality`

## 功能

- Windows 系统声音实时推送到 Android 手机播放。
- 支持 `synthetic` 测试音源，用于诊断和验收。
- 支持局域网连接和 USB `adb reverse` 连接。
- 支持低延迟、均衡、高音质三种播放策略。
- Protocol v2 + Opus 作为主路径，同时保留 legacy PCM16 回滚路径。
- Android 前台播放通知集成 MediaSession。
- 省电模式后台保活引导，覆盖小米、华为和通用 Android 路径。
- EQ 均衡器（3 段，Android），含预设和持久化。
- 响度归一化，播放时显示当前增益。
- 多设备同时推流，最多 4 台 Android 设备。
- mDNS 局域网设备自动发现，无需手动输入 IP。
- 智能断网重连，使用指数退避。
- 连接历史与收藏，常用设备可一键连接。
- Android 麦克风 → PC 反向音频通道，Opus 编码，Windows 命名管道输出。
- 实时抖动可视化（Audio Console Dark 内嵌折线图，三段着色）。
- PC 端控制 Android 音量，桌面托盘快捷预设，手机端音量浮层提示。
- 贡献者文档、Issue 模板、PR 模板、CHANGELOG 和 Codecov 覆盖率报告已补齐。

## v1.8 验收状态

- Release gate：`allow_release`
- FORCE_RELEASE：`false`
- 验证设备：`5391d451`（`Xiaomi 24129PN74C`）
- 验证场景：`USB direct`、`WiFi + windows_loopback`、`2 Android clients`
- latency probe：`low_latency p95=64ms`、`balanced p95=185ms`、`high_quality p95=505ms`
- known_issue：Desktop 单独断开某设备延后到后续版本。

## 本地验证

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

## 打包

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

发布产物：

- `lan-audio-android-arm64-v8a-v1.8.apk`
- `lan-audio-android-armeabi-v7a-v1.8.apk`
- `lan-audio-android-x86_64-v1.8.apk`
- `lan-audio-desktop-v1.8.exe`
- `SHA256SUMS.txt`

## 发布

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version 1.8
```

APK 使用固定 keystore 签名，支持覆盖安装。
