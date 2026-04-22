# TODO / Stub Tracking

## Phase 0 / Phase 1 Rewrite Freeze (`2026-04-22`)

- Release decision: `continue_fixing`
- Release gate: `artifacts/release/acceptance_gate.json`
- Current main-path target: `windows_loopback + v2_header + opus`
- Maintained rollback path: `legacy_las1 + pcm16`
- Shared contracts source: `crates/lan_audio_domain`
- Current phase scope completed in this round:
  - release entry freeze
  - `cargo fmt --check` drift fix
  - mode contracts
  - connection state machine
  - failure taxonomy
  - stable service snapshot schema
  - baseline / contracts / report artifacts
- Current release conclusion: do not publish; continue fixing

## Automation Baseline

- 褰撳墠鐗堟湰锛堢煭鐗堟湰锛夛細`1.3`
- [x] 浠撳簱绾ц鍒欐枃浠讹細`AGENTS.md`
- [x] 鍙戝竷瑙勫垯鏂囨。锛歚docs/RELEASE_POLICY.md`
- [x] 鏈湴楠岃瘉鑴氭湰锛歚scripts/validate_local.ps1`
- [x] 鐗堟湰閫掑鑴氭湰锛歚scripts/bump_version.ps1`
- [x] 鍙戝竷鍏ュ彛鑴氭湰锛歚scripts/release.ps1`
- [x] release 鎵撳寘鑴氭湰锛歚scripts/package_release.ps1`锛圓ndroid release split APK + Windows 鍗?exe锛?- [x] 缁熶竴 CI锛歚.github/workflows/ci.yml`
- [x] Release 宸ヤ綔娴侊細`.github/workflows/release.yml`锛堟瀯寤哄苟闄勫姞 Windows exe銆丄ndroid release APK銆丼HA256锛?- [ ] 涓嬩竴姝ワ細楠岃瘉棣栦釜 `v1.0` 涔嬪悗鐨勮嚜鍔ㄩ€掑鍙戝竷锛坄1.1`锛夊畬鏁撮棴鐜?
## Audio Capture (Windows)

- [x] 瀹炴祴鍙敤锛歐indows -> Android 宸插彲杩炴帴骞跺嚭澹帮紙鍗曡疆鐪熸満楠屾敹閫氳繃锛?
- [ ] 绋冲畾鎬т紭鍖栵細澶氳澶囧疄鏈洪獙璇侊紙涓嶅悓澹板崱/椹卞姩/閲囨牱鐜囷級
- [ ] 绋冲畾鎬т紭鍖栵細闀挎椂闂寸ǔ瀹氭€ч獙璇佷笌鏃堕挓婕傜Щ澶勭悊
- [ ] 绋冲畾鎬т紭鍖栵細鎵╁睍 mix format 鏀寔锛堟洿澶?extensible 鍒嗘敮锛?
- [ ] 绋冲畾鎬т紭鍖栵細瀹屾暣閲嶉噰鏍?閲嶉€氶亾绛栫暐

## Android Playback

- [x] 瀹炴祴鍙敤锛欰ndroid 鐪熸満鍙繛鎺ュ苟绋冲畾鍑哄０锛堜富瑙傚彲鐢紝鏃犱弗閲嶉樆濉烇級
- [x] synthetic 鍩虹嚎閾捐矾鍙敤浜庣ǔ瀹氬鐓т笌鎺掗殰
- [x] windows_loopback 宸插疄娴嬪彲鍑哄０锛堝綋鍓嶇粨璁猴細鍙敤浣嗕粛闇€绋冲畾鎬т紭鍖栵級
- [x] v8 淇锛歫itter buffer 绌洪槦鍒?underrun 鍚庝笉鍐嶇户缁帹杩?expected sequence锛岄伩鍏嶆甯告柊鍖呰璇垽涓?late
- [x] v14 淇锛氭湇鍔＄鍚屼竴瀹㈡埛绔?IP 浠呬繚鐣欎竴涓椿璺?UDP 娴侊紝閬垮厤閲嶈繛鍚庨噸澶嶆帹娴侀€犳垚缂撳啿鎶栧姩
- [x] v15 淇锛欰ndroid 鎾斁棰勭畻鏀逛负楂樼簿搴︽椂閽燂紝鍑忓皯闀挎湡灏戞秷璐瑰鑷寸殑缂撳啿鍫嗙Н
- [x] v16 澧炲姞鍚姩鐗堟湰鏃ュ織锛坄ui_build ...`锛夌敤浜庣‘璁よ澶囧疄闄呰繍琛屽寘鐗堟湰
- [x] v17 澧炲姞 WS 鏂紑鍚?UDP 鎺ㄦ祦 30s 淇濇椿绐楀彛锛岄檷浣庢帶鍒堕€氶亾鐬柇瀵艰嚧鐨勭珛鍗抽潤闊?
- [x] v18 瀹屾垚 UI 淇℃伅鏋舵瀯閲嶆帓锛圕onnection/Playback/Debug锛変笌鍗曚富鎸夐挳浜や簰
- [x] v19 鏀寔涓嫳鏂囧垏鎹笌绯荤粺璇█榛樿锛坺h -> 涓枃锛宱ther -> English锛?
- [x] v20 Android 鍙戠幇閾捐矾澧炲姞 MulticastLock 鏀寔锛屾彁鍗?UDP 骞挎挱鎺ユ敹绋冲畾鎬?
- [x] v21 澧炲姞灞€鍩熺綉涓诲姩鎺㈡祴鍏滃簳锛堝箍鎾け璐ユ椂浠嶅彲鍙戠幇 39991 鏈嶅姟锛?
- [x] v22 鎵弿缁撴灉鍛藉悕浼樺寲 + 鏈€杩戞垚鍔熻繛鎺ョ疆椤?
- [x] v23 杩炴帴浣撻獙鏀跺熬锛歊ecent 鏍囪銆佸揩閫熻繛鎺ュ崱鐗囥€佺┖鍒楄〃鍙戠幇寮曞銆佹壂鎻?loading 鎻愮ず銆侀娆′娇鐢ㄨ交閲忔彁绀?
- [x] v24 棣栧睆浜у搧鍖栨敹鏁涳細椤堕儴鎽樿銆佸崟涓?CTA銆佺鍙ｄ俊鎭笅娌夈€佽皟璇曞尯鍙屽垪鏍呮牸
- [x] v25 鏂板 Media3 鍚庡彴鎾斁鏈嶅姟楠ㄦ灦锛堝墠鍙伴€氱煡 + MediaSessionService + 鍛戒护/浜嬩欢閫氶亾锛?
- [x] v25 鏂板娓愯繘杩佺Щ寮€鍏?`kUseBackgroundPlaybackService`锛堢伆搴﹂樁娈甸粯璁?false锛宭egacy 璺緞淇濈暀锛?
- [x] v26 鍒囨崲鍚庡彴鏈嶅姟涓洪粯璁ら摼璺紙`kUseBackgroundPlaybackService=true`锛?
- [x] v26 鏂板鍚庡彴淇濇椿鍩虹鑳藉姏锛歚WAKE_LOCK` + `PARTIAL_WAKE_LOCK` + `WifiLock`
- [x] v27 淇鍚庡彴鏈嶅姟浜嬩欢绾跨▼宕╂簝锛圗ventChannel 鍥炶皟缁熶竴涓荤嚎绋嬶級
- [x] v28 淇鍚庡彴鏈嶅姟鏄庢枃绛栫暐鎷︽埅锛堝厑璁?LAN `ws://` cleartext锛?
- [x] v29 淇鍚庡彴閲嶈繛绔炴€侊紙鍘婚噸閲嶈繛 + 杩囨湡鍥炶皟闅旂锛?- [x] 鍚庡彴鎭㈠澧炲己锛氫繚瀛樻渶杩戞垚鍔熸挱鏀剧洰鏍囷紝`START_STICKY + AlarmManager` 鍦ㄤ换鍔＄Щ闄?鏈嶅姟鍥炴敹鍚庡皾璇曟仮澶嶈繛鎺?- [x] 鏂嚎閲嶈繛璇箟鏀舵暃锛歐ebSocket transient failure 杩涘叆 reconnecting锛屼笉鍐嶅厛鍙戝竷鑷村懡 error
- [x] 鑷姩閲嶈繛杈圭晫鏀舵暃锛氳繛鎺ュ紓甯镐腑鏂悗鏈€澶氳嚜鍔ㄩ噸杩?3 娆★紱閲嶅紑 App 鏃跺皾璇曟仮澶嶄笂涓€娆℃垚鍔熺殑鎺ㄦ祦鏈嶅姟鍣?- [x] 鑷姩閲嶈繛鐪熸満楠屾敹锛歚synthetic + v2_header + opus` 涓嬮獙璇佸紓甯告柇寮€鏈€澶?3 娆￠噸杩烇紝閲嶅紑 App 鍙仮澶嶄笂娆℃湇鍔″櫒
- [x] release 浣撶Н鏀舵暃锛歳elease 鏋勫缓鍚敤 R8/resource shrink锛屽彂甯?APK 鎸?ABI 鎷嗗垎
- [x] V2 妯″紡绛栫暐鎺ュ叆锛歚low_latency/balanced/high_quality` 宸叉槧灏勫埌 start/max buffer銆乥atch銆乨rop threshold銆佸悗绔亸濂?- [x] Android 浜у搧璇婃柇鍏ュ彛锛氭柊澧炶繛鎺ュ府鍔╂姌鍙犲尯锛堝悓缃戞銆丄P isolation銆佹壂鎻?鎵嬪姩鍦板潃銆乁SB銆佸悗鍙扮數姹犱紭鍖栵級
- [x] 棣栨浣跨敤鎻愮ず鏀逛负鎸佷箙鍖栧彧鎻愮ず涓€娆★紙涓嶅啀鍥?App 杩涚▼閲嶅惎閲嶅寮瑰嚭锛?
- [ ] 绋冲畾鎬т紭鍖栵細v26 鍚庡彴閾捐矾瀹炴満楠屾敹锛堥攣灞?鍒囧悗鍙?鐔勫睆杩炵画鎾斁锛屽鏈哄瀷锛?
- [ ] 绋冲畾鎬т紭鍖栵細澶氭満鍨?AudioTrack 绋冲畾鎬т笌寤惰繜璋冧紭锛堟湰杞凡鍒?Builder + LOW_LATENCY + reported latency 璇婃柇锛屼粛寰呯湡鏈虹‘璁?<=40ms锛?- [ ] 绋冲畾鎬т紭鍖栵細jitter buffer 鑷€傚簲绛栫暐锛堝綋鍓嶅浐瀹氳捣鎾紦鍐诧級
- [ ] 绋冲畾鎬т紭鍖栵細鎾斁绾跨▼浼樺厛绾?鎶楁姈鍔ㄥ寮?

