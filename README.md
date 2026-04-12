# LAN Phone Speaker MVP (Windows -> Android)

当前仓库已可在本机进入“可构建、可启动、可联调准备”状态，技术路线保持不变：Rust + Tauri + Flutter + UDP + WebSocket（当前主路径 PCM 直通）。
Windows 端当前主入口已切换为 Tauri 桌面客户端，`desktop_headless` 保留用于调试与回归。

## 稳定出声结论

- 当前结论：**可出声但暂不稳定**（已达到可用级别，尚未达到“长时稳定无波动”级别）。
- 验证方式：
  - 真机验证：Android 已可成功连接 Windows 端并连续播放，主观听感“可用、无严重阻塞”。
  - synthetic：已用于链路对照（固定 48k/stereo/10ms 帧），作为稳定参考基线。
  - windows_loopback：已实机验证可出声；历史日志中可见少量采集侧告警（如 `capture source is not started`），提示仍有偶发波动空间。
- 已知限制：
  - 仍可能出现偶发 jitter / buffer 波动与短时抖动（不同机型、不同声卡驱动下概率不同）。
  - 当前结论基于“可用性验收”与单轮真机验证，不等于多机型长稳压测通过。
- 推荐使用方式：
  - 优先在稳定 Wi-Fi（同一局域网，尽量 5GHz）环境使用。
  - 保持 `48kHz / stereo` 默认配置（当前主路径即此配置）。
  - 首次排障建议先用 `synthetic` 验证链路，再切换到 `windows_loopback` 验证系统音频采集。
  - 若出现波动，优先观察 Android 端 `buffered_ms / underrun` 与服务端 capture 状态。

## 本机实测状态（2026-04-12, Asia/Shanghai）

### 1) 已修复并验证通过

- 环境检查：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1` 通过
  - `cargo --version` / `rustup --version` / `flutter --version` / `adb version` / `java -version` 均可执行
- Rust workspace：
  - `cargo metadata --no-deps` 成功
- Windows 桌面客户端（Tauri）：
  - `cargo check -p lan_audio_desktop` 成功
  - `cargo tauri build` 成功
  - 产物：`target/release/bundle/msi/` 与 `target/release/bundle/nsis/`
- 服务端可执行目标：
  - `desktop_headless` 已构建（`target/debug/desktop_headless.exe`）
  - `scripts/run_server_headless.ps1` 可拉起进程（服务是常驻进程）
- Android 构建：
  - `scripts/run_android_gradle.ps1` 成功（`assembleDebug`）
  - `scripts/run_android_client.ps1 -BuildApk` 成功
  - APK 产物：`apps/android_flutter/build/app/outputs/flutter-apk/app-debug.apk`

### 2) 已修复但当前机器仍受外部条件限制

- `adb devices` 当前无在线设备，尚未在这台机器完成“安装到真机并实际出声”验收。
- Android 构建期间存在 AGP/Kotlin 版本即将弃用警告，不影响当前 debug 构建。

### 3) 仍需你手动安装的外部工具

- 当前无需新增手动安装清单（核心工具链已补齐）。
- 若后续要做真机验收，请手动确保：
  - 手机已开启开发者模式/USB 调试
  - USB 驱动可用并在 `adb devices` 中出现 `device`

## 可执行目标与命令

### 服务端（Windows）

- 桌面客户端（推荐，普通用户入口）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_desktop_client.ps1
```

- 打包构建（Tauri）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_desktop_client.ps1
```

- 等价命令（在 `apps/desktop/src-tauri`）：

```powershell
cargo tauri dev
cargo tauri build
```

- Tauri 打包产物（默认）位于：
  - `target/release/bundle/`
  - 可直接双击安装：
    - `target/release/bundle/nsis/LAN Audio Desktop Client_0.1.0_x64-setup.exe`
    - `target/release/bundle/msi/LAN Audio Desktop Client_0.1.0_x64_en-US.msi`

- 目标：`desktop_headless`
- 脚本启动：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic
```

- 等价命令：

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic
```

可选参数（脚本）：

- `-AudioSource windows_loopback`
- `-NoAudioFallback`
- `-CaptureDumpWav`
- `-CaptureDumpSeconds 8`
- `-CaptureDumpDir debug_captures`

### 桌面客户端首版能力（Tauri）

- 首页主信息：服务状态、当前音频源、本机连接地址、当前连接设备数量
- 服务控制：启动/停止/重启
- 音频源切换：`Windows 系统音频` / `测试音`
- 选项开关：采集失败 fallback、capture wav 导出
- 连接信息：会话状态、最近连接客户端、可复制连接地址
- 调试入口：折叠式日志区 + 关键 metrics 概览（默认折叠）
- 双语：中文 / English

### Android 客户端

- Flutter 构建 APK：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1 -BuildApk
```

