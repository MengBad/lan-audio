# Protocol v2 Draft

## Release Update (`2026-04-24`)

- `v1.4` has been tagged and released under `FORCE_RELEASE=true`; release-tracking docs keep the remaining checklist items visible as human override instead of silently calling them passed.
- Post-release regression fixes keep the Protocol v2 contract stable: no data-plane header fields, control messages, mode enums, or rollback semantics were changed.
- Android stable snapshot export now includes the already-computed `rx_frames_per_sec` and `audio_track_write_frames_per_sec` UI metrics so the console can show real receive cadence without changing protocol meaning.
- Release decision is currently `allow_release`, sourced from `artifacts/release/acceptance_gate.json`.
- Latency revalidation is now structured: `scripts/export_latency_probe.ps1` reads diagnostics/snapshot JSON and emits `artifacts/latency/latency_probe_latest.json` with per-mode latency proxy targets.
- Shared mode contracts, connection state machine, rollback state, failure taxonomy, service snapshot, and release gate schema now live in `crates/lan_audio_domain`.
- Protocol messages still preserve v1/v2 compatibility, but shared contract types are now imported from the domain layer instead of being duplicated ad hoc.
- The maintained main-path target remains `windows_loopback + v2_header + opus`; the maintained rollback path remains `legacy_las1 + pcm16`.
- Phase 3 server-side data plane abstraction is in place: `legacy_las1`, `v2_header`, and `usb_direct` now have an explicit shared routing layer.

## 1. 协议目标

Protocol v2 的目标不是立即替换全部运行流量，而是为低延迟产品化升级建立稳定工程接口：

- 模式同步：服务端与客户端明确同步 `low_latency / balanced / high_quality`。
- 音频格式显式声明：控制面与数据面都能表达 codec / sample_rate / channels。
- 参数变化重同步：当模式或格式变化时，通过 `config_changed` / `discontinuity` 提示接收端。
- 低延迟优先：所有模式参数、播放后端偏好、数据面灰度都围绕降低端到端延迟设计，但不牺牲回滚能力。
- 为 Opus / USB / 智能策略预留：先落协议承载和能力协商，再逐步启用真实链路。

V2 的产品原则：

- 默认推荐：当前推荐主路径为 `windows_loopback + v2_header + opus`，并持续保留回滚路径。
- 可诊断：控制面必须暴露协议路径、模式、codec、推荐/回滚状态与推荐连接方式。
- 可回滚：`legacy_las1 + pcm16`、`synthetic + v2_header + pcm16` 必须长期可用。
- 可扩展：codec、USB、播放后端、microphone backhaul 不混入临时字段，统一通过 capabilities 与策略结构承载。

## 2. 协议分层

- 控制面：WebSocket（JSON，低频状态和协商）。
- 数据面：
  - Wi-Fi：UDP（二进制音频帧）
  - USB（adb reverse）：TCP（二进制音频帧，`4-byte big-endian length + frame payload`）

### 2.1 Runtime Snapshot Contract

稳定运行时 snapshot 现在同时暴露“配置的数据面格式”和“当前实际运行路径”：

- `data_plane`：配置/协商后的包格式（`legacy_las1` 或 `v2_header`）
- `active_data_plane`：实际运行路径（`legacy_las1`、`v2_header`、`usb_direct`）
- `rollback_available`：当前是否仍可显式回滚到 `legacy_las1 + pcm16`

## 3. 版本策略

- `protocol_version = 2` 表示 Protocol v2。
- 前向兼容：接收端遇到未知字段应忽略（保留已知字段处理）。
- 后向兼容：服务端与客户端允许保留 v1 流程（`client_hello/server_welcome` + `LAS1`）。
- 未识别消息类型：
  - 控制面：可忽略，并在 debug log 中记录。
  - 数据面：若 header 无法识别，丢弃该包并计数。

## 4. 控制面消息定义（WebSocket JSON）

### 4.1 hello

