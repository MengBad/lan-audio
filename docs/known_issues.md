# Known Issues

> 更新：2026-05-25，基线 `v1.15`。状态与派工见 `docs/MAINTENANCE_PLAN.md`。本文件只登记"仍存在/未验证"，不把未验证写成已稳定。

## 仍待验证（影响发布判断）

1. v1.15 UI 构建的 30+ 分钟主路径长稳
   - 配置：`windows_loopback + v2_header + opus` / balanced / WiFi。
   - 现状：`artifacts/release/device_acceptance.json` 只有 2026-04-22 的 2 分钟 × 2 场景样本；v1.4 周期 30+ 分钟样本结论为 `continue_fix`。
   - 影响：下一版 release 前必须补样本，门控不因"代码没改"自动放行。
2. USB direct 10 分钟真机样本
   - 现状：v1.7 周期记录为 `known_issue`，未采集 `adb reverse + usb_direct` 长稳与延迟证据。
   - 影响：USB direct 保持非默认路径。
3. 4 档模式切换重同步回归
   - 历史：v1.4 周期曾复现切换后 `audio_track_write_frames_per_sec=0` / `silence_fill=5893`；后续有修复但无 v1.15 回归样本。
   - 影响：ultra_low_latency / auto-degradation 切换路径保持观察。
4. ultra_low_latency 端到端延迟实测
   - 现状：≤30ms 为设计目标，只有本地验证与代码审查，无真机 p95 artifact。
5. 桌面端单设备断开命令
   - 现状：无稳定 per-client 命令契约，长期 deferred；多设备最多 4 客户端由服务端会话隔离。

## 环境限制（本机，不是产品缺陷）

- 本机 Rust 只装了 `x86_64-pc-windows-msvc` target，缺 MSVC `link.exe` 与 Windows SDK import libs；`cargo check` / `cargo test` 无法在本机链接。装 VS Build Tools(VC++) + Windows 10 SDK 后重跑。
- `cargo` / `flutter` 不在默认 shell PATH；二进制在 `C:\Users\15622\.cargo\bin` 与 `C:\Users\15622\flutter\bin`。
- Flutter 首次运行需 `LOCALAPPDATA` / `PUB_CACHE` 环境变量；本轮已用显式值跑通 `flutter analyze` 与全套 `flutter test`。
- AGP 8.3.2 对 compileSdk 36 有兼容性提示，Kotlin 1.9.22 有未来弃用提示；不阻塞 `assembleDebug`，后续升级 AGP/Kotlin。
- 若旧 `desktop_headless.exe` 仍在运行，再次启动会 UDP 端口占用（os error 10048）；先结束旧进程。

## 已完成 / 不再列为问题

- 本机 `cargo` / `rustup` / `flutter` / `adb` / `java` 可用。
- `local.properties` 自动写入、非 ASCII 路径检查、Jetifier 内存、启动主题资源缺失均已修。
- 脚本 PATH 可见性（`run_server_headless.ps1` / `run_android_client.ps1` / `run_android_gradle.ps1`）已修。
- 接手时的规则文档损坏、版本漂移、恒真发布门控、`FORCE_RELEASE` 未实现、入口测试缺覆盖，已在 2026-05-25 收尾中修复；`check_version_sync.ps1` / `assert_release_gate.ps1` / `cargo fmt` / `flutter analyze` / `flutter test` 实跑通过。
