# Windows Desktop UI (Tauri)

## Scope (v1)

Windows 桌面端已从 `desktop_headless` 调试入口升级为可交付桌面客户端（Tauri 壳 + 内置服务控制）。

首屏聚焦 4 类核心信息：

- 服务状态
- 当前音频源
- 本机连接地址
- 当前连接设备数量

布局：

- 左侧：主状态与服务控制
- 右侧：连接信息与最近设备
- 底部：折叠调试区（默认折叠）

## 用户可执行操作

- 启动服务
- 停止服务
- 重启服务
- 切换音频源：`windows_loopback` / `synthetic`
- 开关：
  - 采集失败时回退到 synthetic
  - 导出 capture wav（调试）

设置在服务运行中变更时，会自动重启服务以应用新配置。

## 数据与状态来源

- 服务状态：桌面壳内部生命周期状态机（`not_started/starting/running/stopping/error`）
- 连接数：`metrics.active_sessions`
- 最近连接客户端：服务端 `metrics.recent_clients`
- 本机地址：桌面端运行时自动探测本机 IPv4
- 模式状态：`current_audio_mode`（默认 `balanced`），由 Protocol v2 `set_audio_mode/audio_mode_changed` 与服务端状态同步。
- 模式策略：桌面端展示 `AudioModeProfile` 摘要（start/max buffer、batch），用于解释当前低延迟/平衡/高音质策略。
- 协议路径：展示 `legacy_las1` / `v2_header`，并明确当前是否处于灰度。
- Codec：展示当前请求 codec 与实际生效 codec；`opus_experimental` 仅在有效 `v2_header` 下启用实验链路，否则回退 PCM16。
- 推荐连接：默认同 Wi-Fi；v2 灰度路径下推荐 USB tethering 或 5GHz Wi-Fi。

## V2 产品化展示要求

Windows 桌面端不把 V2 当作纯协议字段展示，而是解释为用户可理解的链路状态：

- 当前服务状态：是否正在推流。
- 当前连接状态：手机是否已连接。
- 当前协议路径：安全主路径还是 v2 灰度路径。
- 当前模式：`low_latency / balanced / high_quality`。
- 当前音源：`synthetic / windows_loopback`。
- 当前 codec：默认 PCM16；Opus 为 V2 Header 下的实验链路。
- 当前是否灰度：v2 header 和 loopback 灰度必须显式可见。
- 当前推荐连接方式：Wi-Fi / USB tethering。

这些字段放在设备/会话信息区，用纯文本行展示，不再拆成大量卡片。

## 调试区

默认折叠，包含：

- 关键 metrics 概览（发送包、字节、采集帧、采集错误、采集状态、峰值）
- 最近应用日志（含启动/停止/配置切换/异常）

## i18n

当前支持：

- 中文
- English

语言默认按系统语言（`zh*` -> 中文，其它 -> English），可在右上角切换。

## Known TODO

- 二维码连接入口（当前先提供可复制地址）
- 更丰富的会话详情（当前为首版简化）
- 桌面端直接切换 `low_latency/balanced/high_quality` 的 UI（当前先展示状态与策略）
- USB tethering 帮助入口与 Windows 防火墙提示
- Opus 实验链路真机验收（当前已接服务端编码与 Android 系统解码，尚未声明稳定）
- Windows 安装器分发策略与防火墙提示文案细化
