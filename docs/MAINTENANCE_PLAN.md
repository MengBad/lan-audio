# LAN Audio 接手基线 & 维护计划

> 接手日期：2026-05-25
> 接手基线：`main@64e2992`，tag `v1.15`，`VERSION=1.15`
> 本文件是项目接手后的唯一维护入口。规则见 `AGENTS.md`，发布策略见 `docs/RELEASE_POLICY.md`。

---

## 1. 项目是什么

Windows 通过 WASAPI Loopback 采集系统音频，经局域网（Wi-Fi / USB adb）实时推流到 Android 手机，手机充当无线音响。

| 端 | 技术 | 入口 |
|---|---|---|
| Windows 发送端 | Rust + Tauri 2 | `apps/desktop/src-tauri`，前端 `apps/desktop/web/index.html` |
| Windows 调试/回归 | Rust CLI | `crates/lan_audio_server/src/bin/desktop_headless.rs` |
| Android 接收端 | Flutter + Kotlin/Oboe | `apps/android_flutter` |
| 共享契约 | Rust | `crates/lan_audio_domain`（模式/连接状态/回滚/失败码/snapshot/发布门控 schema） |
| 协议 | Rust | `crates/lan_audio_protocol`（v2 控制面 + UDP 二进制头，v3 分片预留） |
| 传输/采集 | Rust | `crates/lan_audio_server`（capture / transport / data_plane / session / adaptive_runtime / sync_engine / reverse_channel） |

工作区 4 个 crate：`lan_audio_domain`、`lan_audio_protocol`、`lan_audio_server`、`lan_audio_desktop`。Flutter 测试 13 个文件、Rust 集成测试 2 个文件。

## 2. 当前运行事实（接手时的真实状态）

这些是本次接手用 grep / 只读审计核实过的**代码事实**，不是文档宣称：

| 项 | 值 | 证据 |
|---|---|---|
| 主路径 | `windows_loopback + v2_header + opus` | `crates/lan_audio_server/src/config.rs:184,192-193` |
| 回滚路径 | `legacy_las1 + pcm16` | `crates/lan_audio_server/src/data_plane.rs:251-252`；`--force-rollback` 在 `config.rs:309` |
| 第二回滚 | `synthetic + v2_header` | `crates/lan_audio_server/src/config.rs:433-435` |
| 自适应运行时 | 默认开启；`--no-adaptive-runtime` 关闭 | `crates/lan_audio_server/src/config.rs:165,318` |
| 模式 | 4 档：ultra_low_latency / low_latency / balanced / high_quality | `apps/android_flutter/lib/main.dart:1831-1852` |
| Protocol | v2 为默认数据面；v3（PCM24 分片）能力门控，默认关闭 | `docs/protocol.md`；`crates/lan_audio_protocol` |
| 发布决策 | `allow_release` | `artifacts/release/acceptance_gate.json` |
| 真机验收 | `passed`（5391d451 / 24129PN74C，2 分钟 × 2 场景，2026-04-22） | `artifacts/release/device_acceptance.json` |
| 长稳证据 | 仅到 v1.4 周期 30+ 分钟样本，结论 `continue_fix` | `docs/todo.md` v1.4 段 |
| Android 生产入口 | 3-tab `IndexedStack`：PlayPage / ConnectPage / SettingsPage | `apps/android_flutter/lib/main.dart:1860-1912` |
| MorePage | **不再挂载**；`MoreServerData` 仍是 ConnectPage 数据契约 | `main.dart:18,1886`；`more_page.dart:878` |

### 接手时已修掉的漂移（本轮落地）

