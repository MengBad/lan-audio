# Hi-Res PCM24 Passthrough — Design Spec (v0.1)

> 状态：设计稿，待评审。代码尚未开始实现。
>
> 目标版本：作为独立 minor bump 发布（候选 v1.10.0），与 v1.9.x 主路径并行可选。
>
> 主路径不变：`windows_loopback + v2_header + opus + 48 kHz` 仍是默认。
> 回滚路径不变：`legacy_las1 + pcm16` 永久保留。

## 1. 动机

当前协议链路所有采样率被 `normalize_encoder_sample_rate` 强制规范到 Opus 支持的 5 个值（8 / 12 / 16 / 24 / 48 kHz），最高 48 kHz。这是 Opus 编解码器本身的物理上限，不是工程偏好。

少数高解析音乐用户（USB DAC、有线监听耳机、HiFi 设备）能在 ≥ 96 kHz 下听到差异。本特性为这部分用户提供一条 **可选** 的无损直通路径，不影响默认链路。

## 2. 非目标

为避免范围蔓延，本期 **不** 处理：

- FLAC 无损压缩。带宽消耗高，但实现复杂度也高。本期只做 PCM24 直通；如未来需要无损压缩再单独立项。
- 32-bit float 音频。Hi-Res 圈惯例是 24bit 整数，浮点是制作环境，不是回放标准。
- 384 kHz / 768 kHz "DXD"。市售设备极少；带宽超过 LAN 实际可用区间。
- USB direct 数据面下的 Hi-Res。本期只支持 v2_header WiFi。

## 3. 受影响层与改动概览

| 层 | 变更 |
|---|---|
| `lan_audio_protocol` | `UdpAudioCodecV2` 增加 `Pcm24 = 4`；`AudioCodecPreference` 增加 `Pcm24`；`ProtocolCapabilities` 增加 `supports_hires_pcm24: bool` |
| `lan_audio_server::config` | `CodecSelection` 增加 `Pcm24`；`--codec hires_pcm24` 入口 |
| `lan_audio_server::audio_capture` | WASAPI 不再向上规范化采样率，按 `GetMixFormat()` 原值出帧；保留现有 48 kHz 重采样路径供 Opus 使用 |
| `lan_audio_server::transport` | 新增 PCM24 编码路径，跳过 `to_fixed_stereo_10ms` 强制 48 kHz；packet 头上报真实采样率与 `frame_duration_ms`；MTU 检查与分片策略 |
| Android 客户端 | 协议握手声明 `supports_hires_pcm24`；播放侧识别 `codec=4` 直接喂 PCM24 → AudioTrack/Oboe |
| Oboe 输出 | 当前硬编码 `ENCODING_PCM_16BIT`，需要扩 `ENCODING_PCM_FLOAT`（24bit packed 在 Android 不直通，统一转 float） |
| EQ DSP | 当前 biquad 是 i16 直通；扩 float 路径，保留现有 i16 路径 |
| UI | `more_page` 新增 "Hi-Res Lossless" 开关，仅在 `high_quality` 模式下可用；显示带宽与设备兼容性提示 |
| 协商 | 客户端不支持时服务端自动回 `Opus`；UI 弹一次性提示"该设备不支持 Hi-Res，已自动回退" |

## 4. 协议变更

### 4.1 codec 枚举

```rust
#[repr(u8)]
pub enum UdpAudioCodecV2 {
    Pcm16 = 1,
    F32 = 2,
    Opus = 3,
    /// Hi-Res passthrough. Big-endian 24-bit signed integer samples,
    /// interleaved L/R. No compression. Sample rate is whatever the
    /// capture device reports — see `UdpAudioHeaderV2.sample_rate`.
    /// Only valid on `v2_header` data plane.
    Pcm24 = 4,
}
```

`UdpAudioPacketV2` payload 布局（Pcm24）：

```
| sample[0].L (3B BE) | sample[0].R (3B BE) | sample[1].L | sample[1].R | ...
```

