import 'package:flutter/material.dart';

import '../../services/mic_capture_service.dart';
import '../../widgets/capsule_button.dart';
import '../audio_console_theme.dart';
import '../../../main.dart' show kAppVersion;
import '../mic_status_widget.dart';

/// Settings tab page (index 2) with grouped card layout.
///
/// Organizes settings into 4 sections:
/// - 播放设置 (Playback) — codec + buffer controls
/// - 音频处理 (Audio Processing) — EQ + loudness
/// - 麦克风 (Microphone) — mic reverse channel controls
/// - 应用设置 (App Settings) — language, theme, update, debug
///
/// Each section is wrapped in a dark surface card with rounded corners.
/// Actual controls within each section are implemented in tasks 6.2–6.4.
class SettingsPage extends StatelessWidget {
  const SettingsPage({
    super.key,
    required this.isZh,
    // Codec
    required this.preferredCodec,
    required this.onPreferredCodecChanged,
    // Buffer
    this.customBufferMs,
    this.onBufferMsChanged,
    // EQ
    required this.eqEnabled,
    required this.eqLowDb,
    required this.eqMidDb,
    required this.eqHighDb,
    required this.onSetEq,
    required this.onApplyEqPreset,
    // Mic
    required this.micService,
    required this.micEnabled,
    required this.serviceTargetHost,
    required this.reverseChannelPort,
    required this.onToggleMic,
    // Loudness
    required this.loudnessNormalizationEnabled,
    required this.onSetLoudnessNormalization,
    // App settings
    required this.lang,
    required this.onLangChanged,
    required this.onOpenPowerSavingGuide,
    required this.onCheckUpdate,
    required this.updateCheckRunning,
    required this.autoConnectEnabled,
    required this.onAutoConnectChanged,
    this.audioFocusExclusive = false,
    this.onAudioFocusChanged,
    // Debug
    required this.debugData,
  });

  final bool isZh;

  // ── Codec ──
  /// `null` means "server default / auto".
  final String? preferredCodec;
  final ValueChanged<String?> onPreferredCodecChanged;

  // ── Buffer ──
  final int? customBufferMs;
  final ValueChanged<int?>? onBufferMsChanged;

  // ── Equalizer ──
  final bool eqEnabled;
  final int eqLowDb;
  final int eqMidDb;
  final int eqHighDb;
  final void Function({bool? enabled, int? lowDb, int? midDb, int? highDb})
      onSetEq;
  final void Function(String preset) onApplyEqPreset;

  // ── Mic ──
  final MicCaptureService micService;
  final bool micEnabled;
  final String? serviceTargetHost;
  final int reverseChannelPort;
  final Future<void> Function() onToggleMic;

  // ── Loudness ──
  final bool loudnessNormalizationEnabled;
  final ValueChanged<bool> onSetLoudnessNormalization;

  // ── App settings ──
  final String lang;
  final ValueChanged<String> onLangChanged;
  final VoidCallback onOpenPowerSavingGuide;
  final VoidCallback onCheckUpdate;
  final bool updateCheckRunning;
  final bool autoConnectEnabled;
  final ValueChanged<bool> onAutoConnectChanged;
  final bool audioFocusExclusive;
  final ValueChanged<bool>? onAudioFocusChanged;

  // ── Debug ──
  /// Key-value pairs for the debug info card.
  final Map<String, String> debugData;

