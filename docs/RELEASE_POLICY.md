# Release Policy

## 当前版本

- 当前版本（短版本）：`1.0`
- 版本唯一来源：仓库根目录 `VERSION`

说明：

- `VERSION` 使用 `major.minor`（如 `1.0`、`1.1`）。
- 需要写入需要语义化版本号的工程文件时，映射为 `major.minor.0`（如 `1.1.0`）。

## 发布前提

以下条件必须全部满足：

1. 本轮目标已完成到可交付状态
2. 本地验证通过，或失败项已明确且不影响发布
3. 当前分支无明显阻塞问题
4. 关键文档已同步（README、todo、protocol、migration）
5. 保留可回滚路径

## 本地验证标准

统一执行：`scripts/validate_local.ps1`

该脚本会按顺序执行：

1. `cargo fmt --all -- --check`
2. `cargo check`
3. `cargo test -p lan_audio_protocol -p lan_audio_server`
4. `cargo check -p lan_audio_desktop`
5. `flutter analyze`
6. `flutter test`
7. `android/gradlew.bat assembleDebug`

## 版本递增规则

统一通过 `scripts/bump_version.ps1` 执行，禁止手工多处改版本。

默认行为：

- 不带参数时：minor +1（`1.0 -> 1.1`）
- 可显式指定：`-Version 1.2`

同步目标：

- `VERSION`
- `Cargo.toml`（workspace.version）
- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/tauri.conf.json`
- `apps/android_flutter/pubspec.yaml`
- `apps/android_flutter/android/local.properties`
- `README.md`（自动化版本段）
- `docs/todo.md`（自动化版本段）
- `docs/RELEASE_POLICY.md`（当前版本段）

## 发布流程（统一入口）

推荐入口：`scripts/release.ps1`

流程：

1. 检查 Git 工作区状态（默认不允许脏工作区）
2. 执行本地验证（默认执行）
3. 执行版本递增并同步
4. 生成 release commit（`chore(release): vX.Y`）
5. 创建 tag（`vX.Y`）
6. 推送分支与 tag
7. 由 GitHub Actions 完成 CI 与 Release 工作流

## GitHub Actions 策略

- `ci.yml`：统一 CI（Rust + Flutter + Android）
- `release.yml`：基于 tag（`v*`）创建 GitHub Release 草稿

发布原则：

- 不允许跳过 CI 直接发布。
- 若 CI 失败，Release 维持草稿或不发布，需先修复。

## 回滚策略

发布后发现异常时，优先按以下路径回滚：

1. 数据面回滚到 `legacy_las1`
2. 保留 `synthetic + v2_header` 作为快速验证路径
3. 必要时回退到上一个 tag 版本

## 发布记录要求

Release notes 至少包含：

- Protocol v2 当前阶段
- 默认主路径
- 已验证范围 / 未验证范围
- 主要风险与已知限制
- 回滚方式
