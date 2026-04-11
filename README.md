# LAN Phone Speaker MVP (Windows -> Android)

当前仓库已可在本机进入“可构建、可启动、可联调准备”状态，技术路线保持不变：Rust + Tauri + Flutter + UDP + WebSocket（当前主路径 PCM 直通）。

## 本机实测状态（2026-04-12, Asia/Shanghai）

### 1) 已修复并验证通过

- 环境检查：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1` 通过
  - `cargo --version` / `rustup --version` / `flutter --version` / `adb version` / `java -version` 均可执行
- Rust workspace：
  - `cargo metadata --no-deps` 成功
- 服务端可执行目标：
  - `desktop_headless` 已构建（`target/debug/desktop_headless.exe`）
  - `scripts/run_server_headless.ps1` 可拉起进程（服务是常驻进程）
- Android 构建：
  - `scripts/run_android_gradle.ps1` 成功（`assembleDebug`）
  - `scripts/run_android_client.ps1 -BuildApk` 成功
  - APK 产物：`apps/android_flutter/build/app/outputs/flutter-apk/app-debug.apk`

### 2) 已修复但当前机器仍受外部条件限制

- `adb devices` 当前无在线设备，尚未在这台机器完成“安装到真机并实际出声”验收。
- Android 构建期间存在 AGP/Kotlin 版本即将弃用警告，不影响当前 debug 构建。

### 3) 仍需你手动安装的外部工具

- 当前无需新增手动安装清单（核心工具链已补齐）。
- 若后续要做真机验收，请手动确保：
  - 手机已开启开发者模式/USB 调试
  - USB 驱动可用并在 `adb devices` 中出现 `device`

## 可执行目标与命令

### 服务端（Windows）

- 目标：`desktop_headless`
- 脚本启动：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource synthetic
```

- 等价命令：

```powershell
cargo run -p lan_audio_server --bin desktop_headless -- --audio-source synthetic
```

可选参数（脚本）：

- `-AudioSource windows_loopback`
- `-NoAudioFallback`
- `-CaptureDumpWav`
- `-CaptureDumpSeconds 8`
- `-CaptureDumpDir debug_captures`

### Android 客户端

- Flutter 构建 APK：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1 -BuildApk
```

- Flutter 运行到设备：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

- 仅 Gradle 调试：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_gradle.ps1
```

## 快速联调步骤

1. 环境检查：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_env.ps1
```

2. 启动服务端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_server_headless.ps1 -AudioSource windows_loopback
```

3. 连接手机并确认：

```powershell
adb devices
```

4. 启动 Android 客户端：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run_android_client.ps1
```

5. App 内执行：发现设备 -> 连接 -> 开始播放。

## 文档

- `docs/dev_setup.md`
- `docs/local_simulation.md`
- `docs/audio_capture.md`
- `docs/architecture.md`
- `docs/todo.md`
- `docs/known_issues.md`
