# AudioRelay 逆向规格（0.27.5 Windows 样本）

> 来源：`https://dl.audiorelay.net/setups/windows/audiorelay-0.27.5.exe`（Inno Setup，解包到 `.net/`）。
> 方法：非侵入静态分析 —— 安装包解包 + .NET 程序集元数据标识符提取（Model.dll 等）。未运行样本、未抓包、未逆向受保护代码。
> 基线：本项目 v1.16.2。本文只记录可证实的产品/协议设计，不暗示代码复制。

## 样本组成

- `Model.dll` (231KB)：协议模型与 IPC 消息（核心）。
- `AudioConnect.Utils.dll`、`audiorelay-backend.exe`、`Settings.dll`、`Analytics.dll`。
- 依赖：`NAudio.dll`（WASAPI）、`Google.Protobuf.dll`、`LanguageExt.Core.dll`、`Serilog`、`SimpleInjector`、Qt5 + JRE。

## 从元数据还原的设计要点（按本轮落地优先级）

### 1. 丢包重传 + 抗抖动（P0，稳定性主线）

- 消息类型：`retransmissionRequest` / `retransmissionResponse` / `notFoundRetransmissionResponse`，字段 `restransmissionTimestampMs`、`requestedTick`、`maxElapsedTimeBetweenPayloads`。
- 语义：接收端按时间戳/tick 请求缺失 payload；服务端重发；找不到返回 notFound。
- 缓冲：`BufferLengthInMsWithEventSync` / `WithoutEventSync`、`useEventSync`、`initializeEventSync` —— 事件同步与定时缓冲两套策略可切换。
- 本项目映射：已有的 UDP 序号 + jitter buffer + `client_watermark` 上报，扩展为显式重传请求/响应与 event-sync 缓冲开关。

### 2. 强制音频格式 + 能力协商回退（P1）

- 消息：`serverAudioConfigRequest/Response`（`forcedServerAudioOptions`、`ProtoForcedServerCompression`）、`fallbackSampleRate` / `fallbackChannels` / `fallbackBits`。
- 语义：服务端可下发强制格式，客户端确认是否支持（`ConfirmSuggestedFormatIsSupported` / `IsFormatSupported`），不支持则按 fallback 三元组降级。
- 本项目映射：扩展 Protocol v2 capabilities 的 codec/format 协商，失败回退 PCM16 / 48k / 16bit（与现有 `legacy_las1 + pcm16` 回滚一致）。

### 3. 设备热插拔 + 网络变化恢复（P1）

- `MMDevice` 通知：`OnDeviceAdded` / `OnDeviceRemoved` / `OnDefaultDeviceChanged` / `OnDeviceStateChanged`、`IsDeviceInvalidated`、`WatchDevice`、`FindDevice`。
- `NetworkAddressChanged`、`localIpAddresses`、`broadcastAddresses`、`IpcNetworkInfoService`。
- 本项目映射：Windows capture 层接 MMDevice 通知（设备移除/默认切换自动重建 capture 或报告错误）；网络变化触发 mDNS 重扫 + 智能重连（已有 reconnect 逻辑扩展）。

### 4. 音量双向控制 + MediaKey（P2）

- `changeAndroidVolume` / `androidVolume` / `desktopVolume` / `muteDesktop` / `isMuted`、`AudioEndpointVolume` / `GetMasterVolume`。
- `MediaKeysService` / `IsMediaKeyMessage`。
- 本项目映射：已有托盘远程音量与 MediaSession；补齐手机端 volume 变化反向同步与桌面静音状态同步。

### 5. IPC/进程架构（参考，不移植）

- 前端与 `audiorelay-backend.exe` 通过 `Ipc*` 消息通信（`IpcStartCaptureMessage` / `IpcSocketsService` / `IpcPriorityService` / `IpcChangeProcessPriorityMessage`）。
- Windows 启动项：`WindowsStartupRegistryName`、`IpcEnableWindowsStartupMessage`；错误上报：`Sentry`、`IpcEnableErrorReportingMessage`。
- 本项目已是单进程 Rust/Tauri，此部分仅作稳定性设计参考（进程优先级、启动恢复）。

## 与本项目的差异结论

- 协议形态不同：AudioRelay 用 Protobuf + IPC；本项目用 WebSocket JSON 控制面 + UDP 二进制数据面（v2/v3）。移植的是**语义**不是报文格式。
- 可借鉴的成熟点：显式重传、格式三元组回退、设备/网络热插拔、事件同步缓冲。
- 不采纳：IPC 双进程、Protobuf 全量替换、JRE 依赖、第三方统计/崩溃上报。