1. `AGENTS.md` 原文损坏（`cargo test` 行被截断、v1.8 检查项重复两次、规则里仍要求挂载 MorePage）→ 重写为单份干净规则，checklist 对齐 3-tab 现实。
2. 版本漂移：`VERSION=1.15` / tag `v1.15`，但 `Cargo.toml workspace.package=1.14.0`、`kAppVersion='1.14'`、`local.properties=1.14.0/3140`、mDNS 硬编码 `"1.7"`、文档写 v1.11 / v1.7.2 → 全部统一，mDNS 改为 `env!("CARGO_PKG_VERSION")`。
3. `CHANGELOG.md` 缺 v1.14 / v1.15 → 补齐；`release.ps1` 的 changelog 提取修为短版本匹配。
4. `docs/todo.md` / `docs/roadmap.md` / `docs/RELEASE_POLICY.md` 被 `.gitignore` 忽略，且 `release.ps1` 的 `git add` 会把它们当显式路径加入（在 `$ErrorActionPreference='Stop'` 下会因 ignored 路径非零退出）→ 取消忽略并把发布门控 artifact 反忽略，发布脚本可以真正提交这些契约文档。
5. `scripts/assert_release_gate.ps1` 原先恒 `exit 0` → 改为读 `acceptance_gate.json` + `device_acceptance.json` 的 artifact-driven 校验（主路径、回滚路径、阻塞码、决策全检）。
6. 规则宣称 `FORCE_RELEASE=true` 但脚本/workflow 完全没实现 → `package_release.ps1` / `release.ps1` 支持 `FORCE_RELEASE` 环境变量与 `-ForceRelease`，只覆盖人工验收字段，不跳过编译/打包/CI/回滚检查；Release body 带 `[human-override]` 标记。
7. 新增 `scripts/check_version_sync.ps1`，在 `validate_local.ps1` 首步执行，并在 `bump_version.ps1` 里补 `kAppVersion` 同步，杜绝下次发版再次漂移。
8. `app_entry_smoke_test.dart` 补独立 connected-state `PlayPage` 测试（真实挂载 `LatencyChartWidget`）与 `MoreServerData` 契约断言；`flutter analyze` / `flutter test`（全套）实跑通过；`.github/workflows/release.yml` Release body 补齐 Protocol/codec/已验证/未验证/MediaSession/回滚最小内容。
9. 工作区根目录的 Anima 文档、`.superpowers/`、`.tmp/`、`"LAN Audio/"` 安装包副本属于外部/本地杂物 → 加入 `.gitignore`，不纳入本项目维护范围。
10. UI 补全轮：Android Settings 接入后台播放入口、EQ 预设、MicStatusWidget、版本行、Auto codec、buffer 重置，PlayPage 渲染状态码；Desktop 修复丢包率分母（改用 `metrics.tx_packets`），渲染最近设备、推荐连接、V2 状态、mode profile、灰度/回滚信息与扩展诊断；两侧新增 i18n/测试，flutter 46 测试全通过。
11. 最终完善轮：刷新 `docs/architecture.md`（MVP 时代描述 → v1.15 双端架构）与 `docs/known_issues.md`（只登记未验证/环境限制）；CI 新增 `version-sync` job 执行 `check_version_sync.ps1` + `assert_release_gate.ps1`；`FORCE_RELEASE` 从"只警告"改为真正覆盖人工字段并在 `acceptance_gate.json` 落 `human_override`，且 `release.ps1` 在 `git add` 之前写入以便随发布提交；release notes 的 human-override 段改为按 artifact 条件输出；`docs/RELEASE_POLICY.md` / `CONTRIBUTING.md` / README 中英文档表 / AGENTS.md 全部对齐。

## 3. 维护主线（按优先级）

