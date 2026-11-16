# Architecture (v1.15)

中文摘要：本文描述 LAN Audio 当前双端架构。Windows 发送端采集系统音频并编码推流，Android 接收端以 Kotlin/Oboe 后台服务播放；控制面与数据面分离，共享契约由 `crates/lan_audio_domain` 持有。主路径与回滚路径必须同时可运行。

## Goals

Windows PC 在局域网（Wi-Fi / USB adb）向 Android 手机实时传音频，手机充当音响。设计约束：低延迟、播放稳定、模式/能力可协商、回滚路径长期可用。

## Workspace Layout

| 模块 | 职责 |
|---|---|
| `crates/lan_audio_domain` | 模式契约、连接状态机、回滚状态、失败码、service snapshot、发布门控 schema |
| `crates/lan_audio_protocol` | WebSocket JSON 控制面（v2 hello/hello_ack/set_audio_mode 等）+ UDP 二进制头（LAS1 / LAV2 / v3 分片） |
| `crates/lan_audio_server` | 音频采集、编码、传输、会话、自适应运行时、反向麦克风通道、mDNS |
| `apps/desktop/src-tauri` | Windows Tauri 2 桌面端，前端 `apps/desktop/web/index.html` |
| `apps/android_flutter` | Flutter UI + Kotlin 播放服务（Oboe / AudioTrack） |
| `apps/android_flutter/android/app/src/main/kotlin` | PlaybackForegroundService、PlaybackSessionRuntime、Oboe sink、jitter、EQ、响度归一化、MediaSession |

工作区 4 个 crate：`lan_audio_domain` / `lan_audio_protocol` / `lan_audio_server` / `lan_audio_desktop`。`desktop_headless` 是 CLI 调试与回归入口，不是日常用户路径。

## End-to-End Path (current main path)

1. Windows WASAPI Loopback 采集 PCM（默认 10ms frame；ultra_low_latency 为 5ms）。
2. `AudioFrameEncoder` 按协商 codec 编码（默认 Opus 48kHz VBR；非 48kHz 输入走 sinc 重采样，实例跨帧复用）。
3. `DataPlaneRouter` 按 `ServerConfig` 选择 `v2_header`（LAV2）发送格式；UDP 数据面推流，最多 4 个客户端会话。
4. 控制面 WebSocket 完成 hello/hello_ack、capabilities、set_audio_mode/audio_mode_changed、参数重同步（config_changed / discontinuity）。
5. Android `PlaybackForegroundService` 收包 → `LasPacketParser` 解析 v2/v3 → 解码/PLC → jitter buffer → playout pacing → Oboe callback / AudioTrack 写入。
6. 客户端按 ping 节奏回传 `client_watermark`；服务端 `AudioSyncEngine`（Kalman+PID）与 CPU/queue watchdog 产出 Green/Yellow/Red tier，动态下调编码器档位。
7. Flutter 通过 `MethodChannel` / `EventChannel` 消费稳定 snapshot 并渲染 3-tab UI（Play / Connect / Settings）与实时延迟图。

## Control / Data / Contract Planes

- 控制面：WebSocket JSON，`serde(tag="type")`；未知 tag 安静忽略，保证旧端兼容。
- 数据面：UDP 二进制；LAS1（legacy）、LAV2（v2_header，默认）、v3（37B 头 + 分片，仅 `supports_hires_pcm24` 双端开启时）。
- 契约面：UI/服务端都读 `lan_audio_domain` 的 snapshot 类型，禁止各端重复定义。
- 发现面：mDNS `_lan-audio._tcp.local.`（TXT 携带由 `env!("CARGO_PKG_VERSION")` 派生的版本与当前模式）+ UDP 广播 + 手动 IPv4 + QR。

## Runtime Paths

| 路径 | 状态 |
|---|---|
| `windows_loopback + v2_header + opus` | 默认主路径 |
| `legacy_las1 + pcm16` | 永久回滚（`--force-rollback` 或桌面端安全模式） |
| `synthetic + v2_header` | 测试/回归路径 |
| `--no-adaptive-runtime` | 关闭 watchdog 的应急回滚 |
| USB tethering / usb_direct | USB 连接方式；usb_direct 不作为默认主路径 |
| v3 + PCM24 | 能力门控，默认关闭；Android 直通已回退到 v1.9.4 基线 |

## Android Playback

- 后台承载：`PlaybackForegroundService` + MediaSession/MediaStyle 通知，`kUseBackgroundPlaybackService=true`。
- 双 sink：Oboe callback 为维护方向，AudioTrack 为回退；三段 EQ（60Hz/1kHz/10kHz）与响度归一化在 PCM 写入前执行，`low_latency` 旁路响度。
- 保活：`PARTIAL_WAKE_LOCK + WifiLock`；前台服务 `onCreate()` 不声明 microphone 类型，麦克风能力仅在反向通道实际开启时升级。
- 反向通道：`ReverseChannelServer` 已 wire 进服务生命周期；Kotlin TCP 写帧与 Rust 读帧均为小端。

## Current Truth (verified 2026-05-25)

- 主路径与回滚路径都在代码中可用；发布决策来自 `artifacts/release/acceptance_gate.json` + `artifacts/release/device_acceptance.json`，不是文档措辞。
- 版本唯一源为根目录 `VERSION`；`scripts/check_version_sync.ps1` 校验 13 个版本面，`scripts/assert_release_gate.ps1` 校验门控契约。
- 生产入口为 3-tab `IndexedStack`；`MorePage` 自 v1.15 起不再挂载，`MoreServerData` 仍是 ConnectPage 数据契约。
- 未完成：v1.15 UI 构建的 30+ 分钟真机长稳、USB direct 10 分钟样本、4 档模式切换重同步回归、桌面单设备断开命令。详见 `docs/MAINTENANCE_PLAN.md`。