```json
{
  "type": "hello",
  "protocol_version": 2,
  "device_name": "Pixel 8",
  "client_id": "android-123",
  "udp_port": 54000,
  "desired_sample_rate": 48000,
  "channels": 2,
  "preferred_audio_mode": "balanced",
  "capabilities": {
    "supports_pcm16": true,
    "supports_f32": false,
    "supports_modes": true,
    "supports_metrics": true,
    "supports_opus_future": true,
    "supports_opus": true,
    "supports_opus_experimental": true,
    "supports_low_latency": true,
    "supports_high_quality": true,
    "supports_native_audio_track": true,
    "supports_fast_path": true,
    "supports_stable_audio_track": true,
    "supports_usb_tethering": true,
    "supports_usb_direct_future": false
  }
}
```

### 4.2 hello_ack

```json
{
  "type": "hello_ack",
  "protocol_version": 2,
  "accepted": true,
  "session_id": "c3f3e2a6-12ab-4f3c-9c27-7f7c93f6d0d2",
  "current_audio_mode": "balanced",
  "transport_type": "wifi",
  "mode_profile": {
    "mode": "balanced",
    "start_buffer_ms": 60,
    "max_buffer_ms": 300,
    "batch_frames": 2,
    "drop_threshold_ms": 220,
    "prefer_low_latency_path": false,
    "prefer_stable_audio_track": true,
    "preferred_codec": "opus",
    "preferred_sample_format": "pcm16",
    "frame_duration_ms": 10,
    "reset_buffer_on_switch": true
  },
  "message": "hello_ack",
  "capabilities": {
    "supports_pcm16": true,
    "supports_f32": false,
    "supports_modes": true,
    "supports_metrics": true,
    "supports_opus_future": true,
    "supports_opus": true,
    "supports_opus_experimental": true,
    "supports_low_latency": true,
    "supports_high_quality": true,
    "supports_native_audio_track": true,
    "supports_fast_path": true,
    "supports_stable_audio_track": true,
    "supports_usb_tethering": true,
    "supports_usb_direct_future": false
  }
}
```

### 4.3 server_info

```json
{
  "type": "server_info",
  "server_id": "38d8aeb8-0ad2-4e11-9a56-1208c8c8cc9d",
  "server_name": "windows-desktop",
  "platform": "windows",
  "app_version": "0.1.0",
  "ws_port": 39991,
  "udp_port": 39992,
  "protocol_version": 2,
  "current_audio_mode": "balanced",
  "mode_profile": {
    "mode": "balanced",
    "start_buffer_ms": 60,
    "max_buffer_ms": 300,
    "batch_frames": 2,
    "drop_threshold_ms": 220,
    "prefer_low_latency_path": false,
    "prefer_stable_audio_track": true,
    "preferred_codec": "opus",
    "preferred_sample_format": "pcm16",
    "frame_duration_ms": 10,
    "reset_buffer_on_switch": true
  },
  "codec": "opus",
  "data_plane": "v2_header",
  "gray_mode": false,
  "recommended_connection": "usb_tethering_or_5ghz_wifi"
}
```

`hello_ack.transport_type`:

- `wifi`: Wi-Fi/LAN path
- `usb`: adb reverse + localhost TCP path

### 4.4 client_info

```json
{
  "type": "client_info",
  "client_name": "flutter-android",
  "platform": "android",
  "app_version": "v29",
  "udp_port": 54000
}
```

### 4.5 set_audio_mode

```json
{
  "type": "set_audio_mode",
  "mode": "low_latency",
  "reason": "user_selected"
}
```

### 4.6 audio_mode_changed

```json
{
  "type": "audio_mode_changed",
  "mode": "low_latency",
  "applied": true,
  "reason": "applied",
  "mode_profile": {
    "mode": "low_latency",
    "start_buffer_ms": 40,
    "max_buffer_ms": 180,
    "batch_frames": 1,
    "drop_threshold_ms": 140,
    "prefer_low_latency_path": true,
    "prefer_stable_audio_track": false,
    "preferred_codec": "opus",
    "preferred_sample_format": "pcm16",
    "frame_duration_ms": 10,
    "reset_buffer_on_switch": true
  }
}
```

### 4.7 playback_state

```json
{
  "type": "playback_state",
  "state": "streaming",
  "buffered_ms": 96,
  "active_sessions": 1
}
```

### 4.8 metrics_snapshot

