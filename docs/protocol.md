# Protocol v2 Draft

## 1. 协议目标

Protocol v2 的目标不是立即替换全部运行流量，而是为后续能力升级建立稳定工程接口：

- 模式同步：服务端与客户端明确同步 `low_latency / balanced / high_quality`。
- 音频格式显式声明：控制面与数据面都能表达 codec / sample_rate / channels。
- 参数变化重同步：当模式或格式变化时，通过 `config_changed` / `discontinuity` 提示接收端。
- 为未来 Opus / 智能策略预留：先落协议承载，再逐步启用真实链路。

## 2. 协议分层

- 控制面：WebSocket（JSON，低频状态和协商）。
- 数据面：UDP（二进制音频帧）。

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
    "supports_low_latency": true,
    "supports_high_quality": true,
    "supports_native_audio_track": true
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
  "message": "hello_ack",
  "capabilities": {
    "supports_pcm16": true,
    "supports_f32": false,
    "supports_modes": true,
    "supports_metrics": true,
    "supports_opus_future": false,
    "supports_low_latency": true,
    "supports_high_quality": true,
    "supports_native_audio_track": true
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
  "current_audio_mode": "balanced"
}
```

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
  "reason": "applied"
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
| codec | u8 | 1=pcm16, 2=f32, 3=opus_future |
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
- `supports_low_latency`
- `supports_high_quality`
- `supports_native_audio_track`

协商原则：

- 连接建立时双向声明 capabilities。
- 运行时策略应取双方能力交集。
- 不支持的能力不应强制启用。

## 8. 模式策略承载结构

模式字段使用统一枚举：

- `low_latency`
- `balanced`
- `high_quality`

承载路径：

- 连接时：`hello.preferred_audio_mode`
- 运行时切换：`set_audio_mode`
- 生效回执：`audio_mode_changed`
- 数据面提示：必要时配合 `config_changed/discontinuity`

## 9. 迁移策略

### 9.1 当前主路径

- 当前运行主路径仍为 legacy/v1：
  - 控制面：`client_hello / server_welcome`
  - 数据面：`LAS1` packet

### 9.2 Protocol v2 当前状态

- 状态：**部分启用（控制面联动已接通）**。
- 已接入：
  - v2 结构体与消息定义
  - `hello/hello_ack` 运行时协商（协议版本 + capabilities）
  - `client_info/server_info` 运行时交换（app/platform）
  - `set_audio_mode/audio_mode_changed` 运行时同步
  - UDP v2 header 编解码 + 双栈识别入口
- 未全量启用：
  - 音频数据仍默认走 `LAS1`
  - UI 未全面开放模式切换到生产路径
  - v2 数据面灰度当前仍保持非默认、可回滚

### 9.3 推荐迁移顺序

1. 先控制面：稳定 `hello/hello_ack` 与 capabilities。
2. 再模式状态：`set_audio_mode` 与运行状态回执。
3. 再数据面 header：灰度启用 v2 header。
4. 最后切换实际主路径：逐步迁移默认运行流量。

## 10. v2 synthetic 真机验收结论

- 日期：2026-04-14（Asia/Shanghai）
- 结论：**synthetic + v2_header 真机验收通过**
- 约束保持不变：
  - `loopback + v2_header` 仍非默认，且仅允许在显式灰度开关下启用
  - 未切 `v2_header` 为默认主路径
  - 未引入 Opus
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
  - 可以评估 `loopback + v2_header` 小流量灰度，但必须继续保持灰度开关和回滚路径

## 11. loopback + v2_header 小流量灰度结论

- 日期：2026-04-14（Asia/Shanghai）
- 结论：**loopback + v2_header 当前未通过**
- 灰度保护：
  - 默认主路径仍为 `legacy_las1`
  - `windows_loopback + v2_header` 仅在显式开关 `--allow-loopback-v2-header-gray` 下允许发送 `LAV2`
  - 未开启显式开关时，`windows_loopback + --data-plane v2_header` 会自动回退到 `legacy_las1`
- 可回滚路径：
  - `windows_loopback + legacy_las1`
  - `synthetic + v2_header`
- 真机事实：
  - Android 真机已完成真实连接，服务端确认 `selected_data_plane=v2_header`
  - Windows loopback 采集已启动，且未观察到 `capture source is not started`
- Android 指标（基线阶段）：
  - `Playback=buffering`
  - `buffered_ms=0~50`
  - `jitter_underrun=23 -> 30`
  - `dropped_late_frames=0/0`
  - `silence_fill_count=0`
  - `rx_frames_per_sec=5.9 ~ 7.0`
  - `audio_track_write_frames_per_sec=5.9 ~ 12.0`
  - `cfg_changed=0`
  - `discontinuity=1`
- 服务端观察：
  - `capture_no_packet_count` 与 `capture_underruns` 持续升高
  - `capture_last_peak / capture_last_rms` 大多数时间接近 `0`
  - `tx_frames_per_sec` 长时间仅约 `6.4 ~ 6.6 fps`
- 模式切换：
  - 已确认 `balanced -> low_latency` 命中服务端，并正确打出 `config_changed/discontinuity`
  - 但 Android 真机在后续切换阶段发生 `adb` 掉线，未完成 `low_latency -> high_quality -> balanced` 全序列
- 主阻塞点：
  - **Windows 采集端**
  - v2 数据面与 Android 播放链路已连通，但 loopback 采集帧输出节奏异常，客户端持续停留在 `buffering`

### 9.4 风险点

- 两端版本不一致。
- 模式切换时参数不同步。
- header 升级导致旧客户端不兼容。

## 12. 当前实现状态（仓库事实）

- 文档层：本文件已定义 Protocol v2 草案与迁移策略。
- 代码骨架：`crates/lan_audio_protocol` 已提供 v2 常量、消息结构、capabilities、audio mode、UDP v2 header。
- 控制面联动：
  - `crates/lan_audio_server/src/session.rs` 已接通 v2 `hello/hello_ack`、`client_info/server_info`、`set_audio_mode/audio_mode_changed`。
  - Android 后台服务链路已发送 v2 `hello/client_info` 并处理 `hello_ack/server_info/audio_mode_changed`。
  - Windows 桌面客户端快照已显示 `current_audio_mode` 与 `protocol_version`。
- 发送/接收预留：
  - 服务端可按配置发送 `legacy_las1` / `v2_header`（默认 `legacy_las1`）。
  - Android/Flutter 接收侧均可识别 `LAS1/LAV2` 双栈头。
  - `config_changed/discontinuity` 已有最小处理：接收侧执行 jitter/audio track 重同步。
- 尚未启用：默认数据面仍发送 legacy `LAS1`，Opus 仍未接线，loopback + v2_header 仍需显式灰度开关且当前真机灰度未通过。

### 12.1 灰度验收记录（2026-04-14）

- 已完成（本机可复现）：
  - `synthetic + v2_header` 本地灰度联调通过（`mock_android_client`）。
  - `LAV2` 识别与 v2 收包稳定（无序号丢失）。
  - 模式切换触发 `config_changed/discontinuity`，接收侧最小重同步路径可执行。
- 未完成（当前环境阻塞）：
  - `loopback + v2_header` 的完整模式切换序列真机采样尚未补齐（后半段被 `adb` 掉线打断）。
  - `high_quality` 与最终回切 `balanced` 的客户端 dump 尚未重新补采。
