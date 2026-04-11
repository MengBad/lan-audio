# TODO / Stub Tracking

## Audio Capture (Windows)

- [ ] 多设备实机验证（不同声卡/驱动/采样率）
- [ ] 长时间稳定性验证与时钟漂移处理
- [ ] 扩展 mix format 支持（更多 extensible 分支）
- [ ] 完整重采样/重通道策略

## Android Playback

- [ ] v7 诊断：用 synthetic 正弦音确认播放队列是否稳定（目标 `rx_frames_per_sec ~= 100`、`audio_track_write_frames_per_sec ~= 100`、`buffered_ms` 小范围波动）
- [ ] v7 诊断：若 synthetic 稳定，再切 `windows_loopback` 排查 Windows 采集/PCM16 封包/时钟漂移
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
- [ ] 多机型 AudioTrack 稳定性与延迟调优（当前已实现基础可播放路径）
- [ ] jitter buffer 自适应策略（当前固定起播缓冲）
- [ ] 播放线程优先级/抗抖动增强

## Opus

- [ ] replace PCM passthrough with real Opus encoder/decoder

## Productization

- [ ] Tauri production UI + status controls
- [ ] installer / firewall guidance
- [ ] structured logs export
