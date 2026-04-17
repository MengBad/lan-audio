# Protocol v2 Migration Strategy

## 当前状态

- 运行主路径：legacy/v1（PCM + `LAS1`）
- Protocol v2 状态：部分启用（控制面联动已接通，数据面双栈灰度可用，低延迟模式策略已接入）
- 产品定位：V2 是低延迟音频传输升级，不只是协议替换。
- 默认策略：安全优先，v2/Opus/loopback 灰度都必须显式启用并可回滚。

## 兼容原则

1. v1 与 v2 可并存。
2. 控制面优先协商；协商失败回落 v1。
3. 未识别字段忽略，未识别消息类型记录日志后忽略。
4. 数据面 header 切换必须建立在协商成功基础上。

## 推荐迁移顺序

1. 控制面先行
   - 启用 `hello/hello_ack` 与 capabilities。
2. 模式状态接入
   - 打通 `set_audio_mode/audio_mode_changed`。
3. 数据面灰度
   - 按连接启用 UDP v2 header，保留 v1 fallback。
4. 播放策略落地
   - 让 `AudioModeProfile` 驱动 start buffer、max buffer、batch、drop threshold 和后端偏好。
5. Codec 实验
   - 先接 `opus_experimental` 的配置与 fallback，再接真实编码/解码。
6. 连接路径优化
   - USB tethering 作为低延迟推荐路径进入验收矩阵，USB direct 仅保留 future capability。
7. 默认路径切换
   - 压测通过后将 v2 设为默认，并保留回滚开关。

## 关键风险

- 两端版本不一致导致协商失败。
- 模式切换时参数未同步，导致播放抖动。
- v2 header 提前启用导致旧客户端无法解析。
- Opus 实验链路若没有 PCM16 fallback，会破坏当前可出声路径。
- USB/网络路径差异会掩盖播放端问题，必须分开记录 Wi-Fi 与 USB 样本。

## 本轮已落点

- Rust 协议层：v2 控制消息和 UDP header 编解码。
- 服务端会话层：`hello/hello_ack`、`client_info/server_info`、`set_audio_mode/audio_mode_changed` 已进入运行流转。
- Android 后台播放主链路：已发送 v2 `hello/client_info`，并处理 `hello_ack/server_info/audio_mode_changed`。
- 服务端持有运行时 `current_audio_mode`，Windows 桌面与 Android 均可显示当前模式语义。
- `AudioModeProfile` 已把三档模式扩展为策略系统：
  - `low_latency`: 40/180ms buffer、batch=1、drop_threshold=140ms、优先低延迟路径。
  - `balanced`: 60/300ms buffer、batch=2、drop_threshold=220ms、默认稳定策略。
  - `high_quality`: 120/500ms buffer、batch=3、drop_threshold=420ms、优先平滑与稳定后端。
- Opus 已进入工程可接入状态：
  - 协议枚举、capabilities、服务端 `--codec opus_experimental`、桌面实验入口已具备。
  - 当前仍未启用真实 Opus 编解码，选择 Opus 会回退 PCM16。
- USB 已进入 V2 路线：
  - 当前推荐 USB tethering 作为低延迟连接方式。
  - USB direct 只作为 `supports_usb_direct_future` 预留，不在当前主路径启用。
- 产品诊断入口：
  - Android 增加连接帮助折叠区，覆盖同网段、访客网络/AP isolation、扫描/手动地址、USB、后台电池优化。
  - Windows 桌面端展示协议路径、模式策略、codec、灰度状态和推荐连接方式。
- 数据面双栈：
  - 服务端支持配置 `legacy_las1` / `v2_header` 发送格式（默认 `legacy_las1`）。
  - 客户端接收侧支持 `LAS1/LAV2` 双栈识别。
  - `config_changed/discontinuity` 已有最小重同步逻辑（清 jitter、重建 AudioTrack）。
  - 灰度保护：`windows_loopback + v2_header` 仅在显式开关 `--allow-loopback-v2-header-gray` 下启用；默认自动回落 `legacy_las1`。
- 数据面默认仍为 legacy `LAS1`（未切 UDP v2 默认发送格式）。
- 默认 codec 仍为 PCM16（未启用真实 Opus）。