  String tr(String zh, String en) => isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AudioConsoleColors.bg,
      body: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(
            AudioConsoleSpacing.lg,
            AudioConsoleSpacing.sm,
            AudioConsoleSpacing.lg,
            AudioConsoleSpacing.xl,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // ─── Section: 播放设置 (Playback) ───
              _buildSection(
                title: tr('播放设置', 'Playback'),
                child: _buildPlaybackContent(),
              ),

              // ─── Section: 音频处理 (Audio Processing) ───
              _buildSection(
                title: tr('音频处理', 'Audio Processing'),
                child: _buildAudioProcessingContent(),
              ),

              // ─── Section: 麦克风 (Microphone) ───
              _buildSection(
                title: tr('麦克风', 'Microphone'),
                child: _buildMicrophoneContent(),
              ),

              // ─── Section: 应用设置 (App Settings) ───
              _buildSection(
                title: tr('应用设置', 'App Settings'),
                child: _buildAppSettingsContent(),
              ),
            ],
          ),
        ),
      ),
    );
  }

  // ─── Playback Section (Codec + Buffer) ───

  Widget _buildPlaybackContent() {
    final bool customBufferEnabled = customBufferMs != null;
    final int displayBufferMs = customBufferMs ?? 80;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // ── Codec selector ──
        Text(
          tr('编解码器', 'Codec'),
          style: AudioConsoleType.body(color: AudioConsoleColors.text2),
        ),
        const SizedBox(height: AudioConsoleSpacing.xs),
        Wrap(
          spacing: AudioConsoleSpacing.xs,
          runSpacing: AudioConsoleSpacing.xs,
          children: [
            CapsuleButton(
              key: const Key('settings_codec_auto'),
              label: tr('自动', 'Auto'),
              selected: preferredCodec == null,
              onTap: () => onPreferredCodecChanged(null),
            ),
            CapsuleButton(
              label: 'Opus (${tr('推荐', 'Recommended')})',
              selected: preferredCodec == 'opus',
              onTap: () => onPreferredCodecChanged('opus'),
            ),
            CapsuleButton(
              label: 'PCM 16-bit',
              selected: preferredCodec == 'pcm16',
              onTap: () => onPreferredCodecChanged('pcm16'),
            ),
            CapsuleButton(
              label: 'AAC',
              selected: preferredCodec == 'aac',
              onTap: () => onPreferredCodecChanged('aac'),
            ),
          ],
        ),

        const SizedBox(height: AudioConsoleSpacing.xl),

        // ── Buffer display ──
        Text(
          tr('缓冲区', 'Buffer'),
          style: AudioConsoleType.body(color: AudioConsoleColors.text2),
        ),
        const SizedBox(height: AudioConsoleSpacing.xs),
        // Large teal number with "ms" suffix
        Row(
          crossAxisAlignment: CrossAxisAlignment.baseline,
          textBaseline: TextBaseline.alphabetic,
          children: [
            Text(
              '$displayBufferMs',
              style: AudioConsoleType.monoValue().copyWith(fontSize: 32),
            ),
            const SizedBox(width: 4),
            Text(
              'ms',
              style: AudioConsoleType.monoValue().copyWith(fontSize: 16),
            ),
          ],
        ),
        const SizedBox(height: AudioConsoleSpacing.xxs),
        // Subtitle: recommended range
        Text(
          tr('推荐: 60-120ms', 'Recommended: 60-120ms'),
          style: AudioConsoleType.monoMeta(color: AudioConsoleColors.text2),
        ),

        const SizedBox(height: AudioConsoleSpacing.md),

        // ── Custom buffer toggle ──
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              tr('自定义缓冲', 'Custom Buffer'),
              style: AudioConsoleType.body(),
            ),
            Switch(
              value: customBufferEnabled,
              onChanged: (enabled) {
                if (enabled) {
                  onBufferMsChanged?.call(80);
                } else {
                  onBufferMsChanged?.call(null);
                }
              },
              activeThumbColor: AudioConsoleColors.teal,
            ),
          ],
        ),

        // ── Buffer slider (visible when custom buffer enabled) ──
        AnimatedSize(
          duration: AudioConsoleMotion.stateTransition,
          curve: Curves.easeInOut,
          alignment: Alignment.topCenter,
          child: customBufferEnabled
              ? Column(
                  children: [
                    const SizedBox(height: AudioConsoleSpacing.xs),
                    SliderTheme(
                      data: SliderThemeData(
                        activeTrackColor: AudioConsoleColors.teal,
                        inactiveTrackColor: AudioConsoleColors.surface2,
                        thumbColor: AudioConsoleColors.teal,
                        overlayColor: AudioConsoleColors.tealDim,
                        trackHeight: 4,
                        thumbShape:
                            const RoundSliderThumbShape(enabledThumbRadius: 7),
                      ),
                      child: Slider(
                        min: 10,
                        max: 200,
                        divisions: 19,
                        value: displayBufferMs.toDouble().clamp(10, 200),
                        onChanged: (v) {
                          onBufferMsChanged?.call(v.round());
                        },
                      ),
                    ),
                    // Min/max labels
                    Row(
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Text(
                          '10ms',
                          style: AudioConsoleType.monoMeta(
                              color: AudioConsoleColors.text3),
                        ),
                        Text(
                          '200ms',
                          style: AudioConsoleType.monoMeta(
                              color: AudioConsoleColors.text3),
                        ),
                      ],
                    ),
                    const SizedBox(height: AudioConsoleSpacing.xxs),
                    Align(
                      alignment: Alignment.centerRight,
                      child: TextButton(
                        key: const Key('settings_buffer_reset'),
                        onPressed: () => onBufferMsChanged?.call(null),
                        child: Text(tr('重置为默认', 'Reset to default')),
                      ),
                    ),
                  ],
                )
              : const SizedBox.shrink(),
        ),
      ],
    );
  }

  // ─── Audio Processing Section ───

  Widget _buildAudioProcessingContent() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // EQ enable/disable switch
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              tr('均衡器', 'Equalizer'),
              style: AudioConsoleType.body(),
            ),
            Switch(
              value: eqEnabled,
              onChanged: (value) => onSetEq(enabled: value),
              activeTrackColor: AudioConsoleColors.tealDim,
              activeThumbColor: AudioConsoleColors.teal,
            ),
          ],
        ),

        // EQ preset chips
        const SizedBox(height: AudioConsoleSpacing.xs),
        Wrap(
          spacing: AudioConsoleSpacing.xs,
          runSpacing: AudioConsoleSpacing.xs,
          children: [
            CapsuleButton(
              key: const Key('eq_preset_flat'),
              label: tr('平直', 'Flat'),
              selected: false,
              onTap: () => onApplyEqPreset('flat'),
            ),
            CapsuleButton(
              key: const Key('eq_preset_bass'),
              label: tr('低音增强', 'Bass'),
              selected: false,
              onTap: () => onApplyEqPreset('bass'),
            ),
            CapsuleButton(
              key: const Key('eq_preset_vocal'),
              label: tr('人声清晰', 'Vocal'),
              selected: false,
              onTap: () => onApplyEqPreset('vocal'),
            ),
            CapsuleButton(
              key: const Key('eq_preset_bright'),
              label: tr('高频亮丽', 'Bright'),
              selected: false,
              onTap: () => onApplyEqPreset('bright'),
            ),
          ],
        ),

        // EQ bars with animated expand/collapse
        AnimatedSize(
          duration: AudioConsoleMotion.stateTransition,
          curve: Curves.easeInOut,
          alignment: Alignment.topCenter,
          child: eqEnabled
              ? Column(
                  children: [
                    const SizedBox(height: AudioConsoleSpacing.sm),
                    _buildEqBar(
                      label: tr('低', 'Low'),
                      valueDb: eqLowDb,
                      onChanged: (v) => onSetEq(lowDb: v),
                    ),
                    const SizedBox(height: AudioConsoleSpacing.xs),
                    _buildEqBar(
                      label: tr('中', 'Mid'),
                      valueDb: eqMidDb,
                      onChanged: (v) => onSetEq(midDb: v),
                    ),
                    const SizedBox(height: AudioConsoleSpacing.xs),
                    _buildEqBar(
                      label: tr('高', 'High'),
                      valueDb: eqHighDb,
                      onChanged: (v) => onSetEq(highDb: v),
                    ),
                  ],
                )
              : const SizedBox.shrink(),
        ),

        const SizedBox(height: AudioConsoleSpacing.lg),

        // Divider between EQ and loudness
        Divider(color: AudioConsoleColors.border, height: 1),
        const SizedBox(height: AudioConsoleSpacing.lg),

        // Loudness normalization toggle
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              tr('响度标准化', 'Loudness Normalization'),
              style: AudioConsoleType.body(),
            ),
            Switch(
              value: loudnessNormalizationEnabled,
              onChanged: onSetLoudnessNormalization,
              activeTrackColor: AudioConsoleColors.tealDim,
              activeThumbColor: AudioConsoleColors.teal,
            ),
          ],
        ),
      ],
    );
  }

  /// Builds a single horizontal EQ bar with label, slider, and dB value.
  Widget _buildEqBar({
    required String label,
    required int valueDb,
    required ValueChanged<int> onChanged,
  }) {
    // Normalize dB value to 0.0–1.0 range for display (range: -10 to +10)
    final normalized = (valueDb + 10) / 20.0;

    return Row(
      children: [
        // Label
        SizedBox(
          width: 32,
          child: Text(
            label,
            style: AudioConsoleType.body(color: AudioConsoleColors.text2),
          ),
        ),
        const SizedBox(width: AudioConsoleSpacing.xs),
        // Slider
        Expanded(
          child: SliderTheme(
            data: SliderThemeData(
              activeTrackColor: AudioConsoleColors.teal,
              inactiveTrackColor: AudioConsoleColors.surface2,
              thumbColor: AudioConsoleColors.teal,
              overlayColor: AudioConsoleColors.tealDim,
              trackHeight: 4,
              thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 7),
            ),
            child: Slider(
              value: normalized.clamp(0.0, 1.0),
              onChanged: (v) {
                // Convert back from 0.0–1.0 to -10..+10 dB
                final db = ((v * 20) - 10).round();
                onChanged(db);
              },
            ),
          ),
        ),
        const SizedBox(width: AudioConsoleSpacing.xs),
        // dB value — fixed width, right-aligned
        SizedBox(
          width: 48,
          child: Text(
            '${valueDb > 0 ? '+' : ''}${valueDb}dB',
            textAlign: TextAlign.right,
            style: AudioConsoleType.monoMeta(color: AudioConsoleColors.teal),
          ),
        ),
      ],
    );
  }

  // ─── Microphone Section ───

  Widget _buildMicrophoneContent() {
    return MicStatusWidget(
      service: micService,
      host: serviceTargetHost,
      reversePort: reverseChannelPort,
      enabled: micEnabled,
      onToggle: onToggleMic,
      isZh: isZh,
    );
  }

  /// Builds the app settings content with language, theme, update, toggles, and debug info.
  Widget _buildAppSettingsContent() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // ── Language selector ──
        Text(
          tr('语言', 'Language'),
          style: AudioConsoleType.body(color: AudioConsoleColors.text2),
        ),
        const SizedBox(height: AudioConsoleSpacing.xs),
        Row(
          children: [
            CapsuleButton(
              label: '中文',
              selected: lang == 'zh',
              onTap: () => onLangChanged('zh'),
            ),
            const SizedBox(width: AudioConsoleSpacing.xs),
            CapsuleButton(
              label: 'English',
              selected: lang == 'en',
              onTap: () => onLangChanged('en'),
            ),
          ],
        ),

        const SizedBox(height: AudioConsoleSpacing.lg),

        // ── Theme selector ──
        Text(
          tr('主题', 'Theme'),
          style: AudioConsoleType.body(color: AudioConsoleColors.text2),
        ),
        const SizedBox(height: AudioConsoleSpacing.xs),
        Row(
          children: [
            CapsuleButton(
              label: tr('浅色', 'Light'),
              selected: false,
              onTap: () {},
            ),
            const SizedBox(width: AudioConsoleSpacing.xs),
            CapsuleButton(
              label: tr('深色', 'Dark'),
              selected: true, // Only dark theme exists currently
              onTap: () {},
            ),
            const SizedBox(width: AudioConsoleSpacing.xs),
            CapsuleButton(
              label: tr('自动', 'Auto'),
              selected: false,
              onTap: () {},
            ),
          ],
        ),

        const SizedBox(height: AudioConsoleSpacing.lg),

        // ── Auto-connect toggle ──
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              tr('自动连接', 'Auto Connect'),
              style: AudioConsoleType.body(),
            ),
            Switch(
              value: autoConnectEnabled,
              onChanged: onAutoConnectChanged,
              activeThumbColor: AudioConsoleColors.teal,
            ),
          ],
        ),

        // ── Audio focus exclusive toggle ──
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              tr('独占音频焦点', 'Exclusive Audio Focus'),
              style: AudioConsoleType.body(),
            ),
            Switch(
              value: audioFocusExclusive,
              onChanged: onAudioFocusChanged,
              activeThumbColor: AudioConsoleColors.teal,
            ),
          ],
        ),

        const SizedBox(height: AudioConsoleSpacing.lg),

        // ── Background playback guide ──
        ListTile(
          key: const Key('settings_background_playback'),
          contentPadding: EdgeInsets.zero,
          title: Text(tr('后台播放', 'Background Playback')),
          trailing: const Icon(Icons.chevron_right),
          onTap: onOpenPowerSavingGuide,
        ),

        // ── Check update button ──
        Center(
          child: updateCheckRunning
              ? const SizedBox(
                  width: 24,
                  height: 24,
                  child: CircularProgressIndicator(
                    strokeWidth: 2,
                    color: AudioConsoleColors.teal,
                  ),
                )
              : TextButton.icon(
                  onPressed: onCheckUpdate,
                  icon: const Icon(
                    Icons.refresh,
                    color: AudioConsoleColors.teal,
                    size: 18,
                  ),
                  label: Text(
                    tr('检查更新', 'Check Update'),
                    style: AudioConsoleType.buttonLabel(
                      color: AudioConsoleColors.teal,
                    ),
                  ),
                ),
        ),

        const SizedBox(height: AudioConsoleSpacing.lg),

        // ── Version ──
        ListTile(
          key: const Key('settings_version_row'),
          contentPadding: EdgeInsets.zero,
          title: Text(tr('版本', 'Version')),
          trailing: Text(
            kAppVersion,
            style: AudioConsoleType.monoMeta(color: AudioConsoleColors.text2),
          ),
        ),

        // ── Expandable debug info card ──
        _buildDebugInfoCard(),
      ],
    );
  }

  /// Builds the expandable debug info card with monospace key-value pairs.
  Widget _buildDebugInfoCard() {
    return Container(
      decoration: BoxDecoration(
        color: AudioConsoleColors.bg2,
        borderRadius: AudioConsoleRadius.card,
      ),
      child: Theme(
        data: ThemeData(
          dividerColor: Colors.transparent,
          splashColor: Colors.transparent,
          highlightColor: Colors.transparent,
        ),
        child: ExpansionTile(
          tilePadding: const EdgeInsets.symmetric(
            horizontal: AudioConsoleSpacing.md,
          ),
          childrenPadding: const EdgeInsets.fromLTRB(
            AudioConsoleSpacing.md,
            0,
            AudioConsoleSpacing.md,
            AudioConsoleSpacing.md,
          ),
          title: Text(
            tr('调试信息', 'Debug Info'),
            style: AudioConsoleType.body(color: AudioConsoleColors.text2),
          ),
          iconColor: AudioConsoleColors.text2,
          collapsedIconColor: AudioConsoleColors.text2,
          children: [
            ...debugData.entries.map(
              (entry) => Padding(
                padding: const EdgeInsets.only(bottom: AudioConsoleSpacing.xxs),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SizedBox(
                      width: 120,
                      child: Text(
                        entry.key,
                        style: TextStyle(
                          fontFamily: AudioConsoleType.monoFamily,
                          fontSize: 12,
                          color: AudioConsoleColors.text2,
                        ),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        entry.value,
                        style: TextStyle(
                          fontFamily: AudioConsoleType.monoFamily,
                          fontSize: 12,
                          color: AudioConsoleColors.text,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// Builds a settings section with a teal bold header and a dark surface card.
  Widget _buildSection({
    required String title,
    required Widget child,
  }) {
    return Padding(
      padding: const EdgeInsets.only(bottom: AudioConsoleSpacing.xl),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Section header — teal bold text
          Text(
            title,
            style: const TextStyle(
              fontFamily: AudioConsoleType.sansFamily,
              fontSize: 16,
              fontWeight: FontWeight.w700,
              color: AudioConsoleColors.teal,
            ),
          ),
          const SizedBox(height: AudioConsoleSpacing.xs),
          // Card container — dark surface with rounded corners
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(AudioConsoleSpacing.lg),
            decoration: BoxDecoration(
              color: AudioConsoleColors.surface,
              borderRadius: AudioConsoleRadius.card,
              boxShadow: AudioConsoleShadow.section,
            ),
            child: child,
          ),
        ],
      ),
    );
  }
}
