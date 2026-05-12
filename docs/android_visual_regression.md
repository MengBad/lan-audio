# Android Visual Regression Baseline

## Widget Golden Baseline

Key UI states are tracked with Flutter goldens:

- `idle`
- `connecting`
- `streaming`
- `error`

Resolutions:

- `phone_small` (`360x780`)
- `phone_large` (`412x915`)
- `tablet_portrait` (`800x1280`)

Update baselines:

```powershell
cd apps/android_flutter
flutter test --update-goldens test/ui_console_golden_test.dart
```

Run regression check:

```powershell
cd apps/android_flutter
flutter test test/ui_console_golden_test.dart
```

Golden output directory:

- `apps/android_flutter/test/goldens/`

## Real Device Multi-Resolution Screenshot Baseline

Use the adb capture script:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\capture_android_ui_baseline.ps1
```

Optional explicit device serial:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\capture_android_ui_baseline.ps1 -Serial 5391d451
```

Output directory:

- `artifacts/ui_baseline/android/<serial>_<timestamp>/`

Generated images are current main screen snapshots after app launch at each configured resolution.
These real-device captures are local-only verification artifacts and are intentionally ignored by Git.

## Diff Threshold Gate

Baseline directory (real device):

- `artifacts/ui_baseline/android/baseline_5391d451/`

One-shot gate (golden + capture + diff):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_android_visual_regression.ps1 -Serial 5391d451
```

Adjust diff threshold (default `0.03`):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_android_visual_regression.ps1 -Serial 5391d451 -DiffThreshold 0.02
```