```json
{
  "type": "metrics_snapshot",
  "tx_packets": 12345,
  "tx_bytes": 23654400,
  "capture_read_errors": 0,
  "capture_underruns": 2,
  "active_sessions": 1
}
```

### 4.9 error

```json
{
  "type": "error",
  "code": "capture_init_failed",
  "message": "capture source is not started",
  "recoverable": true
}
```

### 4.10 reconnect_hint

```json
{
  "type": "reconnect_hint",
  "after_ms": 1000,
  "reason": "network_jitter_spike"
}
```

### 4.11 client_list

```json
{
  "type": "client_list",
  "clients": [
    { "id": "c3f3e2a6-12ab-4f3c-9c27-7f7c93f6d0d2", "name": "Pixel 8", "mode": "balanced" }
  ]
}
```

说明：

- 服务端可在客户端列表变化或 mode 变化后推送。
- Android 可只展示数量（例如“当前共 N 台设备连接中”）。

### 4.12 client_joined / client_left

```json
{
  "type": "client_joined",
  "id": "c3f3e2a6-12ab-4f3c-9c27-7f7c93f6d0d2",
  "name": "Pixel 8"
}
```

```json
{
  "type": "client_left",
  "id": "c3f3e2a6-12ab-4f3c-9c27-7f7c93f6d0d2",
  "name": "Pixel 8"
}
```

说明：

- 服务端广播给所有 v2 客户端。
- 客户端可据此维护当前连接设备数。

## 5. 数据面包头定义（UDP Binary v2）

字段（小端）：

| 字段 | 类型 | 说明 |
|---|---|---|
| magic | [u8;4] | 固定 `LAV2` |
| protocol_version | u8 | 固定 `2` |
| header_size | u16 | 当前头长度 |
| flags | u16 | 包语义位 |
| sequence | u32 | 递增序号 |
| timestamp_ms | u64 | 帧时间戳 |
| codec | u8 | 1=pcm16, 2=f32, 3=opus |
| channels | u8 | 声道数 |
| sample_rate | u32 | 采样率 |
| frame_duration_ms | u16 | 帧时长（ms） |
| payload_size | u16 | payload 字节数 |
| reserved | u16 | 预留 |

## 6. flags 定义

- `silence`：`1 << 0`
- `config_changed`：`1 << 1`
- `discontinuity`：`1 << 2`

说明：

- `config_changed` 用于提示参数变化（采样率/声道/模式切换后的新配置边界）。
- `discontinuity` 用于提示接收侧 reset jitter/decoder 状态。

## 7. capabilities 协商

能力字段：

- `supports_pcm16`
- `supports_f32`
- `supports_modes`
- `supports_metrics`
- `supports_opus_future`
- `supports_opus`
- `supports_opus_experimental`
- `supports_low_latency`
- `supports_high_quality`
- `supports_native_audio_track`
- `supports_fast_path`
- `supports_stable_audio_track`
- `supports_usb_tethering`
- `supports_usb_direct_future`

协商原则：

- 连接建立时双向声明 capabilities。
- 运行时策略应取双方能力交集。
- 不支持的能力不应强制启用。
- `supports_opus=true` 表示该端具备稳定 Opus 链路能力。
- `supports_opus_experimental=true` 仅作为兼容字段保留，便于旧客户端继续协商到稳定 `opus`。
- Android 端会根据 `libopus` JNI 是否可用动态声明 Opus capability。
- `supports_usb_tethering=true` 表示产品层推荐 USB tethering 作为低延迟连接路径；`supports_usb_direct_future` 仅为后续 USB direct 预留。

## 8. 模式策略承载结构

模式字段使用统一枚举：

- `low_latency`
- `balanced`
- `high_quality`

承载路径：

- 连接时：`hello.preferred_audio_mode`
- 连接应答：`hello_ack.mode_profile`
- 运行时切换：`set_audio_mode`
- 生效回执：`audio_mode_changed.mode_profile`
- 服务端状态：`server_info.mode_profile`
- 数据面提示：必要时配合 `config_changed/discontinuity`

`AudioModeProfile` 字段：

