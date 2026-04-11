# LAN Phone Speaker MVP (Windows -> Android)

当前仓库已可在本机进入“可构建、可启动、可联调准备”状态，技术路线保持不变：Rust + Tauri + Flutter + UDP + WebSocket（当前主路径 PCM 直通）。

## 本机实测状态（2026-04-12, Asia/Shanghai）

### 1) 已修复并验证通过

- 环境检查：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1` 通过
  - `cargo --version` / `rustup --version` / `flutter --version` / `adb version` / `java -version` 均可执行
- Rust workspace：
  - `cargo metadata --no-deps` 成功
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

- `UI build: playback-diagnostics-v23`

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