- Flutter 运行到设备：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

- 仅 Gradle 调试：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_gradle.ps1
```

## 快速联调步骤

1. 环境检查：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1
```

2. 启动服务端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource windows_loopback
```

3. 连接手机并确认：

```powershell
adb devices
```

4. 启动 Android 客户端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

5. App 内执行：发现设备 -> 连接 -> 开始播放。

## v7 纯 PCM 诊断链路

当前 Android UI 版本号应显示：

- `UI build: playback-diagnostics-v29`

v8 在 v7 纯 PCM 诊断基础上修复了一个播放端状态机问题：当 jitter buffer 已经空队列 underrun 时，不再继续推进 expected sequence，避免后续正常到达的新 UDP 包被持续误判为 late packet。

v9 继续只做链路稳定性修复：Dart 播放侧改为“按真实经过时间结算并补写欠账帧（catch-up）”，不再依赖严格每 10ms 单帧写入，减少 UI 调度抖动导致的音频断续与音量忽大忽小体感。

v10 进一步降低 UI isolate 抢占：不再对每个 UDP 包触发 `setState`，改为降频刷新，避免高频重绘影响收包与播放调度。

v12 修复 Android 写线程在停止/重启时的中断崩溃，并将每秒统计改为基于真实 elapsed 秒计算。

v13 修正 Android native `audioTrackWriteFrames` 统计为真实帧数（不再是写入调用次数），并提升 jitter 起播/上限缓冲（200ms / 1200ms）以优先稳定播放。

v14 服务端新增“同一客户端 IP 仅保留一个活跃 UDP 流”保护，新的 WS 会话建立后会主动中止旧流，避免重连后出现重复推流导致的突发拥塞；同时将逐帧 `capture frame` 日志降为 `trace` 以减少高频日志对实时链路的干扰。

v15 修复 Android 播放预算时钟精度：`playbackBudget` 从整毫秒改为微秒换算的浮点毫秒，避免定时器量化误差长期累积导致消费速率偏低（`audio_track_write_frames_per_sec` 明显低于 100）并触发缓冲持续堆积。

v16 仅做版本与可观测性增强：启动时输出 `ui_build` 日志，便于确认手机端实际运行的 APK 版本，避免重装后仍观察到旧界面标签时难以判定。

v17 增强断流容错：服务端在 WS 控制通道异常关闭后不再立即中止 UDP 推流，而是保活 30 秒（grace）再释放；Android 端增加 `ws_error/ws_done` 日志，便于定位控制通道断开原因。

v18 聚焦 UI 可用性：页面重排为 `Connection / Playback / Debug` 三段式，新增连接模式分段选择（Discovered/Manual）和单主按钮（按状态自动切换 Connect/Start/Stop），调试指标默认折叠，保留关键播放指标常显。

v19 新增双语支持：应用启动时根据系统语言自动选择（系统中文 -> 中文；其他语言 -> English），并在页面右上角提供语言切换入口（中文/English）。

v20 增强局域网发现可靠性：Android 新增 `ACCESS_WIFI_STATE` / `CHANGE_WIFI_MULTICAST_STATE` 权限，并在应用运行期间持有 `MulticastLock`，减少部分机型收不到 UDP 广播 beacon 的情况。

v21 增加发现兜底：当 UDP 广播发现失败时，Android 会在 `Discovered` 模式下自动执行局域网主动探测（/24 轻量探测 39991），并提供 `Scan LAN` 手动触发按钮。

v22 优化发现体验：主动探测结果显示为更友好的名称（`Scanned Server/扫描发现`），并把“最近成功连接”的主机自动置顶，便于快速复连。同时在此记录发现策略：优先 UDP 广播，失败时自动回退到主动探测。

v23 产品化收尾（本轮）：连接体验聚焦优化，不改音频链路与网络核心逻辑。
- 设备列表为最近成功连接设备增加 `Recent/最近连接` 标记。
- 发现列表默认优先选中最近连接主机。
- 页面顶部增加“快速连接卡片”（一键连接最近设备）。
- 当设备为空时展示发现失败引导：`Scan LAN` + 手动 IP 提示。
- 增加扫描中状态文案：`Scanning LAN... / 正在扫描局域网...`。
- 增加首次使用一次性提示弹窗（轻量引导）。

v24 UI 收敛（本轮）：仅优化首屏信息结构与交互优先级，不改音频链路和发现核心逻辑。
- 顶部区域收敛为：连接状态 chip + 当前连接摘要（设备名/IP）。
- 快速连接卡片保留为最近连接入口，但降级为次级按钮，避免与主操作冲突。
- 主界面下沉端口信息（`ws/udp`）到调试区，列表默认只显示设备名/IP/最近连接标记。
- 全局仅保留一个主 CTA（按状态切换：连接 -> 开始播放 -> 停止播放）。
- 调试指标改为双列紧凑栅格，原始日志保留在底部弱化展示。

v25 后台播放架构改造（Media3 + 渐进迁移）：
- 新增 `PlaybackForegroundService`（`MediaSessionService`）与常驻通知。
- 新增服务控制通道：`MethodChannel('lan_audio/playback_service')`。
- 新增服务事件通道：`EventChannel('lan_audio/playback_events')`。
- 新增迁移开关：`kUseBackgroundPlaybackService`（v25 默认 `false`，保留 legacy 前台链路做灰度）。
- 新增后台链路模块：服务内 WS/UDP/jitter/AudioTrack（Flutter 页面仅做控制与状态展示）。
- Manifest 增加前台媒体服务权限与 service 声明（`foregroundServiceType="mediaPlayback"`）。
- 后台联调命令示例：
  - `adb shell dumpsys activity services | findstr PlaybackForegroundService`
  - `adb logcat | findstr lan_audio_service`
  - 锁屏/切后台后持续观察通知是否仍在、播放是否持续。

v26 后台连续播放稳定性修复（本轮）：
- `kUseBackgroundPlaybackService` 默认改为 `true`，优先走后台服务链路。
- Android 新增 `WAKE_LOCK` 权限。
- `PlaybackForegroundService` 新增 `PARTIAL_WAKE_LOCK` + `WifiLock(WIFI_MODE_FULL_HIGH_PERF)` 的获取与释放。
- 注意：v26 只修复后台保活基础能力，不代表所有机型已完成实机稳定性验收。
- 实时日志抓取（后台/锁屏复测建议开启）：
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\tail_android_playback_logs.ps1 -Clear`

