# TODO / Stub Tracking

## Audio Capture (Windows)

- [x] 实测可用：Windows -> Android 已可连接并出声（单轮真机验收通过）
- [ ] 稳定性优化：多设备实机验证（不同声卡/驱动/采样率）
- [ ] 稳定性优化：长时间稳定性验证与时钟漂移处理
- [ ] 稳定性优化：扩展 mix format 支持（更多 extensible 分支）
- [ ] 稳定性优化：完整重采样/重通道策略

## Android Playback

- [x] 实测可用：Android 真机可连接并稳定出声（主观可用，无严重阻塞）
- [x] synthetic 基线链路可用于稳定对照与排障
- [x] windows_loopback 已实测可出声（当前结论：可用但仍需稳定性优化）
- [x] v8 修复：jitter buffer 空队列 underrun 后不再继续推进 expected sequence，避免正常新包被误判为 late
- [x] v14 修复：服务端同一客户端 IP 仅保留一个活跃 UDP 流，避免重连后重复推流造成缓冲抖动
- [x] v15 修复：Android 播放预算改为高精度时钟，减少长期少消费导致的缓冲堆积
- [x] v16 增加启动版本日志（`ui_build ...`）用于确认设备实际运行包版本
- [x] v17 增加 WS 断开后 UDP 推流 30s 保活窗口，降低控制通道瞬断导致的立即静音
- [x] v18 完成 UI 信息架构重排（Connection/Playback/Debug）与单主按钮交互
- [x] v19 支持中英文切换与系统语言默认（zh -> 中文，other -> English）
- [x] v20 Android 发现链路增加 MulticastLock 支持，提升 UDP 广播接收稳定性
- [x] v21 增加局域网主动探测兜底（广播失败时仍可发现 39991 服务）
- [x] v22 扫描结果命名优化 + 最近成功连接置顶
- [x] v23 连接体验收尾：Recent 标记、快速连接卡片、空列表发现引导、扫描 loading 提示、首次使用轻量提示
- [x] v24 首屏产品化收敛：顶部摘要、单主 CTA、端口信息下沉、调试区双列栅格
- [x] v25 新增 Media3 后台播放服务骨架（前台通知 + MediaSessionService + 命令/事件通道）
- [x] v25 新增渐进迁移开关 `kUseBackgroundPlaybackService`（灰度阶段默认 false，legacy 路径保留）
- [x] v26 切换后台服务为默认链路（`kUseBackgroundPlaybackService=true`）
- [x] v26 新增后台保活基础能力：`WAKE_LOCK` + `PARTIAL_WAKE_LOCK` + `WifiLock`
- [x] v27 修复后台服务事件线程崩溃（EventChannel 回调统一主线程）
- [x] v28 修复后台服务明文策略拦截（允许 LAN `ws://` cleartext）
- [x] v29 修复后台重连竞态（去重重连 + 过期回调隔离）
- [x] 首次使用提示改为持久化只提示一次（不再因 App 进程重启重复弹出）
- [ ] 稳定性优化：v26 后台链路实机验收（锁屏/切后台/熄屏连续播放，多机型）
- [ ] 稳定性优化：多机型 AudioTrack 稳定性与延迟调优（当前已实现基础可播放路径）
- [ ] 稳定性优化：jitter buffer 自适应策略（当前固定起播缓冲）
- [ ] 稳定性优化：播放线程优先级/抗抖动增强

## Opus

- [ ] replace PCM passthrough with real Opus encoder/decoder

## Protocol Evolution (v2)

- [x] Protocol v2 草案文档（控制面/数据面/capabilities/迁移策略）
- [x] Rust 协议结构骨架（AudioMode、Capabilities、ControlMessageV2、UdpAudioHeaderV2）
- [x] 控制面联动已接通：`hello/hello_ack + client_info/server_info + set_audio_mode/audio_mode_changed`
- [x] 数据面双栈灰度准备：服务端可选 `legacy_las1/v2_header`（默认 legacy）+ 客户端 `LAS1/LAV2` 双栈识别
- [x] config_changed/discontinuity 最小处理：服务端打 flag + 客户端最小重同步
- [x] loopback + v2_header 显式灰度开关：默认关闭，未带 `--allow-loopback-v2-header-gray` 时自动回退 `legacy_las1`
- [x] synthetic + v2_header 本地灰度验收（LAV2 识别、模式切换、flags 与重同步联调通过）
- [x] synthetic + v2_header 真机灰度验收（真实 Android 设备完成播放、模式切换、指标采样，结论：通过）
- [x] 双端模式状态联动：服务端持有 `current_audio_mode`，Android + Windows 可显示并同步（默认 `balanced`）
- [ ] 下一阶段：修复 Windows loopback 采集节奏后，重新执行 `loopback + v2_header` 完整真机灰度（当前结论：未通过）
- [ ] 灰度启用：双端协商后按连接动态切换到 v2 数据面 header（当前仍以配置开关为主）
- [ ] 全量启用：默认路径切换到 v2，并保留 v1 回退策略

## loopback + v2_header 小流量灰度结论

- 结论：当前未通过
- 已到位：
  - `windows_loopback + v2_header` 仅在显式灰度开关下允许启用
  - 默认主路径仍是 `legacy_las1`
  - 可回滚到 `windows_loopback + legacy_las1` 与 `synthetic + v2_header`
- 主要问题：
  - Android 真机已连通，但客户端长期停留在 `buffering`
  - `rx_frames_per_sec` 仅约 `6 fps`，`jitter_underrun` 持续上升
  - 服务端 Windows loopback 采集端出现大量 `no_packet / underrun`，`capture_last_peak/rms` 大多数时间近似 `0`
- 主阻塞点：
  - Windows 采集端
- 额外说明：
  - `balanced -> low_latency` 的 `config_changed/discontinuity` 已打通
  - 后续模式切换阶段被真机 `adb` 掉线打断，仍需回补

## Productization

- [x] Tauri 桌面客户端首版可用 UI + 服务状态控制（启动/停止/重启、音频源切换、连接信息、折叠调试区、中英双语）
- [ ] installer / firewall guidance
- [ ] structured logs export
- [ ] 桌面端连接二维码（当前仅文本地址复制）
- [ ] 桌面端会话详情深化（当前仅连接数 + 最近连接设备）
