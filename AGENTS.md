# AGENTS.md

## 目标

本仓库目标是交付一个“Windows 电脑在局域网向 Android 手机实时传音频，手机充当音响”的双端产品。

当前长期主线：

1. 播放稳定性
2. 延迟优化
3. 多策略模式（low_latency / balanced / high_quality）
4. Protocol v2 演进
5. 产品化 UI 与桌面端交付

## Codex 默认执行路径

每次接到任务，默认必须按下列顺序执行，不允许只做其中一段：

1. 读取规则与上下文文档
   - `README.md`
   - `docs/todo.md`
   - `docs/protocol.md`
   - `docs/protocol_v2_migration.md`
   - `docs/desktop_ui.md`
   - `docs/RELEASE_POLICY.md`（如存在）
2. 判断任务归属主线
   - 稳定性 / 延迟优化 / 模式策略 / 协议演进 / UI 产品化 / 发布管理
3. 执行实现与验证
   - 改代码
   - 补或修测试
   - 更新文档
   - 运行本地检查
4. 判断是否满足发布条件
   - 若满足，按发布流程执行
   - 若不满足，明确停在“继续灰度/继续修复”
5. 需要发版时，先通过 `scripts/package_release.ps1` 生成 Android release APK 与 Windows exe
6. 按固定格式汇报

## 当前运行与迁移原则

1. 默认主路径保持安全
   - 当前默认数据面主路径必须保持稳定路径（目前为 `legacy_las1`）
   - 不能在证据不足时强切主路径
2. 必须保留回滚路径
   - `legacy_las1`
   - `synthetic + v2_header`
   - 协议灰度开关路径
3. Protocol v2 目标
   - 支撑模式同步
   - 支撑能力协商与参数重同步
   - 为后续 Opus / 智能模式预留
   - 以灰度替代一次性硬切

## 每次任务必须执行的本地检查

能运行就必须运行（可使用 `scripts/validate_local.ps1` 一键执行）：

- `cargo fmt --all -- --check`
- `cargo check`
- `cargo test -p lan_audio_protocol -p lan_audio_server`
- `cargo check -p lan_audio_desktop`
- `flutter analyze`
- `flutter test`
- `android/gradlew.bat assembleDebug`
- 发版前额外执行：`scripts/package_release.ps1`

命令失败时：

1. 先修复可修复问题
2. 重跑验证
3. 如仍失败，必须在汇报中写清失败原因与影响范围

## 文档更新要求

以下内容发生变化时，必须同步更新文档：

- 协议状态 / 主路径 / 灰度范围
- 默认模式或开关行为
- 版本号
- 回滚路径
- 验收结论
- 发布规则

至少同步这些文件：

- `README.md`
- `docs/todo.md`
- `docs/protocol.md`
- `docs/protocol_v2_migration.md`
- `docs/RELEASE_POLICY.md`（涉及发布时）

## 发布触发条件

仅当以下条件全部满足，才允许进入发布动作：

1. 本轮目标达到可交付状态
2. 本地检查通过，或失败项已明确且不影响发布判断
3. 仓库不存在明显阻塞
4. 文档已同步
5. 代码审查认为可发布

不满足任一项时，禁止发布。

## 发布流程

达到发布条件后，按此顺序执行：

1. 运行 `scripts/validate_local.ps1`
2. 运行 `scripts/package_release.ps1` 做本地 release 产物预检
3. 运行 `scripts/release.ps1`（内部执行版本递增、release 打包、提交、打 tag、推送）
4. 检查 GitHub Actions
5. Actions 通过后创建/发布 Release

Release 说明必须包含：

- 当前 Protocol v2 状态
- 默认主路径
- codec 状态
- 已验证与未验证范围
- 回滚方式

## 版本号规则

- 统一版本源：仓库根目录 `VERSION`（短版本，格式 `major.minor`，例如 `1.0`）
- 默认递增：`1.0 -> 1.1 -> 1.2 -> ...`
- 同步目标：
  - Git tag / GitHub Release（`v<major.minor>`）
  - Windows 桌面版本（Cargo / Tauri）
  - Android 版本（pubspec + local.properties）
  - 文档中的当前版本信息
- Android `versionCode` 采用 `2000 + major * 100 + minor`（例如 `1.1 -> 2101`），避免低于历史测试包导致安装 downgrade。

## 禁止事项

- 不要只写计划不落地
- 不要只改文档不改脚本/流程
- 不要只改代码不更新文档
- 不要删除回滚路径
- 不要把“未验证”写成“已稳定”
- 不要绕过 CI 直接发布

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