| 字段 | 说明 |
| --- | --- |
| `start_buffer_ms` | 开始播放前目标缓冲 |
| `max_buffer_ms` | 允许的最大缓冲 |
| `batch_frames` | 每次 AudioTrack 写入合并的 10ms 帧数 |
| `drop_threshold_ms` | 超过该阈值时允许丢弃旧帧追赶 |
| `prefer_low_latency_path` | 是否优先低延迟/fast path |
| `prefer_stable_audio_track` | 是否优先稳定 AudioTrack 路径 |
| `preferred_codec` | 当前 codec 偏好，默认 `opus` |
| `preferred_sample_format` | 当前 sample format 偏好，默认 `pcm16` |
| `frame_duration_ms` | 单帧时长，当前为 10ms |
| `reset_buffer_on_switch` | 模式切换时是否重置 jitter/audio track |

模式矩阵：

| mode | start/max | batch | drop threshold | 后端偏好 | 使用场景 |
| --- | --- | ---: | ---: | --- | --- |
| `low_latency` | 40/180ms | 1 | 140ms | 低延迟/fast path | 游戏、视频跟听 |
| `balanced` | 60/300ms | 2 | 220ms | 稳定 AudioTrack | 默认 |
| `high_quality` | 120/500ms | 3 | 420ms | 稳定后端和更大缓冲 | 音乐、长时播放 |

## 9. 迁移策略

### 9.1 当前主路径

- 当前推荐主路径：
  - 音频源：`windows_loopback`
  - 数据面：`v2_header`
  - codec：`opus`
- 当前维护的回滚路径：
  - `legacy_las1 + pcm16`

### 9.2 Protocol v2 当前状态

- 状态：**推荐默认路径已切到 `windows_loopback + v2_header + opus`；回滚路径 `legacy_las1 + pcm16` 持续维护中**。
- 已接入：
  - v2 结构体与消息定义
  - `hello/hello_ack` 运行时协商（协议版本 + capabilities）
  - `client_info/server_info` 运行时交换（app/platform）
  - `set_audio_mode/audio_mode_changed` 运行时同步
  - UDP v2 header 编解码 + 双栈识别入口
  - `AudioModeProfile` 运行时下发与 Android 播放策略应用
  - 稳定 Opus 入口（Opus 在 `v2_header` 下作为推荐默认链路启用）
- 未全量完成：
  - Android 真机 loopback 长稳样本仍待补齐；延迟复核已通过 `scripts/export_latency_probe.ps1` 结构化为 artifact 流程
  - USB direct 尚未实现
  - 按连接动态协商切换 v2 header 仍待收敛，当前仍以配置默认值为主

### 9.3 推荐迁移顺序

1. 先控制面：稳定 `hello/hello_ack` 与 capabilities。
2. 再模式状态：`set_audio_mode` 与运行状态回执。
3. 再播放策略：把 `AudioModeProfile` 的缓冲/批处理/后端偏好与 Android 播放链路打通。
4. 再数据面 header：灰度启用 v2 header。
5. 再 codec：接入并稳定 Opus 链路，保留 PCM16 fallback。
6. 最后切换实际主路径：逐步迁移默认运行流量。

## 9.4 Opus 接入策略

- 协议层：`AudioCodecPreference::Opus` 与 `UdpAudioCodecV2::Opus` 为正式稳定枚举；旧 `opus_experimental` 字面量仍兼容解析到 `opus`。
- capabilities：`supports_opus` 表示双方允许稳定 Opus 协商；`supports_opus_experimental` 仅作兼容保留。
- 配置入口：服务端提供 `--codec opus`，桌面端推荐默认选择 `opus`。
- 当前行为：
  - 当有效数据面为 `v2_header` 且 codec 选择为 `opus` 时，服务端使用标准 libopus 编码，固定输出 48kHz 立体声、20ms / 960 samples per channel 的 Opus 帧，数据面 header 写入 `codec=3`。
  - Android 后台播放链路识别 `codec=3` 后，使用 `libopus` JNI 解码为 PCM16，再进入现有 jitter buffer / AudioTrack。
  - Android Opus decode 失败时优先走 PLC concealment，不直接静音。
  - 当数据面回退到 `legacy_las1` 时，effective codec 必须回退 `pcm16`。