## Opus

- [x] 宸ョ▼鍙帴鍏ョ姸鎬侊細鍗忚鏋氫妇銆乧apabilities銆佹湇鍔＄ `--codec opus`锛堝吋瀹规棫 `opus_experimental`锛夈€佹闈㈠叆鍙ｅ凡鍏峰
- [x] 鍥為€€绛栫暐锛氬綋鏈夋晥鏁版嵁闈笉鏄?`v2_header` 鏃讹紝Opus 璇锋眰浠嶈嚜鍔ㄥ洖閫€ PCM16锛屼笉鐮村潖鍙嚭澹颁富璺緞
- [x] 绋冲畾閾捐矾锛氭湇鍔＄鏍囧噯 libopus 缂栫爜 + Android `libopus` JNI 瑙ｇ爜宸叉帴鍏ワ紙鎺ㄨ崘璺緞 `v2_header + opus`锛?- [x] PLC 鍥為€€锛欰ndroid JNI decode 澶辫触鏃舵敼璧?libopus PLC concealment锛屼笉鐩存帴 silence
- [x] 5 鍒嗛挓鍘嬪姏娴嬭瘯锛歴ynthetic + Opus 鍥哄畾 20ms 甯ц繛缁紪鐮侀€氳繃锛宍p99 encode ~= 0.509 ms`锛宑hannel-full drop rate `0.000000`
- [ ] 绋冲畾鎬ч獙璇侊細Opus 涓?PCM16 鐨勭湡鏈哄欢杩熴€丆PU銆佷涪鍖呮仮澶嶅姣?
## Protocol Evolution (v2)

- [x] Protocol v2 鑽夋鏂囨。锛堟帶鍒堕潰/鏁版嵁闈?capabilities/杩佺Щ绛栫暐锛?
- [x] Rust 鍗忚缁撴瀯楠ㄦ灦锛圓udioMode銆丆apabilities銆丆ontrolMessageV2銆乁dpAudioHeaderV2锛?
- [x] 鎺у埗闈㈣仈鍔ㄥ凡鎺ラ€氾細`hello/hello_ack + client_info/server_info + set_audio_mode/audio_mode_changed`
- [x] V2 浣庡欢杩熶骇鍝佹ā鍨嬶細杩炴帴銆佸彂閫併€佹挱鏀俱€佸崗璁€佽瘖鏂簲绫昏兘鍔涘凡鍐欏叆 README/roadmap/protocol
- [x] `AudioModeProfile` 绛栫暐绯荤粺锛氬崗璁眰 + 鏈嶅姟绔?+ Android + 妗岄潰绔涔変竴鑷?
- [x] capabilities 鎵╁睍锛歠ast path銆乻table AudioTrack銆乁SB tethering銆乁SB direct future銆丱pus
- [x] 鏁版嵁闈㈠弻鏍堝噯澶囷細鏈嶅姟绔彲閫?`legacy_las1/v2_header`锛堟闈㈤粯璁?`v2_header`锛? 瀹㈡埛绔?`LAS1/LAV2` 鍙屾爤璇嗗埆
- [x] config_changed/discontinuity 鏈€灏忓鐞嗭細鏈嶅姟绔墦 flag + 瀹㈡埛绔渶灏忛噸鍚屾
- [x] 榛樿璺緞鍒囨崲锛歞esktop 榛樿鏀逛负 `windows_loopback + v2_header + opus`锛宍legacy_las1 + pcm16` 淇濈暀涓烘樉寮忓洖婊?- [x] synthetic + v2_header 鏈湴鐏板害楠屾敹锛圠AV2 璇嗗埆銆佹ā寮忓垏鎹€乫lags 涓庨噸鍚屾鑱旇皟閫氳繃锛?
- [x] synthetic + v2_header 鐪熸満鐏板害楠屾敹锛堢湡瀹?Android 璁惧瀹屾垚鎾斁銆佹ā寮忓垏鎹€佹寚鏍囬噰鏍凤紝缁撹锛氶€氳繃锛?
- [x] 鍙岀妯″紡鐘舵€佽仈鍔細鏈嶅姟绔寔鏈?`current_audio_mode`锛孉ndroid + Windows 鍙樉绀哄苟鍚屾锛堥粯璁?`balanced`锛?
- [x] `windows_loopback + v2_header` 宸叉檵鍗囦负鎺ㄨ崘榛樿璺緞锛涘畨鍏ㄦā寮忓彲鏄惧紡鍥炴粴鍒?`legacy_las1 + pcm16`
- [ ] 涓嬩竴闃舵锛氱ǔ瀹氭€т紭鍖栵紙妯″紡鍒囨崲鍚庣紦鍐插嘲鍊间笌 late frame 绱Н锛?
- [ ] 涓嬩竴闃舵锛歎SB tethering 浣庡欢杩熸牱鏈獙鏀讹紙Wi-Fi 涓?USB 鏍锋湰鍒嗗紑璁板綍锛?- [x] Phase 2锛歎SB direct 浼犺緭灞傞鏋讹紙`adb reverse` + `transport_mode=usb` + localhost TCP length-prefixed 鏁版嵁闈?+ Android USB 鍏ュ彛锛?- [ ] 涓嬩竴闃舵锛歄pus loopback 鐪熸満闀跨ǔ楠岃瘉锛堥粯璁ゅ凡鍒囨崲锛屽彂甯冨墠浠嶉渶琛ヨ冻瀹炴祴鏍锋湰锛?- [ ] 鐏板害鍚敤锛氬弻绔崗鍟嗗悗鎸夎繛鎺ュ姩鎬佸垏鎹㈠埌 v2 鏁版嵁闈?header锛堝綋鍓嶄粛浠ラ厤缃紑鍏充负涓伙級
- [ ] 鍏ㄩ噺鍚敤锛氶粯璁よ矾寰勫垏鎹㈠埌 v2锛屽苟淇濈暀 v1 鍥為€€绛栫暐

