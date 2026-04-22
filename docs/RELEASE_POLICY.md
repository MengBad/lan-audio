# Release Policy

## Rewrite Freeze (`2026-04-22`)

- Release is frozen until `artifacts/release/acceptance_gate.json` changes from `continue_fixing` to `allow_release`.
- `scripts/package_release.ps1`, `scripts/release.ps1`, and `.github/workflows/release.yml` now fail fast on any missing field or failed threshold in that gate file.
- `scripts/package_release.ps1` may build artifacts once all non-artifact gate fields pass, and it is responsible for flipping the local artifact presence flags plus `release_decision` after a successful package run.
- The maintained main-path target remains `windows_loopback + v2_header + opus`.
- The maintained rollback path remains `legacy_las1 + pcm16`.
- `rollback_verified` now depends on `desktop_headless --force-rollback` producing `artifacts/validate/rollback_evidence.json` with `active_data_plane=legacy_las1` and `codec=pcm16`.
- Current repo decision is `continue_fixing`, not release-ready.

## 褰撳墠鐗堟湰

- 褰撳墠鐗堟湰锛堢煭鐗堟湰锛夛細`1.3`
- 鐗堟湰鍞竴鏉ユ簮锛氫粨搴撴牴鐩綍 `VERSION`

璇存槑锛?
- `VERSION` 浣跨敤 `major.minor`锛堝 `1.0`銆乣1.1`锛夈€?- 闇€瑕佸啓鍏ラ渶瑕佽涔夊寲鐗堟湰鍙风殑宸ョ▼鏂囦欢鏃讹紝鏄犲皠涓?`major.minor.0`锛堝 `1.1.0`锛夈€?- Android `versionCode` 浣跨敤 `2000 + major * 100 + minor`锛屼緥濡?`1.1 -> 2101`銆傝瑙勫垯鐢ㄤ簬鍏煎鏃╂湡鐪熸満娴嬭瘯鍖?`2100`锛岄伩鍏嶆寮?APK 琚郴缁熷垽瀹氫负 downgrade銆?
## 鍙戝竷鍓嶆彁

浠ヤ笅鏉′欢蹇呴』鍏ㄩ儴婊¤冻锛?
1. 鏈疆鐩爣宸插畬鎴愬埌鍙氦浠樼姸鎬?2. 鏈湴楠岃瘉閫氳繃锛屾垨澶辫触椤瑰凡鏄庣‘涓斾笉褰卞搷鍙戝竷
3. 褰撳墠鍒嗘敮鏃犳槑鏄鹃樆濉為棶棰?4. 鍏抽敭鏂囨。宸插悓姝ワ紙README銆乼odo銆乸rotocol銆乵igration锛?5. 淇濈暀鍙洖婊氳矾寰?
## 鏈湴楠岃瘉鏍囧噯

缁熶竴鎵ц锛歚scripts/validate_local.ps1`

璇ヨ剼鏈細鎸夐『搴忔墽琛岋細

1. `cargo fmt --all -- --check`
2. `cargo check`
3. `cargo test -p lan_audio_protocol -p lan_audio_server`
4. `cargo check -p lan_audio_desktop`
5. `flutter analyze`
6. `flutter test`
7. `android/gradlew.bat assembleDebug`

## 鐗堟湰閫掑瑙勫垯

缁熶竴閫氳繃 `scripts/bump_version.ps1` 鎵ц锛岀姝㈡墜宸ュ澶勬敼鐗堟湰銆?
榛樿琛屼负锛?
- 涓嶅甫鍙傛暟鏃讹細minor +1锛坄1.0 -> 1.1`锛?- 鍙樉寮忔寚瀹氾細`-Version 1.2`

鍚屾鐩爣锛?
- `VERSION`
- `Cargo.toml`锛坵orkspace.version锛?- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/tauri.conf.json`
- `apps/android_flutter/pubspec.yaml`
- `apps/android_flutter/android/local.properties`
- `README.md`锛堣嚜鍔ㄥ寲鐗堟湰娈碉級
- `docs/todo.md`锛堣嚜鍔ㄥ寲鐗堟湰娈碉級
- `docs/RELEASE_POLICY.md`锛堝綋鍓嶇増鏈锛?
## 鍙戝竷娴佺▼锛堢粺涓€鍏ュ彛锛?
鎺ㄨ崘鍏ュ彛锛歚scripts/release.ps1`

娴佺▼锛?
1. 妫€鏌?Git 宸ヤ綔鍖虹姸鎬侊紙榛樿涓嶅厑璁歌剰宸ヤ綔鍖猴級
2. 鎵ц鏈湴楠岃瘉锛堥粯璁ゆ墽琛岋級
3. 鎵ц鐗堟湰閫掑骞跺悓姝?4. 鎵ц `scripts/package_release.ps1` 鐢熸垚鏈湴 release 浜х墿
5. 鐢熸垚 release commit锛坄chore(release): vX.Y`锛?6. 鍒涘缓 tag锛坄vX.Y`锛?7. 鎺ㄩ€佸垎鏀笌 tag
8. 鐢?GitHub Actions 瀹屾垚 CI 涓?Release 宸ヤ綔娴?
鏈湴鎵撳寘鍏ュ彛锛歚scripts/package_release.ps1`

榛樿浜х墿锛?
- Android锛歚dist/release/android/`锛屾寜 ABI 鎷嗗垎鐨?release APK锛岀敤浜庨檷浣庡崟鍖呬綋绉?- Windows锛歚dist/release/windows/lan-audio-desktop-<version>.exe`
- 鏍￠獙锛歚dist/release/SHA256SUMS.txt`

## GitHub Actions 绛栫暐

- `ci.yml`锛氱粺涓€ CI锛圧ust + Flutter + Android锛?- `build-android.yml`锛氭瀯寤?debug APK 涓?split-per-ABI release APK
- `build-windows-client.yml`锛氬彧鏋勫缓 Windows release exe锛屼笉鍐嶆瀯寤?MSI/NSIS
- `release.yml`锛氬熀浜?tag锛坄v*`锛夋瀯寤?release APK / Windows exe锛屽苟鍒涘缓 GitHub Release 鑽夌

鍙戝竷鍘熷垯锛?
- 涓嶅厑璁歌烦杩?CI 鐩存帴鍙戝竷銆?- 鑻?CI 澶辫触锛孯elease 缁存寔鑽夌鎴栦笉鍙戝竷锛岄渶鍏堜慨澶嶃€?
## 鍥炴粴绛栫暐

鍙戝竷鍚庡彂鐜板紓甯告椂锛屼紭鍏堟寜浠ヤ笅璺緞鍥炴粴锛?
1. 鏁版嵁闈㈠洖婊氬埌 `legacy_las1`
2. 淇濈暀 `synthetic + v2_header` 浣滀负蹇€熼獙璇佽矾寰?3. 蹇呰鏃跺洖閫€鍒颁笂涓€涓?tag 鐗堟湰

## 鍙戝竷璁板綍瑕佹眰

Release notes 鑷冲皯鍖呭惈锛?
- Protocol v2 褰撳墠闃舵
- 榛樿涓昏矾寰?- 宸查獙璇佽寖鍥?/ 鏈獙璇佽寖鍥?- 涓昏椋庨櫓涓庡凡鐭ラ檺鍒?- 鍥炴粴鏂瑰紡