- 当前验收：`synthetic + v2_header + opus` 已完成真机非零 PCM 与听感验收，并通过 5 分钟服务端压力测试（`p99 encode ~= 0.509 ms`，channel-full drop rate `0.000000`）。
- 下一步：继续用 `scripts/export_latency_probe.ps1` 将三档模式的真实诊断快照沉淀为 `artifacts/latency/` 结构化结果。

## 9.5 USB 连接策略

- USB tethering 被纳入 V2 低延迟推荐连接路径。
- 当前实现仍通过 IP/WebSocket/UDP 传输；USB tethering 的作用是提供更稳定、更低抖动的局域网链路。
- `supports_usb_tethering` 表示产品层可提示用户尝试 USB tethering。
- `supports_usb_direct_future` 只为未来 USB direct 传输预留，当前不启用。

## 10. v2 synthetic 真机验收结论

- 日期：2026-04-14（Asia/Shanghai）
- 结论：**synthetic + v2_header 真机验收通过**
- 约束保持不变：
  - `legacy_las1 + pcm16` 继续作为回滚路径维护
  - `windows_loopback + v2_header + opus` 已切为推荐默认路径
  - Opus 已完成稳定编码/解码接线，并通过 synthetic 真机听感与服务端压力测试
- 验收配置：
  - 服务端：`synthetic + --data-plane v2_header`
  - 真机：Android `5391d451 / Xiaomi 24129PN74C`
  - 切换序列：`balanced -> low_latency -> high_quality -> balanced`
- 服务端日志：
  - 记录到 `audio mode changed; mark config_changed/discontinuity in outgoing packet`
  - 依次命中 `Balanced -> LowLatency`, `LowLatency -> HighQuality`, `HighQuality -> Balanced`
- Android 指标（dump）：
  - `after_low_latency`: `Playback=playing`, `buffered_ms=50`, `jitter_underrun=0`, `dropped_late_frames=0/0`, `silence_fill_count=0`, `rx_frames_per_sec=100.0`, `audio_track_write_frames_per_sec=100.0`, `cfg_changed=1`, `discontinuity=2`
  - `after_high_quality`: `Playback=playing`, `buffered_ms=50`, `jitter_underrun=0`, `dropped_late_frames=0/0`, `silence_fill_count=0`, `rx_frames_per_sec=104.5`, `audio_track_write_frames_per_sec=99.5`, `cfg_changed=2`, `discontinuity=3`
  - `after_balanced_final`: `Playback=playing`, `buffered_ms=70`, `jitter_underrun=0`, `dropped_late_frames=0/0`, `silence_fill_count=0`, `rx_frames_per_sec=99.0`, `audio_track_write_frames_per_sec=99.0`, `cfg_changed=3`, `discontinuity=4`
- 客户端行为：
  - `config_changed/discontinuity` 到达后执行最小重同步（`udp_config_changed_resync` + `init AudioTrack`）
  - 模式切换窗口内未见长时间 buffering、突然静音或持续异常波动
- 下一阶段建议：
  - 继续补齐 `windows_loopback + v2_header + opus` 的 Android 真机长稳、低延迟与 USB 样本

## 11. loopback + v2_header 小流量灰度结论

- 日期：2026-04-15（Asia/Shanghai）
- 结论：**`windows_loopback + v2_header` 已完成小流量灰度，并晋升为推荐默认路径；`legacy_las1 + pcm16` 继续作为回滚路径**
- 当前保护：
  - 默认主路径为 `windows_loopback + v2_header + opus`
  - 桌面 UI 提供“安全模式”按钮，一键切回 `legacy_las1 + pcm16`
  - CLI 仍可通过 `--data-plane legacy_las1 --codec pcm16` 显式回滚
- 可回滚路径：
  - `windows_loopback + legacy_las1 + pcm16`
  - `synthetic + v2_header + pcm16`
- 真机事实：
  - Android 真机已建立真实连接并进入持续 `playing`（连续 >2 分钟）
  - `mock_android_client` 收包统计：`v2=1207, v1=0, cfg_changed=3, discontinuity=4`
  - 本轮日志未出现 `capture source is not started`
  - `capture_last_peak/capture_last_rms/capture_source_state` 暂无统一结构化导出口，本轮仅做现象级观察