## v1.2 Phase 1 璁板綍

- 鏃ユ湡锛歚2026-04-21`
- 缁撹锛歅hase 1 浠ｇ爜璺緞宸插畬鎴愶紝鎺ㄨ崘榛樿璺緞宸插垏鍒?`windows_loopback + v2_header + opus`
- 宸插畬鎴愶細
  - Opus 缂栫爜鍥哄畾涓?20ms / 960 samples per channel锛屽榻愬眰宸插姞鍏?  - Android Opus decode 澶辫触鏃惰蛋 PLC concealment
  - `desktop_headless --help`銆乀auri UI銆丷EADME銆佸崗璁枃妗ｅ凡缁熶竴鍒?`opus`
  - Tauri UI 鏂板鈥滃畨鍏ㄦā寮忊€濆洖婊氭寜閽紝鍥炴粴鍒?`legacy_las1 + pcm16`
- 鏈畬鎴愶細
  - Android 鐪熸満 `balanced` 寤惰繜 <=40ms 澶嶆牳
  - 澶氭満鍨?/ 闀挎椂闂?/ USB 鏍锋湰琛ラ綈
- 鍙戝竷鍒ゆ柇锛氭殏涓嶅彂鐗堬紝缁х画鐏板害 / 缁х画淇

## loopback + v2_header 灏忔祦閲忕伆搴︾粨璁?
- 缁撹锛氬凡浠庣伆搴︽彁鍗囦负鎺ㄨ崘璺緞锛屽洖婊氳矾寰勪繚鐣?- 宸插埌浣嶏細
  - 榛樿鎺ㄨ崘璺緞锛歚windows_loopback + v2_header + opus`
  - 鍙洖婊氬埌 `windows_loopback + legacy_las1 + pcm16` 涓?`synthetic + v2_header + pcm16`
