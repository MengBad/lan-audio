# Android Flutter Client (MVP)

当前目录实现了 Android 侧最小可用播放链路：

- discovery + ws session
- UDP PCM 收包
- 简单 jitter buffer（按 sequence 排队）
- AudioTrack 真播放（MethodChannel -> Kotlin）

## 目标链路

`UDP PCM -> Jitter Buffer -> AudioTrack`

## 运行

```bash
flutter pub get
flutter run
```

## 调试页面能力

- 发现/连接桌面端
- 开始播放 / 停止播放
- 播放状态：`playing | buffering | stopped`
- 指标：sample rate、channels、buffered ms、underrun/dropped/late
- 音频日志：无包、缓冲不足、播放中、AudioTrack 初始化失败

## 限制

- 当前优先 PCM 16-bit 直通，不含 Opus。
- jitter buffer 为最小版本（固定起播缓冲，无自适应）。
- 仍需真实机型覆盖验证稳定性与延迟表现。
