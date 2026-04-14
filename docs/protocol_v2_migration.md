# Protocol v2 Migration Strategy

## 当前状态

- 运行主路径：legacy/v1（PCM + `LAS1`）
- Protocol v2 状态：部分启用（控制面联动已接通，数据面进入双栈灰度准备）

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
4. 默认路径切换
   - 压测通过后将 v2 设为默认，并保留回滚开关。

## 关键风险

- 两端版本不一致导致协商失败。
- 模式切换时参数未同步，导致播放抖动。
- v2 header 提前启用导致旧客户端无法解析。

## 本轮已落点

- Rust 协议层：v2 控制消息和 UDP header 编解码。
- 服务端会话层：`hello/hello_ack`、`client_info/server_info`、`set_audio_mode/audio_mode_changed` 已进入运行流转。
- Android 后台播放主链路：已发送 v2 `hello/client_info`，并处理 `hello_ack/server_info/audio_mode_changed`。
- 服务端持有运行时 `current_audio_mode`，Windows 桌面与 Android 均可显示当前模式语义。
- 数据面双栈：
  - 服务端支持配置 `legacy_las1` / `v2_header` 发送格式（默认 `legacy_las1`）。
  - 客户端接收侧支持 `LAS1/LAV2` 双栈识别。
  - `config_changed/discontinuity` 已有最小重同步逻辑（清 jitter、重建 AudioTrack）。
  - 灰度保护：`windows_loopback + v2_header` 仅在显式开关 `--allow-loopback-v2-header-gray` 下启用；默认自动回落 `legacy_las1`。
- 数据面默认仍为 legacy `LAS1`（未切 UDP v2 默认发送格式）。

## 本轮验收结论（2026-04-14）

- synthetic + v2_header：
  - 本地灰度：通过（LAV2 收包、模式切换、flags 与重同步链路均可验证）。
  - 真机灰度：通过（真实 Android 设备已完成连接、播放、模式切换与指标采样）。
- loopback + v2_header：
  - 已完成一轮受控真机灰度，结论为“当前未通过”。
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

- 结论：**loopback + v2_header 当前未通过**
- 开关规则：
  - 默认主路径仍是 `legacy_las1`
  - 只有 `--audio-source windows_loopback --data-plane v2_header --allow-loopback-v2-header-gray` 才会启用 loopback 的 `LAV2`
  - 未带显式开关时会自动回退到 `legacy_las1`
- 回滚路径：
  - `windows_loopback + legacy_las1`
  - `synthetic + v2_header`
- 真机观测：
  - Android 真机已建立真实连接，服务端确认 loopback 路径实际发送 `LAV2`
  - 客户端基线指标停留在 `Playback=buffering`，`buffered_ms=0~50`，`jitter_underrun=23 -> 30`
  - `rx_frames_per_sec` 仅约 `5.9 ~ 7.0`，明显低于 synthetic 阶段
  - 服务端 `capture_no_packet_count`、`capture_underruns` 持续上升，`capture_last_peak/capture_last_rms` 大多数时间近似 `0`
- 模式切换：
  - 已确认 `balanced -> low_latency` 的 `config_changed/discontinuity`
  - 后续切换过程中真机 `adb` 掉线，完整四段切换未补齐
- 主阻塞点：
  - **Windows 采集端**
  - 协议 v2 数据面本身已经接通，但 loopback 采集输出帧速率与内容质量不足，导致 Android 持续缓冲
