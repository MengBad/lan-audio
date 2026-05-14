<p align="center">
  <h1 align="center">LAN Audio</h1>
  <p align="center">
    把 Windows 电脑变成音频发送端，把 Android 手机变成局域网音响 — 低延迟，支持 Wi-Fi 和 USB 两种传输方式。
  </p>
</p>

<p align="center">
  <a href="https://github.com/MengBad/lan-audio/releases"><img alt="Release" src="https://img.shields.io/github/v/release/MengBad/lan-audio?color=6366f1" /></a>
  <a href="https://github.com/MengBad/lan-audio/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/MengBad/lan-audio?color=22c55e" /></a>
  <a href="https://codecov.io/gh/MengBad/lan-audio"><img alt="Coverage" src="https://codecov.io/gh/MengBad/lan-audio/branch/main/graph/badge.svg" /></a>
  <a href="https://github.com/MengBad/lan-audio/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/MengBad/lan-audio?style=flat" /></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%7C%20Android-8b5cf6" />
</p>

<p align="center">
  <a href="README.md">English</a> &nbsp;|&nbsp;
  <a href="CHANGELOG.md">更新日志</a> &nbsp;|&nbsp;
  <a href="CONTRIBUTING.md">贡献指南</a>
</p>

---

## 这是什么？

LAN Audio 能将 Windows 系统音频实时传输到一台或多台 Android 手机上播放，通过局域网（Wi-Fi）或 USB 数据线连接。你的 Android 设备就变成了一个低延迟的网络音响。

同时还支持**反向音频通道**（Android 麦克风 → PC）和**远程音量控制**（PC → Android），构成完整的双向音频桥接。

| 发送端 | 传输方式 | 接收端 |
| :---: | :---: | :---: |
| Windows PC（Rust + Tauri） | Wi-Fi / USB（adb reverse） | Android 手机（Flutter + Oboe） |
| WASAPI  Loopback 采集 | Protocol v2 + Opus 编码 | 硬件加速 Opus 解码 |
| 无头模式或桌面 GUI | TCP + mDNS 服务发现 | Audio Console Dark 界面 |

## 界面截图

<div align="center">
  <table>
    <tr>
      <td align="center"><b>桌面发送端</b></td>
      <td align="center"><b>Android - 设备发现</b></td>
    </tr>
    <tr>
      <td><img src="screenshots/screenshot-desktop-sender.png" alt="桌面发送端" width="100%" /></td>
      <td><img src="screenshots/screenshot-android-discovery.png" alt="Android 设备发现" width="100%" /></td>
    </tr>
    <tr>
      <td align="center"><b>Android - 播放界面</b></td>
      <td align="center"><b>Android - 均衡器</b></td>
    </tr>
    <tr>
      <td><img src="screenshots/screenshot-android-playback.png" alt="Android 播放界面" width="100%" /></td>
      <td><img src="screenshots/screenshot-android-eq.png" alt="Android 均衡器" width="100%" /></td>
    </tr>
  </table>
</div>

## 功能特性

### 核心传输
- **实时音频推流** — 通过 WASAPI Loopback 采集 Windows 系统音频，经 TCP 实时传送到 Android
- **双模传输** — Wi-Fi（局域网）方便日常使用，USB（adb reverse）在复杂网络环境下更稳定
- **三种延迟模式** — `low_latency`（~64ms p95）、`balanced`（~185ms p95）、`high_quality`（~505ms p95）
- **Opus 编码** — 48kHz 低延迟 Opus 编码，VBR 动态比特率，高效利用带宽
- **多设备同时推流** — 最多支持 4 台 Android 设备同时连接，独立会话互不干扰

### 发现与连接
- **mDNS 自动发现** — Android 端自动扫描局域网内的 Windows 发送端，无需手动输入 IP
- **智能断线重连** — 指数退避策略（1s → 2s → 4s → 8s → 16s），网络恢复后自动重连
- **连接历史与收藏** — 常用设备持久化保存，一键重连

