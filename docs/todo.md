# TODO / Stub Tracking

## Automation Baseline

- 当前版本（短版本）：`1.1`
- [x] 仓库级规则文件：`AGENTS.md`
- [x] 发布规则文档：`docs/RELEASE_POLICY.md`
- [x] 本地验证脚本：`scripts/validate_local.ps1`
- [x] 版本递增脚本：`scripts/bump_version.ps1`
- [x] 发布入口脚本：`scripts/release.ps1`
- [x] release 打包脚本：`scripts/package_release.ps1`（Android release split APK + Windows 单 exe）
- [x] 统一 CI：`.github/workflows/ci.yml`
- [x] Release 工作流：`.github/workflows/release.yml`（构建并附加 Windows exe、Android release APK、SHA256）
- [ ] 下一步：验证首个 `v1.0` 之后的自动递增发布（`1.1`）完整闭环

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
- [x] 后台恢复增强：保存最近成功播放目标，`START_STICKY + AlarmManager` 在任务移除/服务回收后尝试恢复连接
- [x] 断线重连语义收敛：WebSocket transient failure 进入 reconnecting，不再先发布致命 error
- [x] 自动重连边界收敛：连接异常中断后最多自动重连 3 次；重开 App 时尝试恢复上一次成功的推流服务器
- [x] 自动重连真机验收：`synthetic + v2_header + opus` 下验证异常断开最多 3 次重连，重开 App 可恢复上次服务器
- [x] release 体积收敛：release 构建启用 R8/resource shrink，发布 APK 按 ABI 拆分
- [x] V2 模式策略接入：`low_latency/balanced/high_quality` 已映射到 start/max buffer、batch、drop threshold、后端偏好
- [x] Android 产品诊断入口：新增连接帮助折叠区（同网段、AP isolation、扫描/手动地址、USB、后台电池优化）
- [x] 首次使用提示改为持久化只提示一次（不再因 App 进程重启重复弹出）
- [ ] 稳定性优化：v26 后台链路实机验收（锁屏/切后台/熄屏连续播放，多机型）
- [ ] 稳定性优化：多机型 AudioTrack 稳定性与延迟调优（本轮已切 Builder + LOW_LATENCY + reported latency 诊断，仍待真机确认 <=40ms）
- [ ] 稳定性优化：jitter buffer 自适应策略（当前固定起播缓冲）
- [ ] 稳定性优化：播放线程优先级/抗抖动增强

## Opus

- [x] 工程可接入状态：协议枚举、capabilities、服务端 `--codec opus`（兼容旧 `opus_experimental`）、桌面入口已具备
- [x] 回退策略：当有效数据面不是 `v2_header` 时，Opus 请求仍自动回退 PCM16，不破坏可出声主路径
- [x] 稳定链路：服务端标准 libopus 编码 + Android `libopus` JNI 解码已接入（推荐路径 `v2_header + opus`）
- [x] PLC 回退：Android JNI decode 失败时改走 libopus PLC concealment，不直接 silence
- [x] 5 分钟压力测试：synthetic + Opus 固定 20ms 帧连续编码通过，`p99 encode ~= 0.509 ms`，channel-full drop rate `0.000000`
- [ ] 稳定性验证：Opus 与 PCM16 的真机延迟、CPU、丢包恢复对比

## Protocol Evolution (v2)