24-bit BE 是 PCM24 在 wire 上的事实标准。Android 一侧需要重新组装到 `int32` `<<= 8` 后做 32-bit signed 处理。

### 4.2 capability

```rust
pub struct ProtocolCapabilities {
    // ... 现有字段不变 ...
    pub supports_hires_pcm24: bool,
}
```

旧客户端反序列化未知字段不会失败（serde `#[serde(default)]` 已加），所以这是安全的增量。

### 4.3 mode profile

`AudioModeProfile.preferred_codec` 当前是 `AudioCodecPreference`，增加 `Pcm24` 变体后服务端在 `high_quality + supports_hires_pcm24=true` 时下发 `Pcm24`，否则按现有逻辑下发 `Opus`/`Pcm16`。

## 5. 帧大小与 MTU 策略（最关键的工程问题）

### 5.1 数据量级

| 采样率 | 时长 | bytes_per_frame (24bit × 2ch) | UDP payload |
|---|---|---|---|
| 48 kHz | 10 ms | 480 sample × 6 B = **2880 B** | header + 2880 = ~2933 B |
| 48 kHz | 20 ms | 960 × 6 = **5760 B** | ~5813 B |
| 96 kHz | 10 ms | 960 × 6 = **5760 B** | ~5813 B |
| 96 kHz | 20 ms | 1920 × 6 = **11520 B** | ~11573 B |
| 192 kHz | 10 ms | 1920 × 6 = **11520 B** | ~11573 B |
| 192 kHz | 20 ms | 3840 × 6 = **23040 B** | ~23093 B |

### 5.2 LAN MTU 现实

- WiFi 默认 1500 B；多数 AP 不开 jumbo frame
- 单 UDP 包超过 ~1450 B 会触发 IP 层分片（Path MTU Discovery 失败时直接丢包）
- 即使 IP 层分片成功，丢任意一个 fragment 整个 UDP 包都被弃，丢包率呈指数恶化

结论：**所有 PCM24 配置都会超 MTU**，必须在应用层分片。

### 5.3 分片策略

固定 5 ms 子帧粒度：

```
| 1 个原始 v2_header 大帧（10 ms）逻辑上分为 2 个子帧
| 每个子帧自带子帧头（带 chunk_index + total_chunks + frame_seq）
| 单个子帧 ≤ 1400 字节 payload（48 kHz/24bit/2ch/5ms = 1440 B 已超）
```

更稳妥：**3 ms 子帧**

| 采样率 | 3 ms PCM24 stereo | 单子帧 payload |
|---|---|---|
| 48 kHz | 144 × 6 = 864 B | 864 + 子帧头(8) = 872 B ✅ |
| 96 kHz | 288 × 6 = 1728 B | 1728 + 8 = 1736 B ❌ 还是超 |

只能继续切：96 kHz 用 1.5 ms 子帧 / 192 kHz 用 0.75 ms 子帧。但这把 packet rate 抬到 666 / 1333 包每秒，对接收端 jitter buffer 是新的压力。

**建议折中方案**：

- 48 kHz 24bit：5 ms 子帧（packet rate 200/s，1440 B），与现有 v2_header 框架兼容
- 96 kHz 24bit：5 ms 子帧（packet rate 200/s，2880 B → **必须分片为 2 个**）→ 应用层定义 `frag_index/total` 字段
- 192 kHz 24bit：5 ms 子帧（packet rate 200/s，5760 B → 必须分片为 4 个）

### 5.4 v2_header 格式不变

新增的 **应用层分片头** 跟在 `UdpAudioHeaderV2` 之后、payload 之前：

```
struct PcmFragHeader {
    u8 frag_index;        // 0..total-1
    u8 total_frags;       // 1 = 不分片
    u16 frag_payload_size;
    u32 logical_frame_seq; // 同一逻辑帧的所有 frag 共享
}
```

