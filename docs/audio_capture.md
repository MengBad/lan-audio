# Audio Capture Layer (Stage 4)

## 目标

Windows loopback raw PCM 可持续产帧，并为 Android 播放链路提供稳定输入。

## 已完成

- `PcmFrameAccumulator`：任意 packet 累积为固定 10ms 帧。
- Windows loopback：持续读包、静音/无包区分、峰值/RMS统计。
- 可选 debug wav 导出（用于证明采集真实有效）。

## 当前输出形态

- 当前网络主路径发送 PCM16 payload。
- loopback 内部维持 f32 处理，发送前最小转换。

## 注意

- 目前支持 mix format 主路径：float32 / i16 PCM。
- 复杂格式与完整重采样仍为后续工作。
