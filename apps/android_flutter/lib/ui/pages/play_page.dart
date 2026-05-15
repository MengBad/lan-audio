import 'package:flutter/material.dart';

import '../audio_console_status.dart';
import '../audio_console_theme.dart';
import '../widgets/danger_action_button.dart';
import '../widgets/latency_chart_widget.dart';
import '../widgets/mode_selector_widget.dart';

class PlayPage extends StatelessWidget {
  const PlayPage({
    super.key,
    required this.isZh,
    required this.consoleState,
    required this.statusChipLabel,
    required this.statusText,
    required this.isConnecting,
    required this.wsConnected,
    required this.playbackStopped,
    required this.metricBufferText,
    required this.metricUnderrunText,
    required this.tcpRoundTripMs,
    required this.modeItems,
    required this.currentModeId,
    required this.modeSelectorEnabled,
    required this.onModeSelected,
    required this.onStopPlayback,
    required this.onRetryConnection,
    required this.serverName,
    required this.currentLatencyMs,
    required this.baselineLatencyMs,
    required this.effectiveCodec,
    required this.sampleRate,
    required this.channels,
  });

  final bool isZh;
  final ConsoleUiState consoleState;
  final String statusChipLabel;
  final String statusText;
  final bool isConnecting;
  final bool wsConnected;
  final bool playbackStopped;
  final String metricBufferText;
  final String metricUnderrunText;
  final int? tcpRoundTripMs;
  final List<ModeSelectorItem> modeItems;
  final String currentModeId;
  final bool modeSelectorEnabled;
  final ValueChanged<String> onModeSelected;
  final VoidCallback onStopPlayback;
  final VoidCallback onRetryConnection;
  final String? serverName;

  /// Live read of the current end-to-end latency (ms). Sampled by the chart.
  final ValueGetter<double?> currentLatencyMs;

  /// Live read of the pre-optimization baseline latency (ms). Sampled by the
  /// chart. Returning the same value across frames produces a flat reference
  /// line.
  final ValueGetter<double?> baselineLatencyMs;

  /// Negotiated codec on the wire (`opus` / `pcm16` / etc).
  final String effectiveCodec;

  /// Negotiated sample rate (Hz). Reads `_sampleRate` from `MainShell`.
  final int sampleRate;

  /// Negotiated channel count. Reads `_channels` from `MainShell`.
  final int channels;