接收端按 `(logical_frame_seq, frag_index)` 重组。任一 frag 缺失则整逻辑帧丢弃（不做部分播放）。

## 6. 重采样

桌面端 WASAPI 在 Hi-Res 模式下不再强制 48 kHz：

- 用户启用 Hi-Res 时，告知 Windows 把 Mix Format 设到 96/192 kHz（或检测当前 Mix Format，若已是高采样率直接用）
- WASAPI loopback `GetMixFormat()` 返回什么就用什么
- Opus 路径仍走现有 `to_fixed_stereo_10ms` 重采样到 48 kHz（这条路径 **必须** 替换为带 anti-aliasing 的 polyphase resampler，否则 96→48 nearest-neighbor 会引入混叠失真）
- PCM24 路径直通，无重采样

**重采样器选型**：`rubato` crate（纯 Rust、Hann 窗 sinc、24-tap 默认）。约 0.3 ms/10ms-frame 的 CPU 开销，可接受。

## 7. Android 端

### 7.1 协议解码

`LasPacket.decode` 识别 `codec=4`（PCM24），按 24bit BE → 32bit signed left-shift 还原：

```kotlin
val sample32 = (b0.toLong() shl 24) or (b1.toLong() shl 16) or (b2.toLong() shl 8)
val sample32Signed = (sample32.toInt() shr 8)  // 32 -> 24-bit signed
```

### 7.2 输出层

`OboeAudioSink::open` 当前硬编码 `oboe::AudioFormat::I16`，扩展为 float：

```cpp
builder.setFormat(use_float ? oboe::AudioFormat::Float : oboe::AudioFormat::I16);
```

PCM24 解码后转 float `[-1.0, 1.0]`：

```kotlin
val sampleFloat = sample32Signed.toFloat() / (1 shl 23).toFloat()
```

OboeAudioSink 在 float 模式下不能复用现有 i16 ring buffer，需要扩展 `PcmRingBuffer` 为模板或新增 `PcmFloatRingBuffer`。

### 7.3 EQ DSP

现有 `BiquadFilter::processSample` 的输入/输出已是 float，只是被夹回 i16 末段。Hi-Res 模式下：

- 输入：float（已经 `[-1, 1]` 归一化）
- 输出：float
- 不做 i16 clamp，直接 `tanh`-style soft clip 防越界

EQ 模块在 Hi-Res 下质量反而比 i16 路径更高。

## 8. UI 改动

`more_page.dart`：

- "音质"区段下新增 toggle "Hi-Res Lossless"
- 仅在当前 mode 是 `high_quality` 时启用；其他 mode 灰显并提示"切到高质量模式后可用"
- 启用时显示提示文本：
  > Hi-Res 直通需要 ≥ 50 Mbps 稳定 LAN，并且服务端已切到 96 kHz Mix Format。
  > 移动数据 / 远场 WiFi 不推荐使用。
  > 设备不支持时会自动回退到 Opus 48 kHz。
- 实际带宽实时显示在 `_AudioQualityStrip` 旁

## 9. 协商与回退

### 9.1 hello → hello_ack

1. Android 在 Hello 中声明 `supports_hires_pcm24=true`（条件：Oboe float 路径可用 + 自动检测设备 native rate ≥ 48 kHz）
2. 服务端在 high_quality 模式 + 用户选择 Hi-Res 时检查客户端能力
3. 若客户端不支持，下发 `Opus` 并在 `hello_ack.message` 写明 `hires_unsupported_falling_back_to_opus`
4. UI 收到该字段后弹一次性 toast 告知用户

### 9.2 运行时切换

用户在 high_quality 下切换 Hi-Res 开关：

1. 客户端发 `set_audio_mode { mode: HighQuality, codec_pref: Pcm24 }`
2. 服务端切换 encoder：发送 `config_changed + discontinuity` flag
3. 客户端清空 jitter buffer + 重建 Oboe（float 模式）
4. 服务端 v2_header 改 codec 字段为 4

