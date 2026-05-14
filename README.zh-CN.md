# 🔊 LAN Audio

把 Windows 电脑的声音实时传到 Android 手机上播放，手机变无线音响。

[English](README.md) | [下载](https://github.com/MengBad/lan-audio/releases)

---

## 使用场景

- 电脑音响不行，手机/平板扬声器还不错
- 躺床上想听电脑的声音
- 临时需要个无线音响，不想买硬件

## 截图

<!-- TODO: 添加真实截图到 screenshots/ 目录 -->

> 截图即将更新。可以从 [Releases](https://github.com/MengBad/lan-audio/releases) 下载体验。

## 特性

- **低延迟** — low_latency 模式 Wi-Fi p95 约 64ms，USB 更低
- **Opus 编码** — 48kHz VBR，省带宽
- **自动发现** — mDNS 局域网扫描，不用手动输 IP
- **三种模式** — low_latency / balanced / high_quality
- **断线重连** — 指数退避自动恢复
- **均衡器** — 3 段 EQ + 预设
- **反向麦克风** — 手机麦克风传回电脑
- **PC 控音量** — 电脑端直接调手机音量
- **多设备** — 最多 4 台手机同时收听
- **USB 模式** — 数据线直连，更稳定

## 快速开始

### 下载

**Android：** 从 [Releases](https://github.com/MengBad/lan-audio/releases) 下载 APK。大多数手机选 `arm64-v8a`。

**Windows：** 从 Releases 下载 exe，或自己编译：

```powershell
git clone https://github.com/MengBad/lan-audio.git
cd lan-audio
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source windows_loopback
```

### 使用

1. 电脑和手机连同一个 Wi-Fi
2. 电脑启动 LAN Audio
3. 手机打开 App，自动发现电脑
4. 点击连接，开始播放

## 开发

```powershell
# 一键验证
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1

# 构建 Release
powershell -ExecutionPolicy Bypass -File .\scripts\package_release.ps1 -Clean
```

## 协议

[MIT](LICENSE)