### Android 音频处理
- **3 段均衡器** — 低音 / 中音 / 高音独立调节，内置预设（平坦、低音增强、人声、高音增强），设置自动保存
- **响度归一化** — 基于 RMS 分析的软件增益控制，实时显示当前增益值，带平滑过渡
- **Oboe / AudioTrack** — 底层音频输出优先使用 Oboe（推荐），自动回退到 AudioTrack

### 反向通道与控制
- **麦克风 → PC 反向音频** — Android 麦克风通过 Opus 编码 TCP 推送到 PC（端口 7878），Windows 端以命名管道输出
- **PC 端音量控制** — 在 Windows 桌面托盘直接调节 Android 音量，手机端实时显示音量浮层提示
- **实时电平表** — 正向和反向音频通道均提供逐帧电平监控

### 可观测性
- **实时抖动可视化** — Android 界面内嵌抖动折线图，三段着色（绿/黄/红），显示 p50/p95 指标
- **桌面端诊断导出** — 一键导出运行时状态快照（JSON 格式）用于问题排查
- **MediaSession 集成** — Android 前台播放通知，支持 MediaStyle 控件和元数据展示

### 可靠性保障
- **永久回滚路径** — `legacy_las1 + pcm16` 降级路径始终保留，绝不删除
- **Protocol v2** — 能力协商、模式同步、参数重同步机制
- **强制回滚验证** — 命令行一键切换到回滚路径进行测试
- **灰度开关** — 协议路径通过运行时配置切换，而非直接删除代码

## 快速开始

### 环境要求

- **Windows 10+**，音频输出正常
- **Android 8.0+** 设备
- **Rust 1.75+**（仅发送端需要）
- 同一局域网的 **Wi-Fi** 连接，或一根 **USB 数据线**

### Windows 发送端

```powershell
# 克隆仓库
git clone https://github.com/MengBad/lan-audio.git
cd lan-audio

# 使用系统真实音频启动推流
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback

# 使用合成测试音源（用于诊断）
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic

# USB 模式（需要 adb 和设备序列号）
cargo run -p lan_audio_server --bin desktop_headless -- --transport usb --adb-serial <序列号> --audio-source windows_loopback
```

### Android 接收端

