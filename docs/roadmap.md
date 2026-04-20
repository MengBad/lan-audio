# Project Roadmap

当前默认运行主路径仍是 `legacy_las1 + pcm16`。Protocol v2 的定位是低延迟音频传输产品升级，而不是为了替换协议而替换协议。

V2 必须同时满足：

- 低延迟：围绕更低端到端延迟设计模式、buffer、codec 与连接路径。
- 可诊断：用户和开发者能看到协议路径、codec、模式、连接方式与关键健康指标。
- 可回滚：`legacy_las1 + pcm16`、`synthetic + v2_header + pcm16`、显式灰度开关必须长期可用。
- 可扩展：为 Opus、USB、智能模式、多设备与后续 microphone backhaul 保留清晰落点。

## 1. 播放稳定性

- 已完成：Android/Windows 真机可连接并出声，主路径达到可用级别。
- 已完成：Android 后台播放服务、AudioTrack 写入、jitter buffer、关键播放指标。
- 已完成：`synthetic + v2_header` 真机播放与模式切换验收通过。
- 已完成：`windows_loopback + v2_header` 小流量真机灰度可播，但暂不稳定。
- 尚未完成：多机型长稳验证、锁屏/后台长稳、时钟漂移与自适应 jitter。

## 2. 延迟优化

- 已完成：基础 jitter buffer 与播放调度修复。
- 已有骨架：`low_latency / balanced / high_quality` 三档策略已映射到 start buffer、max buffer、batch、drop threshold、播放后端偏好与 codec 偏好。
- 尚未完成：自动低延迟自适应、USB tethering 延迟样本、loopback + v2 长尾抖动收敛。

## 3. 多策略模式

- 已完成：Protocol v2 控制面同步 `current_audio_mode`。
- 已完成：Rust / Android / Windows 桌面端均有一致的 `AudioModeProfile` 语义。
- 尚未完成：设备级播放后端自动选择、不同 Android 机型的 fast path / stable AudioTrack 回退策略。

## 4. Protocol v2 演进

- 已完成：Protocol v2 草案、Rust v2 结构体、UDP v2 header、控制面 `hello/hello_ack`、mode 同步、capabilities。
- 已完成：数据面 `LAS1/LAV2` 双栈识别，默认仍为 `legacy_las1`。
- 已完成：`config_changed/discontinuity` 最小真实处理。
- 已完成：按会话 capability 协商 `data_plane + codec`，不支持 Opus 的客户端自动回退 PCM16。
- 已完成：Opus 实验链路已使用标准 libopus 服务端编码 + Android libopus/JNI 解码，`synthetic + v2_header + opus_experimental` 真机非零 PCM 验收通过。
- 尚未完成：Opus 听感确认、Opus loopback 灰度、Opus 长稳/CPU/丢包恢复对比。
- 尚未启用：v2 数据面默认主路径、Opus 默认 codec。

## 5. 产品化 UI 与桌面端交付

- 已完成：Windows Tauri 桌面客户端可启动/停止服务，展示状态、连接、模式、协议路径、codec 与折叠调试信息。
- 已完成：Android 首页和设置中已有连接、播放、发现、最近连接、多语言与诊断入口。
- 已有路线：USB tethering 作为低延迟推荐连接方式，USB direct 作为未来能力。
- 尚未完成：发布包签名、自动诊断报告、USB 连接体验闭环、多设备会话 UI。

## 当前发布判断

- 默认主路径：`legacy_las1 + pcm16`。
- 可灰度路径：`synthetic + v2_header + pcm16`、`synthetic + v2_header + opus_experimental`、显式开关下的 `windows_loopback + v2_header`。
- 当前不建议切默认：`v2_header` 和 `opus_experimental` 都还需要更多真机长稳与 loopback 验收。
