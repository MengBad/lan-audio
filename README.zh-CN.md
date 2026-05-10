[English](./README.md) | [简体中文](./README.zh-CN.md)

# LAN Audio

把 Windows 电脑的声音通过局域网传到 Android 手机，让手机充当无线音响。

LAN Audio 是一个 Windows 到 Android 的实时音频串流项目。仓库由 Rust 流媒体后端、Flutter Android 客户端和 Tauri Windows 桌面端组成，同时维护 Protocol v2 主路径与显式回滚路径。

## 项目简介

项目目标很直接：Windows PC 播放音频，通过 LAN 或 USB 辅助网络传输到 Android 设备，再由手机扬声器播放。当前仓库仍偏向开发、测试和受控发布，不是面向普通用户的一键安装成品。

## 当前状态

- 当前版本：`1.5`。
- 仓库包含 Rust LAN 服务端、Android Flutter 客户端和 Windows Tauri 桌面端。
- 推荐主路径：`windows_loopback + v2_header + opus`。
- 永久维护回滚路径：`legacy_las1 + pcm16`。
- `v1.5` 是人工批准的 FORCE_RELEASE 构建，保持协议主路径稳定，同时交付 Audio Console Dark UI、latency probe 验收、Android/Windows 更新检测、诊断导出、缓冲策略优化和工具链升级。
- Android 与 Windows UI 采用 Audio Console Dark 设计，字体为 DM Sans + IBM Plex Mono。
- Android 已集成 MediaSession，包含播放状态、metadata、MediaStyle 控件、play/pause 和 stop。
- Android / Windows 均支持静默更新检测，并提供手动入口跳转 GitHub Releases。
- Desktop 支持结构化 support bundle 导出，Android 支持诊断包 zip 导出并调用系统分享面板。
- Desktop 主界面显示连接 QR 码，Android 支持扫码解析 `lan-audio://<ip>:<port>` 并自动连接。
- Android 与 Desktop 均提供 ConnectionRefused / Timeout 防火墙排查引导。
- balanced 模式播放缓冲策略已优化，latency probe 通过结构化脚本导出三档实测结果。
- 本地验证、打包和发布脚本已经纳入仓库。
- 当前后续工作集中在稳定性、延迟优化、模式策略、Protocol v2 演进和产品化体验。

## 快速开始

1. 检查本地工具链：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1
```

2. 启动 Windows 发送端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource windows_loopback
```

如需使用合成测试音源：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic
```

3. 启动 Android 客户端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

4. 在 Android app 中发现服务端、手动输入地址，或扫描桌面端 QR 码后连接并开始播放。

执行完整本地验证：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

导出 latency probe 结构化结果：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\export_latency_probe.ps1 -SnapshotPath .\dist\diagnostics\*.json
```

## 工作方式

```text
Windows audio capture (windows_loopback / synthetic)
    -> Rust LAN server
    -> WebSocket control + UDP audio
       or localhost TCP audio over adb reverse in USB mode
    -> Android playback service and client UI
    -> jitter buffer / native playback output
    -> phone speaker
```

## 仓库结构

```text
apps/
  android_flutter/   Android 客户端（Flutter UI + 原生播放桥接）
  desktop/           Windows 桌面应用（Tauri）

crates/
  lan_audio_domain/   共享运行时与发布契约
  lan_audio_protocol/ 协议消息与数据包格式
  lan_audio_server/   音频采集、传输、会话和指标运行时

docs/               架构、协议、开发、UI、路线图与发布文档
scripts/            环境检查、本地运行、验证、打包和发布脚本
artifacts/release/  发布门控与设备验收工件
```

## 开发说明

这是一个多组件仓库：Rust 后端 crate、Flutter Android app、Tauri 桌面前端都在同一工作区。修改运行时、发布逻辑或 UI 前，建议先阅读下方文档，并运行 `scripts/check_env.ps1` 与 `scripts/validate_local.ps1` 确认工具链状态。

## 文档入口

- [架构说明](docs/architecture.md)
- [开发环境](docs/dev_setup.md)
- [协议说明](docs/protocol.md)
- [Protocol v2 迁移](docs/protocol_v2_migration.md)
- [桌面端 UI 说明](docs/desktop_ui.md)
- [已知问题](docs/known_issues.md)
- [TODO / 状态](docs/todo.md)
- [路线图](docs/roadmap.md)
- [发布策略](docs/RELEASE_POLICY.md)
- [Android 视觉回归](docs/android_visual_regression.md)

## 路线图

- 持续提升推荐 Windows 到 Android 主路径的播放稳定性。
- 降低并控制端到端延迟。
- 保持 `low_latency`、`balanced`、`high_quality` 三档策略在桌面端与 Android 端一致。
- 继续演进 Protocol v2，同时永久保留显式回滚路径。
- 产品化桌面端与 Android 体验，包括 QR 连接、诊断包导出、回滚模式和防火墙引导。

## 许可证

仓库根目录当前没有 `LICENSE` 文件。