- [x] Protocol v2 草案文档（控制面/数据面/capabilities/迁移策略）
- [x] Rust 协议结构骨架（AudioMode、Capabilities、ControlMessageV2、UdpAudioHeaderV2）
- [x] 控制面联动已接通：`hello/hello_ack + client_info/server_info + set_audio_mode/audio_mode_changed`
- [x] V2 低延迟产品模型：连接、发送、播放、协议、诊断五类能力已写入 README/roadmap/protocol
- [x] `AudioModeProfile` 策略系统：协议层 + 服务端 + Android + 桌面端语义一致
- [x] capabilities 扩展：fast path、stable AudioTrack、USB tethering、USB direct future、Opus
- [x] 数据面双栈准备：服务端可选 `legacy_las1/v2_header`（桌面默认 `v2_header`）+ 客户端 `LAS1/LAV2` 双栈识别
- [x] config_changed/discontinuity 最小处理：服务端打 flag + 客户端最小重同步
- [x] 默认路径切换：desktop 默认改为 `windows_loopback + v2_header + opus`，`legacy_las1 + pcm16` 保留为显式回滚
- [x] synthetic + v2_header 本地灰度验收（LAV2 识别、模式切换、flags 与重同步联调通过）
- [x] synthetic + v2_header 真机灰度验收（真实 Android 设备完成播放、模式切换、指标采样，结论：通过）
- [x] 双端模式状态联动：服务端持有 `current_audio_mode`，Android + Windows 可显示并同步（默认 `balanced`）
- [x] `windows_loopback + v2_header` 已晋升为推荐默认路径；安全模式可显式回滚到 `legacy_las1 + pcm16`
- [ ] 下一阶段：稳定性优化（模式切换后缓冲峰值与 late frame 累积）
- [ ] 下一阶段：USB tethering 低延迟样本验收（Wi-Fi 与 USB 样本分开记录）
- [ ] 下一阶段：Opus loopback 真机长稳验证（默认已切换，发布前仍需补足实测样本）
- [ ] 灰度启用：双端协商后按连接动态切换到 v2 数据面 header（当前仍以配置开关为主）
- [ ] 全量启用：默认路径切换到 v2，并保留 v1 回退策略

## v1.2 Phase 1 记录

- 日期：`2026-04-21`
- 结论：Phase 1 代码路径已完成，推荐默认路径已切到 `windows_loopback + v2_header + opus`
- 已完成：
  - Opus 编码固定为 20ms / 960 samples per channel，对齐层已加入
  - Android Opus decode 失败时走 PLC concealment
  - `desktop_headless --help`、Tauri UI、README、协议文档已统一到 `opus`
  - Tauri UI 新增“安全模式”回滚按钮，回滚到 `legacy_las1 + pcm16`
- 未完成：
  - Android 真机 `balanced` 延迟 <=40ms 复核
  - 多机型 / 长时间 / USB 样本补齐
- 发布判断：暂不发版，继续灰度 / 继续修复

## loopback + v2_header 小流量灰度结论

- 结论：已从灰度提升为推荐路径，回滚路径保留
- 已到位：
  - 默认推荐路径：`windows_loopback + v2_header + opus`
  - 可回滚到 `windows_loopback + legacy_las1 + pcm16` 与 `synthetic + v2_header + pcm16`
- 本轮验收（2026-04-15）：
  - Android 真机连续播放 >2 分钟，`Playback=playing`
  - 模式切换已覆盖：`balanced -> low_latency -> high_quality -> balanced`
  - 切换后累计：`cfg_changed=3`, `discontinuity=4`
  - `rx_frames_per_sec≈99~101`，`audio_track_write_frames_per_sec≈99~101`
- 当前风险：
  - 模式切换后 `buffered_ms` 峰值可达 300
  - `dropped_late_frames` 可累积（本轮到 104）
  - 发布前仍需补齐 Android 真机 latency / 多机型样本
- 额外说明：
  - 本轮日志未出现 `capture source is not started`
  - 回滚路径保持可用：`legacy_las1` / `synthetic + v2_header`

## Productization

- [x] Tauri 桌面客户端首版可用 UI + 服务状态控制（启动/停止/重启、音频源切换、连接信息、折叠调试区、中英双语）
- [x] Windows release 交付路径收敛为单 exe（GitHub Actions 与本地 `package_release.ps1` 均只产出 exe）
- [x] 桌面端 V2 产品状态展示：协议路径、模式策略、codec、灰度状态、推荐连接方式
- [x] Android 端 V2 产品状态展示：连接来源、协议路径、播放后端、灰度路径、模式策略
- [x] USB tethering 正式纳入低延迟推荐路径（当前为路线/文案/状态位，不是 USB direct 实现）
- [ ] installer / firewall guidance
- [ ] structured logs export
- [ ] 桌面端连接二维码（当前仅文本地址复制）
- [ ] 桌面端会话详情深化（当前仅连接数 + 最近连接设备）
- [ ] 自动诊断报告：同网段/AP isolation/后台电池优化/延迟模式建议