## 本轮验收结论（2026-04-14）

- synthetic + v2_header：
  - 本地灰度：通过（LAV2 收包、模式切换、flags 与重同步链路均可验证）。
  - 真机灰度：通过（真实 Android 设备已完成连接、播放、模式切换与指标采样）。
- loopback + v2_header：
  - 已完成一轮受控真机灰度，结论为“可播但暂不稳定”。
  - 默认主路径仍保持 `legacy_las1`，且继续保留显式灰度开关与回滚路径。

## v2 synthetic 真机验收结论

- 结论：**synthetic + v2_header 真机验收通过**
- 真机：`5391d451 / Xiaomi 24129PN74C`
- 服务端配置：`synthetic + --data-plane v2_header`
- 模式切换序列：`balanced -> low_latency -> high_quality -> balanced`
- 服务端验证：
  - 已记录三次 `audio mode updated by client`
  - 已记录三次 `audio mode changed; mark config_changed/discontinuity in outgoing packet`
- Android dump 指标：
  - `after_low_latency`: `Playback=playing`, `buffered_ms=50`, `jitter_underrun=0`, `dropped_late_frames=0/0`, `silence_fill_count=0`, `rx_frames_per_sec=100.0`, `audio_track_write_frames_per_sec=100.0`, `cfg_changed=1`, `discontinuity=2`
  - `after_high_quality`: `Playback=playing`, `buffered_ms=50`, `jitter_underrun=0`, `dropped_late_frames=0/0`, `silence_fill_count=0`, `rx_frames_per_sec=104.5`, `audio_track_write_frames_per_sec=99.5`, `cfg_changed=2`, `discontinuity=3`
  - `after_balanced_final`: `Playback=playing`, `buffered_ms=70`, `jitter_underrun=0`, `dropped_late_frames=0/0`, `silence_fill_count=0`, `rx_frames_per_sec=99.0`, `audio_track_write_frames_per_sec=99.0`, `cfg_changed=3`, `discontinuity=4`
- 观察：
  - 模式切换后客户端进入最小重同步并快速恢复 `playing`
  - 未见长时间 buffering、突然没声或明显异常波动
- 下一阶段建议：
  - 可以进入 `loopback + v2_header` 小流量灰度，但当前轮次不放开、不切默认

## loopback + v2_header 小流量灰度结论

- 结论：**loopback + v2_header 可播（真机小流量灰度通过），但暂不稳定**
- 开关规则：
  - 默认主路径仍是 `legacy_las1`
  - 只有 `--audio-source windows_loopback --data-plane v2_header --allow-loopback-v2-header-gray` 才会启用 loopback 的 `LAV2`
  - 未带显式开关时会自动回退到 `legacy_las1`
- 回滚路径：
  - `windows_loopback + legacy_las1`
  - `synthetic + v2_header`
- 真机观测：
  - Android 真机连续播放超过 2 分钟，`Playback=playing`
  - `mock_android_client` 收包确认 `LAV2`（`v2=1207, v1=0`）
  - Android 关键指标：`rx_frames_per_sec≈99~101`、`audio_track_write_frames_per_sec≈99~101`
  - 切换后指标累计：`cfg_changed=3`, `discontinuity=4`
  - 本轮未出现 `capture source is not started`
- 模式切换：
  - 已覆盖 `balanced -> low_latency -> high_quality -> balanced`
  - 服务端打出 `config_changed/discontinuity`，Android 侧执行 `udp_config_changed_resync` + `init AudioTrack`
- 主阻塞点：
  - **模式切换重同步 + Android 播放缓冲策略**
  - 表现为切换后 `buffered_ms` 峰值上探到 300、`dropped_late_frames` 累积（本轮到 104）

## 发布判断

- 当前 V2 可继续进入下一阶段，但不满足正式 Release 条件。
- 原因：
  - `loopback + v2_header` 仍暂不稳定。
  - Opus 只有配置/协议入口，没有真实编码/解码。
  - USB tethering 尚未做本项目真机验收。
  - 本轮未要求手机真机验证，新增低延迟策略只基于本地验证与代码审查。
