# TODO / Stub Tracking

## Audio Capture (Windows)

- [ ] 多设备实机验证（不同声卡/驱动/采样率）
- [ ] 长时间稳定性验证与时钟漂移处理
- [ ] 扩展 mix format 支持（更多 extensible 分支）
- [ ] 完整重采样/重通道策略

## Android Playback

- [ ] 多机型 AudioTrack 稳定性与延迟调优（当前已实现基础可播放路径）
- [ ] jitter buffer 自适应策略（当前固定起播缓冲）
- [ ] 播放线程优先级/抗抖动增强

## Opus

- [ ] replace PCM passthrough with real Opus encoder/decoder

## Productization

- [ ] Tauri production UI + status controls
- [ ] installer / firewall guidance
- [ ] structured logs export
