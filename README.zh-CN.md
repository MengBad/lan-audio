<p align="center">
  <h1 align="center">LAN Audio</h1>
  <p align="center">
    把 Windows 电脑的声音实时传到 Android 手机上播放，手机变无线音响。
  </p>
</p>

<p align="center">
  <a href="https://github.com/MengBad/lan-audio/releases"><img alt="Release" src="https://img.shields.io/github/v/release/MengBad/lan-audio?color=6366f1" /></a>
  <a href="https://github.com/MengBad/lan-audio/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/MengBad/lan-audio?color=22c55e" /></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%7C%20Android-8b5cf6" />
</p>

<p align="center">
  <a href="#快速开始">快速开始</a> &nbsp;|&nbsp;
  <a href="https://github.com/MengBad/lan-audio/releases">下载</a> &nbsp;|&nbsp;
  <a href="CHANGELOG.md">更新日志</a> &nbsp;|&nbsp;
  <a href="README.md">English</a>
</p>

---

## 简介

LAN Audio 通过 WASAPI Loopback 捕获 Windows 系统音频，实时传输到同一局域网内的 Android 设备上播放。支持 Wi-Fi 和 USB（adb）两种连接方式，mDNS 自动发现设备。

**使用场景：**
- 电脑音响不行，手机/平板扬声器还不错
- 躺床上想听电脑的声音
- 临时需要个无线音响，不想买硬件

## 特性

### 音频管线
- **Opus 编码** — 48kHz VBR，非 48kHz 源自动 sinc 重采样
- **PCM16 无损** — 无压缩回退路径，最大兼容性
- **三种播放模式** — 低延迟（~64ms p95）/ 均衡 / 高质量
- **原生 3 段 EQ** — Oboe 路径上的 biquad 峰值滤波器（60Hz / 1kHz / 10kHz）+ 预设
- **响度归一化** — 均衡/高质量模式下自动增益控制
- **自适应运行时** — 服务端 CPU + 队列压力看门狗，压力大时自动降级编码

### 连接
- **自动发现** — mDNS 局域网扫描，不用手动输 IP
- **USB 模式** — 数据线直连（通过 adb），更稳定
- **断线重连** — 指数退避自动恢复
- **多设备** — 最多 4 台手机同时收听
- **PC 控音量** — 托盘菜单直接调手机音量

### Android 客户端
- **编码器选择** — UI 上可选 Auto / Opus / PCM 16
- **延迟图表** — 平滑实时曲线 + 虚线基线参考 + 右上角实时 ms 数值
- **音质指示条** — 显示协商的编码器、采样率、声道数
- **反向麦克风** — 手机麦克风传回电脑（端口 7878）
- **后台播放** — ForegroundService + MediaSession，锁屏后继续播放

### Windows 桌面端
- **系统托盘** — 左键点击显示窗口，右键快捷菜单（音量、更新、退出）
- **紧凑 UI** — Audio Console Dark 主题，所有控件一屏可见
- **一键推流** — 启动/停止 + 二维码供手机扫码连接
- **安全模式** — 一键回滚到兼容路径用于排障

## 快速开始

### 下载

**Android：** 从 [Releases](https://github.com/MengBad/lan-audio/releases) 下载 APK。大多数手机选 `arm64-v8a`。

**Windows：** 从 Releases 下载 exe，或从源码编译：

```powershell
git clone https://github.com/MengBad/lan-audio.git
cd lan-audio
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

### 使用方法

1. 电脑和手机连同一个 Wi-Fi
2. 电脑启动 LAN Audio
3. 手机打开 App，自动发现电脑
4. 点击连接，开始播放

**USB 模式**（更稳定，不需要 Wi-Fi）：
```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --transport usb --adb-serial <设备序列号> --audio-source windows_loopback
```

## 技术栈

| 组件 | 技术 |
| :--- | :--- |
| 音频采集 | WASAPI Loopback (Windows) |
| 桌面 GUI | Tauri 2 + Rust |
| 传输协议 | UDP 数据 + WebSocket 控制，Protocol v2/v3 |
| 编码 | Opus 48kHz VBR, PCM16 |
| Android 客户端 | Flutter + Kotlin |
| 音频输出 | Oboe (NDK), AudioTrack 回退 |
| 设备发现 | mDNS |

## 项目结构

```
apps/
  android_flutter/       Android 客户端 (Flutter + Kotlin Native)
  desktop/               Windows 桌面端 (Tauri + Rust)

crates/
  lan_audio_protocol/    协议定义与数据包解析
  lan_audio_server/      音频采集、编码、传输、自适应运行时
  lan_audio_domain/      共享类型与常量

docs/                    协议规范、架构设计、设计文档
scripts/                 构建、验证与发布脚本
```

## 开发

### 环境要求

- Rust 1.75+
- Flutter 3.x
- Android SDK + NDK（Oboe 需要）
- Windows 10+

### 构建与测试

```powershell
# 一键验证（格式检查 + 编译 + 测试）
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1

# 或手动执行
cargo fmt --all -- --check
cargo check
cargo test -p lan_audio_protocol -p lan_audio_server
```

### 构建 Release

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

## 文档

| 文档 | 说明 |
| :--- | :--- |
| [协议规范](docs/protocol.md) | 数据包格式、协商流程 |
| [协议 v2 迁移](docs/protocol_v2_migration.md) | v1 到 v2 迁移指南 |
| [架构设计](docs/architecture.md) | 系统架构概览 |
| [桌面端 UI 设计](docs/desktop_ui.md) | 桌面客户端 UI 规范 |
| [开发环境搭建](docs/dev_setup.md) | 开发环境配置指南 |
| [已知问题](docs/known_issues.md) | 已知 Bug 与限制 |
| [路线图](docs/roadmap.md) | 功能路线图 |
| [发布策略](docs/RELEASE_POLICY.md) | 版本号与发布流程 |

> 英文文档请参阅 [README.md](README.md)

## 协议

[MIT](LICENSE)