切换瞬间会有 ~300 ms 静音，用户感知为"明显的切换瞬间"。

## 10. 性能与带宽

| 配置 | 带宽 | CPU 桌面 | CPU Android |
|---|---|---|---|
| Opus 48k 96kbps | ~96 kbps | low | low |
| **PCM24 48k stereo** | **~2.3 Mbps** | **negligible** | **negligible** |
| **PCM24 96k stereo** | **~4.6 Mbps** | low (无重采样) | low |
| **PCM24 192k stereo** | **~9.2 Mbps** | low | low |

LAN 通常 100~1000 Mbps，不构成压力。WiFi 5/6 干净环境下 192 kHz 也跑得动；干扰多的 2.4 GHz 可能丢包。

## 11. 验收门控

发布前必须完成：

- [ ] cargo fmt + cargo check + cargo clippy 全 workspace 通过
- [ ] 96 + Rust 现有测试全部通过 + 新增 PCM24 wire format round-trip + frag header round-trip 测试
- [ ] flutter analyze + flutter test 通过
- [ ] gradlew assembleDebug 通过
- [ ] **真机端到端验收**：
  - 桌面 Mix Format = 48 kHz，PCM24 启用 → 听感无杂音 + 无 underrun
  - 桌面 Mix Format = 96 kHz，PCM24 启用 → 同上
  - 桌面 Mix Format = 192 kHz，PCM24 启用 → 同上 + 带宽监控显示 ~9 Mbps
  - 模式切换 Opus → PCM24 → Opus 各 30 秒，jitter buffer 不发散
  - WiFi 弱信号下 PCM24 优雅降级（packet loss > 5% 时建议自动回 Opus）
- [ ] **回退验收**：客户端不支持 Hi-Res 时 hello_ack 流程正确
- [ ] 文档：`protocol.md` 更新；`README` Quick Start 增加 Hi-Res 段落；`CHANGELOG` 写明所有改动

## 12. 实施分阶段

| Phase | 内容 | 估时 |
|---|---|---|
| 6.1 | 协议层加 PCM24 codec + capability + frag header | 0.5 天 |
| 6.2 | rubato 重采样器替换 nearest-neighbor（独立可发布修复） | 0.5 天 |
| 6.3 | 桌面采集 + PCM24 编码路径 + 应用层分片 | 1 天 |
| 6.4 | Android 解码 + frag 重组 + Oboe float 路径 | 1.5 天 |
| 6.5 | EQ DSP 扩 float | 0.5 天 |
| 6.6 | UI + 协商 + 回退 | 1 天 |
| 6.7 | 测试 + 真机验证 + 文档 | 1 天 |
| **合计** | | **6 天** |

各阶段都可独立 PR、独立 cargo test，避免"6 天 big bang"风险。

## 13. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| Oboe float 路径在低端机不稳定 | Hi-Res 不可用 | 启动时探测，失败回 i16 + Opus |
| 应用层分片实现 bug 导致主路径回归 | 灾难 | 默认关闭，CLI 显式 `--no-pcm24-frag` 可关；现有 Opus 路径不依赖 frag header |
| 用户在不该启用的设备上启用 | 体验差 | UI 文案明确 + 自动检测 supports_hires_pcm24 |
| WiFi 弱信号下持续丢包 | 听觉断裂 | watchdog 检测到 packet loss > 5% 时建议自动回 Opus，弹 toast 提醒用户 |
| Phase 4 watchdog 在 PCM24 路径下错误降级 | 误降级 | tier_encoder_profile 增加 PCM24 分支，Red 强制回 Opus 48k 而非降码率 |

## 14. 待评审决策点