从 [GitHub Releases](https://github.com/MengBad/lan-audio/releases) 下载对应你设备的 APK：

| ABI | 适用设备 |
| --- | :---: |
| `arm64-v8a` | 大多数现代 Android 手机 |
| `armeabi-v7a` | 较旧的 32 位 ARM 设备 |
| `x86_64` | 模拟器 / Intel 设备 |

1. 在 Android 设备上安装 APK
2. 确保两端在同一 Wi-Fi 网络（或通过 USB 连接）
3. 打开 LAN Audio — mDNS 会自动发现附近的发送端
4. 点击发送端名称即可连接，也可以手动输入 IP 地址
5. 选择播放模式，开始享受音乐

### 桌面图形界面（Tauri）

```powershell
cd apps/desktop
cargo tauri dev
```

桌面版提供系统托盘界面，支持快照监控、音量预设和诊断导出。

## 系统架构

<p align="center">
  <img src="screenshots/screenshot-architecture.png" alt="系统架构图" width="700" />
</p>

### 仓库结构

```
apps/
  android_flutter/       Android 客户端（Flutter + Kotlin 原生桥接）
  desktop/               Windows 桌面应用（Tauri + Rust）

crates/
  lan_audio_domain/      共享领域模型和发布门控定义
  lan_audio_protocol/    协议 v1/v2 类型定义、数据包格式、协商逻辑
  lan_audio_server/      音频采集、Opus 编码、TCP 传输、会话运行时

docs/                    协议规范、UI 设计、发布策略、路线图
scripts/                 本地验证、打包、发布自动化脚本
artifacts/release/       发布门控记录和设备验收证据
```

### 数据面路径

| 路径 | 协议头 | 编码 | 状态 |
| :--- | :--- | :--- | :--- |
| **主路径** | `v2_header` | `opus` | 推荐使用 |
| **回滚路径** | `legacy_las1` | `pcm16` | 永久保留 |

服务快照可暴露配置的和实际运行的路径状态，包含 EQ、响度归一化、重连状态、多设备摘要等信息。

## 开发指南

### 环境搭建

```powershell
# 一键本地验证
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1

# 调试模式运行 Android 应用
cd apps\android_flutter
flutter run

# 构建发布产物
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

### 本地检查清单

提交 PR 前，请执行以下检查（或直接运行 `validate_local.ps1` 一键完成）：

- `cargo fmt --all -- --check`
- `cargo check`
- `cargo test -p lan_audio_protocol -p lan_audio_server`
- `cargo check -p lan_audio_desktop`
- `flutter analyze`
- `flutter test`

### 提交规范

- 使用前缀：`feat:`、`fix:`、`chore:`、`refactor:`、`docs:`
- 每个 PR 聚焦单一变更
- 绝不删除或隐藏回滚路径
- 详见 [CONTRIBUTING.md](CONTRIBUTING.md)

## 版本发布

版本号统一由 [VERSION](VERSION) 文件管理。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Version 1.8
```

GitHub Release 产物：
- `lan-audio-android-arm64-v8a-v1.8.apk`
- `lan-audio-android-armeabi-v7a-v1.8.apk`
- `lan-audio-android-x86_64-v1.8.apk`
- `lan-audio-desktop-v1.8.exe`
- `SHA256SUMS.txt`

## 文档

| 文档 | 说明 |
| :--- | :--- |
| [协议规范](docs/protocol.md) | 有线格式和数据包结构 |
| [Protocol v2 迁移](docs/protocol_v2_migration.md) | v1 → v2 迁移指南 |
| [桌面 UI](docs/desktop_ui.md) | Tauri 桌面界面设计 |
| [系统架构](docs/architecture.md) | 系统设计与数据流 |
| [开发环境](docs/dev_setup.md) | 开发环境配置指南 |
| [发布策略](docs/RELEASE_POLICY.md) | 发布标准与流程 |
| [已知问题](docs/known_issues.md) | 当前限制与待解决问题 |
| [更新日志](CHANGELOG.md) | 版本变更记录 |
| [贡献指南](CONTRIBUTING.md) | 如何参与贡献 |

## 回滚与恢复

如果推荐的 Opus 路径不稳定，可以使用以下降级方式：

```powershell
# Legacy PCM16 降级
cargo run -p lan_audio_server --bin desktop_headless -- --force-rollback

# 或显式指定路径组合
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --force-rollback
```

可用降级组合：
- `legacy_las1 + pcm16`
- `windows_loopback + legacy_las1 + pcm16`
- `synthetic + v2_header + pcm16`

## 常见问题

<details>
<summary><b>为什么锁屏后 Android 端会断连？</b></summary>
请检查电池优化设置。LAN Audio 内置了省电模式保活指南，覆盖小米、华为及通用 Android 路径，可在 <b>设置 → 省电引导</b> 中查看。
</details>

<details>
<summary><b>能通过互联网使用吗？</b></summary>
LAN Audio 为局域网设计。互联网传输受限于延迟和带宽，体验不佳。如需远程使用，建议通过 VPN 接入家庭网络。
</details>

<details>
<summary><b>最低能到多少延迟？</b></summary>
<code>low_latency</code> 模式通过 Wi-Fi 传输：p95 ≈ 64ms。USB 模式延迟略低。<code>balanced</code> 模式约 185ms，<code>high_quality</code> 模式约 505ms，使用更大缓冲区换取更高音质。
</details>

## 开源许可

[MIT](LICENSE) © LAN Audio 贡献者

---

<p align="center">
  <sub>基于 Rust、Flutter、Tauri 和 Oboe 构建 — 为追求全屋音频自由的你。</sub>
</p>
