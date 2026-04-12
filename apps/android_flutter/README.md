# Android Flutter Client (MVP)

当前目录实现了 Android 侧最小可用播放链路：

- discovery + ws session
- UDP PCM 收包
- 简单 jitter buffer（按 sequence 排队）
- AudioTrack 真播放（MethodChannel -> Kotlin）

v25 新增后台播放改造骨架：

- `PlaybackForegroundService`（Media3 `MediaSessionService` + 前台通知）
- `MethodChannel('lan_audio/playback_service')` 控制命令
- `EventChannel('lan_audio/playback_events')` 状态事件
- Dart 侧迁移开关：`kUseBackgroundPlaybackService`（v26 默认 `true`，可手动回退 legacy）
- v27 修复服务事件线程崩溃：EventChannel 状态推送统一在主线程执行。
- v28 修复后台服务 `ws://` 明文连接拦截：启用 `usesCleartextTraffic=true`。
- v29 修复后台重连竞态：移除重复重连触发，增加回调代际隔离，连接成功后取消挂起重连任务。

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
- v26 新增 `WAKE_LOCK` + `PARTIAL_WAKE_LOCK` + `WifiLock`，用于降低锁屏/后台场景中断概率。
- 仍需继续进行不同机型的锁屏/后台/熄屏实机验收。
