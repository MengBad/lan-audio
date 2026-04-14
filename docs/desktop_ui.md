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
- 模式状态预留：`current_audio_mode`（默认 `balanced`，后续通过 Protocol v2 `set_audio_mode/audio_mode_changed` 接线）

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
- 桌面端模式切换 UI（low_latency/balanced/high_quality）与 Protocol v2 联动
- Windows 安装器分发策略与防火墙提示文案细化