1. ❓ **支持 192 kHz 是否过头？** 96 kHz 已经覆盖绝大部分 Hi-Res 流媒体（Tidal Master、Apple Hi-Res Lossless 都是 96 kHz/24bit）。砍掉 192 减少 50% 复杂度。
2. ❓ **应用层分片头放在 `v2_header` 之后还是合并进 `v2_header.reserved`？** 前者向后兼容，后者更紧凑但破坏现有 wire format。
3. ❓ **是否需要 SRC 旁路检测？** 用户机器 Mix Format 仍是 48 kHz 时启用 Hi-Res 没意义（采集源就是 48 kHz）。是否在 UI 显示警告？
4. ❓ **Opus 路径的重采样器升级（Phase 6.2）单独发布还是和 Hi-Res 绑定？** 升级会让 Opus 路径在非标准采样率桌面上音质提升，是普惠改动。建议拆 v1.9.4 单独发。

---

## 14.1 评审决策（2026-05-14 已定案）

- **Q1 → 砍 192 kHz**。仅支持 96 kHz/24bit。覆盖 Tidal Master / Apple Hi-Res Lossless / Qobuz Studio Premier 主流 Hi-Res 流媒体；带宽上限 ~4.6 Mbps；分片从 4-frag 减到 2-frag。本期不做 192/352/768。
- **Q2 → 合并进 `v2_header.reserved` + protocol_version=3**。`reserved` 字段重用为 `frag_index(8) | total_frags(8)`；`v2_header` 末尾另加 `logical_seq u32`；保留 `magic = LAV2`，但 `protocol_version` 字段从 `2` 提升到 `3`。Phase 3+4+5 在 v2 上的所有现有功能在 v3 下保持兼容。旧服务端拿到 v3 包会因 `UnsupportedVersion` 报错并丢弃 — 这是预期：**v3 packet 只在双方都协商了 `supports_hires_pcm24=true` 时才发送**，旧客户端永远不会收到 v3。
- **Q3 → 检测 + 提示但不拦截**。Android UI 启用 Hi-Res 时，`server_info.recommended_connection` 字段附带 `mix_format_hz`；若桌面端报告 `mix_format_hz < 96000`，Android 弹绿色 toast 提示 "桌面 mix format 是 48 kHz，请去 Windows 声音设置改为 96 kHz/24bit 才能真正听到 Hi-Res 区别"。Hi-Res 仍然启用，让用户自己决定。
- **Q4 → 合并进 v1.10.0**。Phase 6.2（rubato sinc 重采样器）作为 Phase 6.1 的子任务一起做，统一发 v1.10.0。理由：现有 nearest-neighbor 在 48→48 是 no-op，对 99% 用户没影响，单独发 patch 收益小；和 Hi-Res 一起发能形成 "音质升级" 完整故事。

## 14.2 决策对实施计划的影响

| 原 Phase | 改动 |
|---|---|
| 6.1 协议层 | 同时 bump `PROTOCOL_VERSION_V2` → `PROTOCOL_VERSION_V3=3`；`UdpAudioHeaderV2` 改名 `UdpAudioHeaderV3`，`reserved` 拆分 + 末尾加 `logical_seq`；`UdpAudioCodecV2::Pcm24=4` 仍按计划加 |
| 6.2 重采样器 | 改为 v1.10.0 内子任务，不单独发 v1.9.5 |
| 6.3 桌面采集 | 砍 192 kHz 分支；只处理 48 / 96 kHz 两档 |
| 6.4 Android 解码 | 砍 192 kHz；frag 重组上限 2 |
| 6.7 测试 | 减少一档真机验证（只要 48 + 96 两档） |

新估时：~5 工作日（原 6 减 1 因为砍 192）。


## 15. 决策门

评审通过后才进入实现阶段。评审重点：

- 第 5 节分片策略是否能在丢包率 1-5% 区间稳定
- 第 9 节运行时切换是否能避免主路径回归
- 第 13 节风险表是否有遗漏

---

**作者**：codex@2026-05-14
**状态**：DRAFT - 待评审
**下一步**：人工评审 → 决策第 14 节 4 个待定项 → 决定是否进入 Phase 6.1