- Android 指标（模式切换完成后）：
  - `Playback=playing`
  - `buffered_ms=20~300`
  - `jitter_underrun=0`
  - `dropped_late_frames=0 -> 104`
  - `silence_fill_count=0`
  - `rx_frames_per_sec≈99~101`
  - `audio_track_write_frames_per_sec≈99~101`（切换瞬间可短暂波动）
  - `cfg_changed=3`
  - `discontinuity=4`
- 模式切换：
  - 已完成 `balanced -> low_latency -> high_quality -> balanced`
  - 服务端正确打出 `config_changed/discontinuity`，Android 执行最小重同步（`udp_config_changed_resync` + `init AudioTrack`）
- 主阻塞点：
  - **模式切换重同步与 Android 播放侧缓冲策略**
  - 数据面 v2 连通性与 loopback 可播已成立，但切换后存在缓冲抬升与 late frame 累积，需要继续稳定性优化

### 11.1 风险点

- 两端版本不一致。
- 模式切换时参数不同步。
- header 升级导致旧客户端不兼容。

## 12. 当前实现状态（仓库事实）

- 文档层：本文件已定义 Protocol v2 低延迟产品升级草案、模式策略、Opus/USB 路线与迁移策略。
- 代码骨架：`crates/lan_audio_protocol` 已提供 v2 常量、消息结构、capabilities、audio mode profile、codec preference、UDP v2 header。
- 控制面联动：
  - `crates/lan_audio_server/src/session.rs` 已接通 v2 `hello/hello_ack`、`client_info/server_info`、`set_audio_mode/audio_mode_changed`。
  - Android 后台服务链路已发送 v2 `hello/client_info` 并处理 `hello_ack/server_info/audio_mode_changed/server_info.mode_profile`。
  - Windows 桌面客户端快照已显示 `current_audio_mode`、`mode_profile`、`protocol_path`、codec 与灰度状态。
- 发送/接收预留：
  - 服务端可按配置发送 `legacy_las1` / `v2_header`（桌面默认 `v2_header`）。
  - Android/Flutter 接收侧均可识别 `LAS1/LAV2` 双栈头。
  - 服务端标准 libopus `opus` 编码与 Android `libopus` JNI 解码已接入推荐路径，decode 失败时走 PLC concealment。
  - `config_changed/discontinuity` 已有最小处理：接收侧执行 jitter/audio track 重同步。
- 模式策略：
  - `AudioModeProfile` 已在 Rust/Android/桌面端形成一致语义。
  - Android jitter buffer 已按 mode profile 调整 start/max buffer、batch 和 drop threshold。
- 尚未完成：Android 真机 `windows_loopback + v2_header + opus` 的长稳 / USB 样本仍待补齐；latency 复核已具备结构化 probe/export；USB direct 未实现。

## 13. Opus synthetic 真机听感验收结论

- 结论：**synthetic + v2_header + opus 真机听感验收通过，并已完成 5 分钟服务端压力测试。**
- 日期：2026-04-20（Asia/Shanghai）
- 配置：`synthetic + --data-plane v2_header --codec opus`
- 真机：`5391d451 / Xiaomi 24129PN74C`
- 客观指标：`Playback=playing`，`pcmPeak≈6539~8731`，`pcmRms≈0.138~0.155`，`rx_frames_per_sec≈99~101`，`audio_track_write_frames_per_sec≈96~101`。
- 主观听感：用户已确认听到测试音，且没有卡顿、破音。
- 默认策略：推荐默认路径已切到 `windows_loopback + v2_header + opus`；`legacy_las1 + pcm16` 继续作为回滚路径。

### 12.1 灰度验收记录（2026-04-14）

- 已完成（本机可复现）：
  - `synthetic + v2_header` 本地灰度联调通过（`mock_android_client`）。
  - `LAV2` 识别与 v2 收包稳定（无序号丢失）。
  - 模式切换触发 `config_changed/discontinuity`，接收侧最小重同步路径可执行。
- 后续优化项（非阻塞）：
  - 继续压降模式切换后的 `buffered_ms` 峰值与 `dropped_late_frames` 累积。
  - 继续增加多机型 / 长时段 / USB 下的 `windows_loopback + v2_header + opus` 样本。
