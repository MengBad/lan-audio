# AGENTS.md

## 目标

本仓库目标是交付一个"Windows 电脑在局域网向 Android 手机实时传音频，手机充当音响"的双端产品。

当前长期主线：

1. 播放稳定性
2. 延迟优化
3. 多策略模式（ultra_low_latency / low_latency / balanced / high_quality）
4. Protocol v2 / v3 演进
5. 产品化 UI 与桌面端交付

当前版本：**v1.17**（统一版本源：仓库根目录 `VERSION`）

## Codex 默认执行路径

每次接到任务，默认必须按下列顺序执行，不允许只做其中一段：

1. 读取规则与上下文文档
   * `README.md`
   * `docs/todo.md`
   * `docs/roadmap.md`
   * `docs/protocol.md`
   * `docs/protocol_v2_migration.md`
   * `docs/desktop_ui.md`
   * `docs/RELEASE_POLICY.md`
   * `docs/MAINTENANCE_PLAN.md`
2. 判断任务归属主线
   * 稳定性 / 延迟优化 / 模式策略 / 协议演进 / UI 产品化 / 发布管理 / MediaSession / 更新检测
3. 执行实现与验证
   * 改代码
   * 补或修测试
   * 更新文档
   * 运行本地检查
4. 判断是否满足发布条件
   * 若满足，按发布流程执行
   * 若不满足，明确停在"继续灰度/继续修复"
5. 需要发版时，先通过 `scripts/package_release.ps1` 生成 Android release APK 与 Windows exe
6. 按固定格式汇报

## 当前运行与迁移原则

1. 默认主路径保持稳定
   * 当前默认数据面主路径：`windows_loopback + v2_header + opus`
   * 回滚路径必须始终保留：`legacy_las1 + pcm16`
   * 不能在证据不足时改变主路径
2. 必须保留回滚路径
   * `legacy_las1 + pcm16`
   * `synthetic + v2_header`
   * 协议灰度开关路径
3. Protocol v2 / v3 目标
   * 支撑模式同步、能力协商与参数重同步
   * v3 仅用于 Hi-Res PCM24 分片，默认不启用
   * 为后续智能模式预留
   * 以灰度替代一次性硬切

## 每次任务必须执行的本地检查

能运行就必须运行（可使用 `scripts/validate_local.ps1` 一键执行）：

* `cargo fmt --all -- --check`
* `cargo check`
* `cargo test -p lan_audio_protocol -p lan_audio_server`
* `cargo check -p lan_audio_desktop`
* `flutter analyze`
* `flutter test`（发布前必须单独运行 `flutter test test/app_entry_smoke_test.dart`）
* `android/gradlew.bat assembleDebug`
* `scripts/check_version_sync.ps1`（版本一致性）
* `scripts/assert_release_gate.ps1`（发布门控契约；CI `version-sync` job 同样执行这两项）

## 发布门控 checklist

- [ ] `apps/android_flutter/lib/main.dart` 中仍调用 `buildAudioConsoleTheme()`
- [ ] 生产入口挂载 3-tab `IndexedStack`：`PlayPage` / `ConnectPage` / `SettingsPage`
- [ ] `MoreServerData` 仍作为 `ConnectPage` 的服务端数据契约（`MorePage` 自 v1.15 起不再挂载）
- [ ] `PlayPage` 挂载 `ModeSelectorWidget` / `DangerActionButton` / `LatencyChartWidget`
- [ ] `PlaybackForegroundService.onCreate()` 不含 `FOREGROUND_SERVICE_TYPE_MICROPHONE`
- [ ] `MicCaptureService.start()` 权限检查在 `AudioRecord` 采集之前
- [ ] Kotlin TCP 写帧与 Rust 读帧字节序一致（均为小端）
- [ ] `LatencyChartWidget` 实时数据链路端到端联通
- [ ] `ReverseChannelServer::start/stop` 已 wire 进服务生命周期
- [ ] `VERSION` 文件为纯 ASCII 无 BOM
- [ ] `app_entry_smoke_test` 覆盖当前生产入口全部核心 widget
- [ ] 生产入口不出现 `MVP` 标题
- [ ] `scripts/check_version_sync.ps1` 通过
- [ ] `scripts/assert_release_gate.ps1` 通过
- [ ] 发布前运行 `flutter test test/app_entry_smoke_test.dart`

## 强制发布覆盖（FORCE_RELEASE）

当任务 prompt 包含 `FORCE_RELEASE=true` 时，发布门控中"真机验证 / 长稳样本 / 发布 checklist"视为人工已确认：

规则：
- 通过 `FORCE_RELEASE=true` 环境变量或 `scripts/release.ps1 -ForceRelease` 触发；`release.ps1` 把 `human_override` 写回 `artifacts/release/acceptance_gate.json` 并随发布提交
- 未完成的人工验收项标注 `[human-override]` 而非 `[x]`
- 编译与本地验证仍必须通过（`scripts/validate_local.ps1`）
- 仍需运行 `scripts/package_release.ps1` 和 `scripts/release.ps1`
- Release 说明中注明"本版本经人工判定发布，部分验收项以 human-override 标记"
- `FORCE_RELEASE` 只覆盖 `device_acceptance_passed` / `rewrite_validate_passed` / `acceptance_json_present` / 真机决策 / `release_decision=continue_fixing`；不跳过 `validate_local_passed` / `rollback_verified` / 编译、打包、CI、阻塞码与主路径-回滚契约检查

## 发布流程

达到发布条件后，按此顺序执行：

1. 运行 `scripts/validate_local.ps1`
2. 运行 `scripts/package_release.ps1` 做本地 release 产物预检
3. 运行 `scripts/release.ps1`（内部执行版本递增、release 打包、提交、打 tag、推送）
4. 检查 GitHub Actions
5. Actions 通过后创建/发布 Release

Release 说明必须包含：

* 当前 Protocol v2 / v3 状态
* 默认主路径
* codec 状态
* 已验证与未验证范围
* 新功能状态（MediaSession / 更新检测）
* 回滚方式

## 版本号规则

* 统一版本源：仓库根目录 `VERSION`（短版本 `major.minor`，支持 `major.minor.patch`）
* 默认递增：`1.3 -> 1.4 -> 1.5 -> ...`
* 同步目标：
  + Git tag / GitHub Release（`v<major.minor>` 或 `v<major.minor.patch>`）
  + Windows 桌面版本（Cargo / Tauri）
  + Android 版本（pubspec + local.properties + `kAppVersion`）
  + 文档中的当前版本信息
* Android `versionCode` 采用 `2000 + major * 1000 + minor * 10 + patch`（patch 缺省按 `0` 处理）
* 服务端 mDNS `version` 字段必须由 `env!("CARGO_PKG_VERSION")` 派生，禁止硬编码

## 禁止事项

* 不要只写计划不落地
* 不要只改文档不改脚本/流程
* 不要只改代码不更新文档
* 不要删除回滚路径
* 不要把"未验证"写成"已稳定"
* 不要绕过 CI 直接发布
* MediaSession 集成不能影响 AudioTrack / Oboe 播放路径
* 更新检测必须是静默后台请求，不能阻塞启动流程
* 更新检测不做自动下载安装，只引导用户跳转 GitHub Release 页面

## 最终汇报格式

每次任务完成后必须按以下结构汇报：

1. 本轮总览
2. 代码改动
3. 协议/模式/codec 当前状态
4. 稳定性与延迟相关影响
5. 本地验证结果
6. 文档更新
7. 是否达到发布条件
8. 若已发布：版本号、Actions、Release 结果
9. 下一步唯一建议
