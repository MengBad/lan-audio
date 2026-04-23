[English](./README.md) | [简体中文](./README.zh-CN.md)

# LAN Audio
把 Windows 电脑的声音通过局域网传到 Android 手机，让手机充当无线音响。

LAN Audio 是一个面向 Windows 到 Android 音频传输场景的多组件项目，目标是在局域网内把电脑正在播放的声音实时推送到手机播放。仓库当前由 Rust 流媒体后端、Flutter Android 客户端和 Tauri 桌面端组成，并同时维护 Protocol v2 主路径与显式回滚路径。

## 项目简介

这个项目要解决的问题很直接：电脑正常播放声音时，不依赖额外音频硬件，把声音通过局域网或 USB 辅助网络链路发到 Android 手机，让手机直接出声。当前仓库更偏向开发、测试和受控发布流程，还不是面向普通用户的一键安装成品。

## 当前状态

- 仓库中已经包含 Rust 服务端、Android Flutter 客户端和 Windows Tauri 桌面端。
- 当前文档推荐主路径是 `windows_loopback + v2_header + opus`。
- 当前长期保留的回滚路径是 `legacy_las1 + pcm16`。
- Android 和 Windows 两侧都已经有本地验证、打包和发版脚本。
- 当前持续推进的重点仍然是稳定性、延迟优化、模式策略、Protocol v2 演进，以及桌面/UI 产品化。
- 以现在的成熟度来看，这个项目更适合 Windows + Android 环境下的开发者和测试者使用。

## 快速开始

1. 先检查本地工具链：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1
```

2. 启动 Windows 发送端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource windows_loopback
```

如果只是做链路调试，也可以先用合成音源：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic
```

3. 启动 Android 客户端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

4. 在 Android 端发现或手动输入桌面端地址，连接后开始播放。

如果要执行完整的本地验证：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```

## 工作方式

```text
Windows 音频采集（windows_loopback / synthetic）
    -> Rust LAN 服务端
    -> WebSocket 控制面 + UDP 音频数据
       或 USB 模式下经 adb reverse 的 localhost TCP 音频数据
    -> Android 播放服务与客户端界面
    -> jitter buffer / 原生播放输出
    -> 手机扬声器
```

## 仓库结构

```text
apps/
  android_flutter/   Android 客户端（Flutter UI + 原生播放桥接）
  desktop/           Windows 桌面应用（Tauri）

crates/
  lan_audio_domain/   共享运行时与发布契约
  lan_audio_protocol/ 协议消息与数据包格式
  lan_audio_server/   音频采集、传输、会话与指标运行时

docs/               架构、协议、开发、UI、路线图与发布文档
scripts/            环境检查、本地运行、验证、打包与发版脚本
artifacts/release/  仓库内跟踪的发布门控与设备验收产物
```

## 开发说明

这是一个多组件仓库：既有 Rust 后端 crate，也有 Flutter Android 应用和 Tauri 桌面前端。建议先看下面列出的文档入口，再用 `scripts/check_env.ps1` 和 `scripts/validate_local.ps1` 确认工具链状态，再进入具体模块开发。

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

- 继续提升推荐主路径下的播放稳定性。
- 继续压低并稳定端到端延迟。
- 保持 `low_latency`、`balanced`、`high_quality` 三种模式在桌面端和 Android 端的行为一致。
- 继续演进 Protocol v2，同时不移除显式回滚路径。
- 逐步把桌面端和 Android 端做成更易用、可诊断的产品形态。

## 许可证

仓库根目录当前没有 `LICENSE` 文件。