v27 崩溃修复（本轮）：
- 修复 `PlaybackEventBus` 线程模型：EventChannel 事件统一切回主线程分发，避免连接阶段 `@UiThread` 崩溃。

v28 连接修复（本轮）：
- 修复后台服务 `OkHttp` 被系统明文策略拦截的问题：`AndroidManifest` 增加 `android:usesCleartextTraffic="true"`。
- 目的：允许当前 MVP 的局域网 `ws://` 控制链路正常连接（不涉及 TLS）。

v29 缓冲卡死修复（本轮）：
- 修复后台会话重连竞态：`ws failure` 不再重复触发双重重连调度。
- 增加流回调代际保护（generation guard），忽略过期连接回调，避免旧失败事件打断新连接。
- `ws connected` 后会主动取消挂起中的重连任务，降低“刚连上又回缓冲”的概率。

v11 增加播放写入批处理：Dart 侧把 2~4 个连续 10ms PCM 帧合并后一次写入 AudioTrack，降低 MethodChannel 高频调用开销。

v7 已禁用服务端 v6 的响度归一化/限幅逻辑，网络发送路径固定为：

- `48000 Hz`
- `stereo`
- `PCM16 little-endian`
- `10ms/frame`
- `1920 bytes/frame`

推荐先用 synthetic 正弦音做对照实验：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic
```

如果 synthetic 也出现忽大忽小或 `buffered_ms` 大幅跳动，优先排查 Android 播放队列、AudioTrack 写入边界和 jitter buffer 消费节奏。

如果 synthetic 稳定，而 `windows_loopback` 不稳定，再排查 Windows 采集、格式转换、PCM16 封包和设备时钟漂移。

服务端连接后每秒会输出 `tx summary`，重点看：

- `tx_peak`
- `tx_rms`
- `tx_frame_bytes`，应稳定为 `1920`
- `tx_frames_per_sec`，应接近 `100`
- `sample_rate`，应为 `48000`
- `channels`，应为 `2`
- `frame_duration_ms`，应为 `10`
- `seq`

Android 端每秒输出/展示 `rx_summary`，重点看：

- `rx_frames_per_sec`
- `queued_frames`
- `buffered_ms`
- `jitter_underrun`
- `dropped_late_frames`
- `silence_fill_count`
- `audio_track_write_frames_per_sec`
- `audio_track_short_write_count`

这些指标用于定位链路时钟/队列/帧边界问题，不代表已经完成 Opus、同步或自适应 jitter buffer。

## 文档

- `docs/dev_setup.md`
- `docs/local_simulation.md`
- `docs/audio_capture.md`
- `docs/architecture.md`
- `docs/todo.md`
- `docs/known_issues.md`
- `docs/desktop_ui.md`