| # | 主线 | 当前状态 | 下一个可交付物 |
|---|---|---|---|
| P0 | 播放稳定性 | 短探针通过（`silence_fill<5`、`underrun=0`）；30+ 分钟长稳在 v1.4 周期仍 `continue_fix`；v1.15 UI 改版后无新长稳样本 | 在 5391d451 上补 30 分钟 `windows_loopback+v2_header+opus` + balanced 长稳，写回 `artifacts/release/device_acceptance.json` |
| P1 | 延迟优化 | `low_latency p95=64ms` / `balanced 185ms` / `high_quality 505ms`；ultra_low_latency 目标 ≤30ms 仅本地验证 | 用 `scripts/export_latency_probe.ps1` 生成 `artifacts/latency/latency_probe_latest.json`，补 ultra 档实测 |
| P2 | 多策略模式 | 4 档已落地；auto-degradation（jitter p95>10ms）已实现 | 补 4 档切换重同步回归（历史上 mode-switch 曾出现 `audio_track_write_frames_per_sec=0`） |
| P3 | Protocol v2/v3 | v2 稳定；v3 分片 + PCM24 服务端保留、Android 直通已回退到 v1.9.4 基线 | 保持 `supports_hires_pcm24=false` 默认；若要重启，必须独立播放路径 + 真机样本，不碰主路径 |
| P4 | UI 产品化 | v1.15 3-tab Android + 单页 Windows 已发 | 补 UI 视觉回归基线（Before/After 同场景/视口/主题） |
| P5 | 发布管理 | 门控已 artifact-driven；版本一致性已自动化 | 每次发版前跑完整 `validate_local.ps1`，不绕过 CI |

## 4. 节奏（周期化维护）

### 每次改动（强制，来自 `AGENTS.md`）
1. 读 `README.md`、`docs/todo.md`、`docs/roadmap.md`、`docs/protocol.md`、`docs/protocol_v2_migration.md`、`docs/desktop_ui.md`、`docs/RELEASE_POLICY.md`、本文件。
2. 判定任务归属主线（P0-P5）。
3. 改代码 + 补/修测试 + 更新文档 + 跑本地检查。
4. 判断是否达到发布条件；不满足就停在"继续灰度/继续修复"，不要写成已稳定。
5. 按 9 段固定格式汇报。

### 本地检查（能跑就跑）
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate_local.ps1
```
等价步骤：`check_version_sync.ps1` → `cargo fmt --all -- --check` → `cargo check` → `cargo test -p lan_audio_protocol -p lan_audio_server` → `cargo check -p lan_audio_desktop` → `flutter pub get` / `flutter analyze` / `flutter test` → `gradlew.bat assembleDebug`。

发版前额外：`flutter test test/app_entry_smoke_test.dart`、`scripts/assert_release_gate.ps1`。

### 每周
- 复核 `artifacts/release/acceptance_gate.json` 与真机样本日期；超过 30 天无新样本就降级为"继续灰度"。
- 跑一次 `--force-rollback` 冒烟，确认 `active_data_plane=legacy_las1`、`codec=pcm16`、`rollback_state=active`。
- 检查 `VERSION` / tag / `CHANGELOG.md` / 三份文档标记一致（`check_version_sync.ps1` 已自动覆盖）。

### 每个 minor 版本
1. `scripts/validate_local.ps1`
2. `scripts/package_release.ps1 -Clean`
3. `scripts/release.ps1`（内部递增版本、提交、打 tag、推送）
4. 看 GitHub Actions
5. Actions 通过后发布 Release（body 必须含 Protocol 状态 / 主路径 / codec / 已验证与未验证 / MediaSession / 更新检测 / 回滚方式）

### 每次真机（5391d451 或登记设备）
记录到 `artifacts/release/device_acceptance.json`：transport、audio_source、mode、duration_sec、`underrun_delta`、`silence_fill_delta_max_per_5s`、`jitter_buffer_*`、device、tested_at。Wi-Fi 与 USB 分开记，不混样本。

## 5. 不可动红线

- 默认主路径 `windows_loopback + v2_header + opus` 不在证据不足时切换。
- `legacy_las1 + pcm16`、`synthetic + v2_header`、协议灰度开关、`--force-rollback`、`--no-adaptive-runtime` 长期保留。
- MediaSession / 更新检测不得影响 AudioTrack / Oboe 播放路径；更新检测只静默后台请求 + 跳转 Release 页，不自动下载安装。
- 不删除回滚路径、不绕过 CI 直接发布、不把"未验证"写成"已稳定"。
- v3 / PCM24 不碰默认主路径；`supports_hires_pcm24` 默认 false。
- `VERSION` 是唯一版本源；所有版本面由 `bump_version.ps1` + `check_version_sync.ps1` 约束。

## 6. 已知未清项（接手移交，不假装已完成）

| 项 | 事实 | 处理 |
|---|---|---|
| v1.15 真机长稳 | 无当前 UI 构建的 30+ 分钟样本 | 排在 P0 第一项；未出样本前不申报新 release |
| USB direct 10 分钟 | v1.7 周期记录为 `known_issue`，未采集 | 保持标记，补 `adb reverse + usb_direct` 长稳 |
| 模式切换无声 | v1.4 周期曾复现 `audio_track_write_frames_per_sec=0`，后续有修复但无 v1.15 回归样本 | P2 回归项 |
| 桌面单设备断开 | 无稳定 per-client 命令契约，长期 deferred | 保持 deferred，不临时加半成品命令 |
| ~~`docs/known_issues.md` / `docs/architecture.md` 陈旧~~ | 最终完善轮已刷新为 v1.15 事实 | 无待办；后续随代码改动同步维护 |
| 本机 Rust 工具链 | `cargo 1.94.1` 与 `rustc` 存在，但 MSVC `link.exe` / Windows SDK import libs（kernel32.lib 等）缺失；`cargo check` / `cargo test` 在本机无法链接 | 装 VS Build Tools(VC++) + Windows 10 SDK 后重跑 `cargo check` / `cargo test -p lan_audio_protocol -p lan_audio_server`；本轮 Rust 侧只跑通 `cargo fmt --all -- --check` |

## 7. 回滚操作卡

```powershell
# CLI 全量回滚
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic --force-rollback

