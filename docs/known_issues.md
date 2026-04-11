# Known Issues

## 当前仍存在的问题（已按本机实测更新，2026-04-12）

1. 真机联调外部条件
- `adb` 工具已可用，但当前 `adb devices` 没有在线设备。
- 影响：无法在本机完成“Android 真机安装 + 实际出声”最终验收。

2. Android 构建告警（不阻塞 debug 构建）
- AGP 8.3.2 对 compileSdk 36 给出兼容性提示。
- Kotlin 1.9.22 给出未来弃用提示。
- 影响：当前 `assembleDebug` 与 `flutter build apk --debug` 可成功；后续建议升级 AGP/Kotlin。

3. 服务端端口占用风险
- 若已有旧的 `desktop_headless.exe` 在运行，再次启动会出现 UDP 端口占用（os error 10048）。
- 处理：先结束旧进程后再启动。

## 已完成的阻塞清理

- 已补齐并验证：`cargo`、`rustup`、`flutter`、`adb`、`java`。
- 已修复 Android 侧：
  - `local.properties` 自动写入 `flutter.sdk` 与 `sdk.dir`
  - 非 ASCII 路径检查阻塞（通过 `android.overridePathCheck=true`）
  - Jetifier 内存问题（当前关闭 `android.enableJetifier`）
  - 启动主题资源缺失（已替换为系统主题）
- 已修复脚本 PATH 可见性：
  - `run_server_headless.ps1`
  - `run_android_client.ps1`
  - `run_android_gradle.ps1`

## 本机实测命令结果

- `powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1` -> 成功
- `powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic` -> 可启动（常驻进程）
- `powershell -ExecutionPolicy Bypass -File .\scripts\run_android_gradle.ps1` -> `assembleDebug` 成功
- `powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1 -BuildApk` -> 成功，产物：
  - `apps/android_flutter/build/app/outputs/flutter-apk/app-debug.apk`
