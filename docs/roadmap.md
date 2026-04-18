# Project Roadmap

当前主路径说明：

- 默认运行主路径仍是 PCM + legacy `LAS1`，这是当前安全回滚路径。
- Protocol v2 已从协议骨架推进为低延迟产品升级骨架：控制面联动已接通，数据面具备 `LAS1/LAV2` 双栈灰度，默认仍不全量切换。
- V2 的产品目标是低延迟、可诊断、可回滚、可扩展，而不是为了升级而升级。

## V2 产品能力模型

### 连接能力

- 已完成：UDP 自动发现、手动地址、局域网扫描、最近连接/快速连接。
- 已有骨架：Android/桌面端可展示连接路径与协议路径。
- 尚未启用：USB direct。
- 后续扩展：USB tethering 低延迟推荐路径、USB direct 探测、发现失败时更明确的连接帮助。

### 发送能力

- 已完成：`synthetic` 稳定基线、`windows_loopback` 系统音频采集。
- 已有骨架：loopback + v2 header 显式灰度开关。
- 尚未启用：microphone backhaul。
- 后续扩展：手机麦克风回传、loopback + v2 长稳压测、采集侧动态缓冲。

### 播放能力

- 已完成：Android 后台播放服务、AudioTrack 写入、jitter buffer、基础追帧。
- 已有骨架：三档模式对应 start buffer、max buffer、batch、drop threshold、后端偏好。
- 尚未启用：设备级后端自动选择。
- 后续扩展：fast path 探测、高质量后端回退策略、智能自适应缓冲。

### 协议能力

- 已完成：Protocol v2 控制面联动、capabilities、mode 同步、`config_changed/discontinuity`。
- 已有骨架：数据面 `LAV2` header、codec 枚举、Opus 实验入口。
- 已有实验链路：服务端 `opus-rs` 编码、Android `MediaCodec audio/opus` 解码、桌面 V2 Opus 选择入口。
- 尚未启用：Opus 默认路径、v2 数据面默认主路径。
- 后续扩展：v1/v2 双栈灰度扩大、Opus synthetic/loopback 真机验收。

### 诊断能力

- 已完成：Android 播放指标、桌面折叠 metrics/log、连接地址复制。
- 已有骨架：连接帮助入口、网络/后台问题文档化。
- 尚未启用：自动诊断报告。
- 后续扩展：同网段检查、AP isolation 提示、USB 推荐、后台电池优化检查。

## 1) 播放稳定性

- 已完成：Android/Windows 真机可连接并出声（可用）。
- 已有骨架：后台播放服务（Media3）与关键诊断指标。
- 尚未启用：多机型长稳结论。
- 后续扩展：后台锁屏长时稳定性、抗抖策略强化、不同 Android 厂商策略验证。

## 2) 延迟优化

- 已完成：基础 jitter buffer 与若干播放调度修复。
- 已有骨架：三档模式参数矩阵与 Android 播放策略入口。
- 尚未启用：自动低延迟自适应。
- 后续扩展：低延迟路径探测、USB tethering 低延迟验收、loopback + v2 长尾抖动收敛。

## 3) 多策略模式

- 已完成：`low_latency / balanced / high_quality` 控制面同步。
- 已有骨架：`AudioModeProfile` 将模式映射到 start/max buffer、batch、drop threshold、codec/sample format/frame duration 偏好。
- 尚未启用：完整设备后端选择与自动回退。
- 后续扩展：智能自适应、设备能力记忆、模式切换体验提示。

## 4) 协议演进（Protocol v2）

- 已完成：v2 协议草案、Rust v2 结构体、UDP v2 header 编解码、session 协商入口。
- 已有骨架：Opus experimental、USB capabilities、数据面双栈。
- 已有实验链路：Opus 在 `v2_header` 下可显式启用，但尚未稳定验收。
- 尚未启用：默认 UDP 运行流量切到 v2 header；Opus 作为默认或稳定 codec。
- 后续扩展：先扩大 synthetic/loopback 灰度，再做默认路径切换。

## 5) 产品化 UI / 桌面端交付

- 已完成：Windows Tauri 可执行客户端、单主按钮控制台 UI。
- 已有骨架：桌面与 Android 端显示模式、协议路径、播放后端、灰度状态、推荐连接方式。
- 尚未启用：完整安装器发布与自动诊断报告。
- 后续扩展：连接二维码、防火墙引导、USB 帮助、会话详情、发布包签名。