# 显式老路径
cargo run -p lan_audio_server --bin desktop_headless -- --data-plane legacy_las1 --codec pcm16 --audio-source windows_loopback

# 仅关闭自适应运行时
cargo run -p lan_audio_server --bin desktop_headless -- --no-adaptive-runtime
```
期望证据：`active_data_plane=legacy_las1`、`codec=pcm16`、`rollback_state=active`。桌面 UI 的"安全模式"按钮是同一路径的图形入口。

## 8. 维护任务书（可直接派工）

| ID | 任务 | 边界 | 验收 |
|---|---|---|---|
| M-01 | 30 分钟主路径长稳 + 写回 device_acceptance.json | `artifacts/release/` + 测试记录 | underrun/silence_fill 落在 gate 阈值内，JSON 含 tested_at/device/duration |
| M-02 | 4 档模式切换重同步回归 | `apps/android_flutter` Kotlin 播放层 + 测试 | 切换后 5s 窗口 `silence_fill<10`、`audio_track_write_frames_per_sec>0` |
| M-03 | ultra_low_latency 延迟实测 | `scripts/export_latency_probe.ps1` + `artifacts/latency/` | 新增 ultra 档 p95 artifact，不改动主路径默认值 |
| M-04 | v1.15 UI 视觉基线 Before/After | `artifacts/ui_baseline/android/` | 同场景/视口/主题两态截图 + 差异说明 |
| M-05 | ~~刷新 `docs/known_issues.md` / `docs/architecture.md`~~ | 两个文档 | 最终完善轮已完成：v1.15 架构 + 未验证项清单，无"已稳定"措辞 |

## 9. 接手结论

代码主路径、回滚路径、发布门控、版本一致性、生产入口测试、规则文档这六条接手基线现已**落地并可用**；`check_version_sync.ps1` / `assert_release_gate.ps1` / `cargo fmt` / `flutter analyze` / `flutter test` 全套实跑通过，`cargo check` / `cargo test` 受本机缺 MSVC linker 阻塞（已记录，不写成通过）。真机长稳与部分历史 known_issue 仍是**未验证**，按 P0 排队。当前停在 `v1.15` 已发布之后的"继续灰度/继续修复"状态，本轮不发版、不提交、不打 tag。