  String tr(String zh, String en) => isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    final isPlaying = consoleState == ConsoleUiState.streaming ||
        consoleState == ConsoleUiState.buffering;

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 16),
      child: Column(
        children: [
          const Spacer(flex: 1),
          // Large animated status orb
          AnimatedContainer(
            duration: AudioConsoleMotion.orbPulse,
            width: 64,
            height: 64,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: isPlaying
                  ? AudioConsoleColors.teal
                  : consoleState == ConsoleUiState.error
                      ? AudioConsoleColors.error
                      : AudioConsoleColors.bg3,
              border: !isPlaying && consoleState != ConsoleUiState.error
                  ? Border.all(color: AudioConsoleColors.teal, width: 1.5)
                  : null,
              boxShadow: isPlaying
                  ? [
                      BoxShadow(
                        color: AudioConsoleColors.tealGlow,
                        blurRadius: 24,
                        spreadRadius: 4,
                      ),
                    ]
                  : null,
            ),
            child: Icon(
              isPlaying
                  ? Icons.volume_up
                  : consoleState == ConsoleUiState.error
                      ? Icons.error_outline
                      : Icons.wifi_off,
              size: 32,
              color: isPlaying
                  ? Colors.white
                  : consoleState == ConsoleUiState.error
                      ? Colors.white
                      : AudioConsoleColors.teal,
            ),
          ),
          const SizedBox(height: 16),
          // Status text
          Text(
            isPlaying ? tr('正在播放', 'Now Playing') : tr('未连接', 'Disconnected'),
            style: const TextStyle(
              fontSize: 20,
              fontWeight: FontWeight.w700,
              color: AudioConsoleColors.text,
            ),
          ),
          const SizedBox(height: 4),
          // Server name
          Text(
            serverName ?? tr('无服务器', 'No server'),
            style: const TextStyle(
              fontSize: 13,
              color: AudioConsoleColors.text2,
            ),
          ),
          const SizedBox(height: 28),
          // Mode selector pills
          ModeSelectorWidget(
            items: modeItems,
            selectedId: currentModeId,
            enabled: modeSelectorEnabled,
            onSelected: onModeSelected,
          ),
          const SizedBox(height: 20),
          // Retry button (error state)
          if (consoleState == ConsoleUiState.error) ...[
            FilledButton.tonal(
              key: const Key('retry_action'),
              onPressed: isConnecting ? null : onRetryConnection,
              child: Text(tr('重试连接', 'Retry Connection')),
            ),
            const SizedBox(height: 12),
          ],
          // Stop button
          DangerActionButton(
            label: tr('停止播放', 'Stop Playback'),
            enabled: wsConnected || !playbackStopped,
            onPressed: (wsConnected || !playbackStopped) ? onStopPlayback : null,
          ),
          const SizedBox(height: 16),
          // Thin divider
          Divider(
            color: AudioConsoleColors.border,
            height: 1,
          ),
          const SizedBox(height: 12),
          // One-line metrics
          Text(
            '${tr('缓冲', 'Buf')} ${metricBufferText}ms · '
            '${tr('欠载', 'Underrun')} $metricUnderrunText · '
            'RTT ${tcpRoundTripMs ?? '-'}ms',
            style: AudioConsoleType.monoMeta(),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 14),
          // Latency comparison chart (Phase 2 visualization)
          LatencyChartWidget(
            currentLatencyMs: currentLatencyMs,
            baselineLatencyMs: baselineLatencyMs,
            isZh: isZh,
          ),
          const SizedBox(height: 8),
          // Audio quality strip — codec / sample rate / channels.
          // Hidden when no session has been negotiated yet.
          if (wsConnected)
            _AudioQualityStrip(
              codec: effectiveCodec,
              sampleRate: sampleRate,
              channels: channels,
              isZh: isZh,
            ),
          const Spacer(flex: 2),
        ],
      ),
    );
  }
}

/// Compact "now playing" audio-quality strip rendered just below the latency
/// chart. Shows the negotiated codec, sample rate, and channel count in the
/// Audio Console Dark style. Designed to feel like an Apple-Music-esque
/// passive readout — no controls, no taps.
class _AudioQualityStrip extends StatelessWidget {
  const _AudioQualityStrip({
    required this.codec,
    required this.sampleRate,
    required this.channels,
    required this.isZh,
  });

  final String codec;
  final int sampleRate;
  final int channels;
  final bool isZh;

  String tr(String zh, String en) => isZh ? zh : en;

  String _codecLabel() {
    switch (codec.toLowerCase()) {
      case 'opus':
        return 'Opus';
      case 'pcm16':
        return 'PCM 16';
      case 'f32':
        return 'PCM Float';
      default:
        return codec.toUpperCase();
    }
  }

  String _sampleRateLabel() {
    if (sampleRate >= 1000) {
      final khz = sampleRate / 1000.0;
      // Drop trailing .0 for whole-kHz rates (48000 -> "48 kHz").
      if (khz == khz.truncateToDouble()) {
        return '${khz.toInt()} kHz';
      }
      return '${khz.toStringAsFixed(1)} kHz';
    }
    return '$sampleRate Hz';
  }

  String _channelsLabel() {
    switch (channels) {
      case 1:
        return tr('单声道', 'Mono');
      case 2:
        return tr('立体声', 'Stereo');
      default:
        return tr('$channels 声道', '$channels ch');
    }
  }

  @override
  Widget build(BuildContext context) {
    final monoStyle = AudioConsoleType.monoMeta();
    final dotStyle = monoStyle.copyWith(color: AudioConsoleColors.text2);
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Text(_codecLabel(), style: monoStyle),
        Text(' · ', style: dotStyle),
        Text(_sampleRateLabel(), style: monoStyle),
        Text(' · ', style: dotStyle),
        Text(_channelsLabel(), style: monoStyle),
      ],
    );
  }
}