- 鏈疆楠屾敹锛?026-04-15锛夛細
  - Android 鐪熸満杩炵画鎾斁 >2 鍒嗛挓锛宍Playback=playing`
  - 妯″紡鍒囨崲宸茶鐩栵細`balanced -> low_latency -> high_quality -> balanced`
  - 鍒囨崲鍚庣疮璁★細`cfg_changed=3`, `discontinuity=4`
  - `rx_frames_per_sec鈮?9~101`锛宍audio_track_write_frames_per_sec鈮?9~101`
- 褰撳墠椋庨櫓锛?
  - 妯″紡鍒囨崲鍚?`buffered_ms` 宄板€煎彲杈?300
  - `dropped_late_frames` 鍙疮绉紙鏈疆鍒?104锛?
  - 鍙戝竷鍓嶄粛闇€琛ラ綈 Android 鐪熸満 latency / 澶氭満鍨嬫牱鏈?- 棰濆璇存槑锛?
  - 鏈疆鏃ュ織鏈嚭鐜?`capture source is not started`
  - 鍥炴粴璺緞淇濇寔鍙敤锛歚legacy_las1` / `synthetic + v2_header`

## Productization

- [x] Tauri 妗岄潰瀹㈡埛绔鐗堝彲鐢?UI + 鏈嶅姟鐘舵€佹帶鍒讹紙鍚姩/鍋滄/閲嶅惎銆侀煶棰戞簮鍒囨崲銆佽繛鎺ヤ俊鎭€佹姌鍙犺皟璇曞尯銆佷腑鑻卞弻璇級
- [x] Windows release 浜や粯璺緞鏀舵暃涓哄崟 exe锛圙itHub Actions 涓庢湰鍦?`package_release.ps1` 鍧囧彧浜у嚭 exe锛?- [x] 妗岄潰绔?V2 浜у搧鐘舵€佸睍绀猴細鍗忚璺緞銆佹ā寮忕瓥鐣ャ€乧odec銆佺伆搴︾姸鎬併€佹帹鑽愯繛鎺ユ柟寮?- [x] Android 绔?V2 浜у搧鐘舵€佸睍绀猴細杩炴帴鏉ユ簮銆佸崗璁矾寰勩€佹挱鏀惧悗绔€佺伆搴﹁矾寰勩€佹ā寮忕瓥鐣?
- [x] USB tethering 姝ｅ紡绾冲叆浣庡欢杩熸帹鑽愯矾寰勶紙褰撳墠涓鸿矾绾?鏂囨/鐘舵€佷綅锛屼笉鏄?USB direct 瀹炵幇锛?
- [ ] installer / firewall guidance
- [ ] structured logs export
- [ ] 妗岄潰绔繛鎺ヤ簩缁寸爜锛堝綋鍓嶄粎鏂囨湰鍦板潃澶嶅埗锛?
- [ ] 妗岄潰绔細璇濊鎯呮繁鍖栵紙褰撳墠浠呰繛鎺ユ暟 + 鏈€杩戣繛鎺ヨ澶囷級
- [ ] 鑷姩璇婃柇鎶ュ憡锛氬悓缃戞/AP isolation/鍚庡彴鐢垫睜浼樺寲/寤惰繜妯″紡寤鸿
- [x] 妗岄潰绔?USB 妯″紡鍖哄潡锛歛db 璁惧鍒楄〃銆佸惎鐢?鍋滅敤鎸夐挳銆佸綋鍓?serial/reverse 绔彛灞曠ず
- [x] Android 绔璁惧鎰熺煡锛氭樉绀衡€滃綋鍓嶅叡 N 鍙拌澶囪繛鎺ヤ腑鈥?
## v1.2 Phase 2 + Phase 4 璁板綍

- 鏃ユ湡锛歚2026-04-21`
- 缁撹锛歅hase 2锛圲SB 妯″紡锛変笌 Phase 4锛堝璁惧鍚屾挱锛変唬鐮佽矾寰勫凡鎺ュ叆锛岀户缁伆搴﹂獙璇侊紝涓嶅彂鐗堛€?- 宸插畬鎴愶細
  - 鏈嶅姟绔細`usb_transport`锛坄adb devices -l`銆乫orward setup/teardown锛夛紝`TransportMode::Usb`锛孶SB 鏁版嵁闈?TCP length-prefixed銆?  - 鏈嶅姟绔細`ClientRegistry` 澶氬鎴风骞挎挱锛坄MAX_CLIENTS=8`銆佹柇绾挎竻鐞嗐€佸け璐ュ崟鐐圭Щ闄ゃ€佹瘡瀹㈡埛绔嫭绔?mode锛夈€?  - 鍗忚锛歚client_list / client_joined / client_left` 鎺у埗娑堟伅銆?  - 妗岄潰绔細Tauri command `list_adb_devices / enable_usb_mode / disable_usb_mode` 涓?USB UI 鍖哄潡銆?  - Android锛歎SB 杩炴帴鍏ュ彛锛坙ocalhost锛夈€乀CP length-prefixed 鎺ユ敹銆佽瘖鏂樉绀轰紶杈撴ā寮忎笌 TCP RTT銆佸璁惧鏁伴噺鏄剧ず銆?  - 娴嬭瘯锛氭柊澧?integration test `multi_client_broadcast_handles_disconnect_and_rejects_over_limit`銆?- 鏈畬鎴愶細
  - Android 鐪熸満 USB 浣庡欢杩熸牱鏈笌澶氭満鍚屾椂鍚劅缁熶竴楠屾敹銆?  - 闀挎椂闂村璁惧绋冲畾鎬ф牱鏈ˉ榻愩€?- 鍙戝竷鍒ゆ柇锛氭殏涓嶅彂鐗堬紝缁х画鐏板害/缁х画淇銆?
## v1.3 Acceptance Close-Out (2026-04-21)

- [x] Phase 1 鎵ц瀹屾垚锛圵i-Fi 瀹炴満璺戝畬 alanced 10min + low_latency 5min锛屽疄娴嬫湭杈惧埌 underrun=0 鍜?low_latency buffered_ms < 40ms 鐩爣锛?- [x] Phase 2 鎵ц瀹屾垚锛圲SB 瀹炴満璺戝畬 5min锛屽疄娴?	cp_rtt_ms 涓?uffered_ms 鏈ǔ瀹氭弧瓒崇洰鏍囬槇鍊硷紱鏈鏂嚎閿欒琛岋級
- [x] Phase 4 鏈疆鎸夊彂甯冩寚浠よ烦杩囨祴璇曪紙榛樿鍙敤锛屾湭鍋氭柊澧炲疄娴嬶級
- [x] USB 鏂囨。鏈宸茬粺涓€涓?db reverse锛圓ndroid localhost 鎺у埗/鏁版嵁锛?

## v1.3 Blocker Fix (2026-04-21 → 2026-04-22)

- [x] Add `hello_ack.transport_type` (`wifi` / `usb`) and wire it from server `TransportMode`.
- [x] Android playback profile adds `TransportHint` and transport-aware overrides:
  - USB + low_latency: start/max=8/20, batch=1, drop=12
  - USB + balanced: start/max=15/30, batch=1, drop=20
  - WiFi + low_latency: start/max=20/50, batch=1, drop=35
  - WiFi + balanced: unchanged
- [x] Low-latency jitter guard: 50-frame arrival-window p95; temporary +5ms prefill (cap 30ms), auto-recover after 10s stable.
- [x] Diagnostics add `jitter_p95_ms` and `TCP RTT current/median` display.
- [x] USB jitter buffer underrun fix (2026-04-22): increased USB balanced mode startBufferMs 15→160, maxBufferMs 30→320, dropThresholdMs 20→280
- [x] Device acceptance passed: USB+synthetic underrun_delta=0, silence_fill_delta ≤7/5s, jitter buffer avg 202ms; WiFi+loopback underrun_delta=0, silence_fill_delta ≤15/5s
- [x] Release gate updated: known_blockers=0, device_acceptance_passed=true, acceptance_json_present=true

## Phase 3: Data Plane Abstraction

- [x] Extract three data plane paths into unified trait/interface:
  - `legacy_las1` (PCM16 only, no header)
  - `v2_header` (current main path: Opus/PCM16 with LAV2 header)
  - `usb_direct` (current USB localhost TCP wrapper for the existing length-prefixed stream)
- [x] Refactor server transport layer to use data plane abstraction
- [x] Expose `active_data_plane` / `rollback_available` in the shared service snapshot contract and keep Android/desktop parsing aligned
- [ ] Refactor Android receiver runtime to use the same data plane abstraction internally
- [ ] Add data plane capability negotiation to protocol
- [x] Document data plane contracts and migration path
