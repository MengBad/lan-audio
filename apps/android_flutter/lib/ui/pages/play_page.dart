import 'package:flutter/material.dart';

import '../audio_console_status.dart';
import '../audio_console_theme.dart';
import '../metrics_formatter.dart';
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
    required this.onConnectPressed,
    required this.onDisconnectPressed,
    required this.serverName,
    required this.currentLatencyMs,
    required this.baselineLatencyMs,
    required this.effectiveCodec,
    required this.sampleRate,
    required this.channels,
    this.lossPercent,
    this.isBluetoothOutput = false,
    this.networkQuality = 0,
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
  final VoidCallback onConnectPressed;
  final VoidCallback onDisconnectPressed;
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

  /// Estimated packet loss percentage (0–100). `null` when not streaming.
  final double? lossPercent;

  /// Whether the device is currently outputting audio via Bluetooth.
  final bool isBluetoothOutput;

  /// Network quality indicator: 0=bad, 1=ok, 2=good. Computed from RTT and loss.
  final int networkQuality;

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
            curve: Curves.easeInOut,
            width: AudioConsoleSizes.statusOrbDiameter,
            height: AudioConsoleSizes.statusOrbDiameter,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: isPlaying
                  ? AudioConsoleColors.teal
                  : consoleState == ConsoleUiState.error
                      ? AudioConsoleColors.error
                      : Colors.transparent,
              border: !isPlaying && consoleState != ConsoleUiState.error
                  ? Border.all(color: AudioConsoleColors.text2, width: 2.0)
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
                      ? Icons.error
                      : Icons.wifi_off,
              size: 40,
              color: isPlaying || consoleState == ConsoleUiState.error
                  ? Colors.white
                  : AudioConsoleColors.text2,
            ),
          ),
          const SizedBox(height: 16),
          // Status text — 20px bold, white (AudioConsoleColors.text)
          Text(
            isPlaying ? tr('正在播放', 'Now Playing') : tr('未连接', 'Disconnected'),
            style: const TextStyle(
              fontFamily: AudioConsoleType.sansFamily,
              fontSize: 20,
              fontWeight: FontWeight.w700,
              color: AudioConsoleColors.text,
            ),
          ),
          const SizedBox(height: 6),
          // Compact status chip + detail line from MainShell state.
          Container(
            key: const Key('play_status_chip'),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
            decoration: BoxDecoration(
              color: AudioConsoleColors.surface2,
              borderRadius: AudioConsoleRadius.button,
              border: Border.all(color: AudioConsoleColors.border),
            ),
            child: Text(
              statusChipLabel,
              key: const Key('play_status_chip_label'),
              textAlign: TextAlign.center,
              style: AudioConsoleType.monoMeta(color: AudioConsoleColors.teal),
            ),
          ),
          const SizedBox(height: 4),
          Text(
            statusText,
            key: const Key('play_status_text'),
            textAlign: TextAlign.center,
            style: AudioConsoleType.monoMeta(color: AudioConsoleColors.text2),
          ),
          const SizedBox(height: 8),
          // Server name — 13px gray (AudioConsoleColors.text2)
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                serverName ?? tr('无服务器', 'No server'),
                style: const TextStyle(
                  fontFamily: AudioConsoleType.sansFamily,
                  fontSize: 13,
                  color: AudioConsoleColors.text2,
                ),
              ),
              if (wsConnected) ...[
                const SizedBox(width: 6),
                Icon(
                  networkQuality == 2
                      ? Icons.signal_wifi_4_bar
                      : networkQuality == 1
                          ? Icons.network_wifi_3_bar
                          : Icons.signal_wifi_bad,
                  size: 16,
                  color: networkQuality == 2
                      ? AudioConsoleColors.teal
                      : networkQuality == 1
                          ? AudioConsoleColors.amber
                          : AudioConsoleColors.error,
                ),
              ],
            ],
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
          // Primary action button (Connect / Disconnect)
          SizedBox(
            width: double.infinity,
            child: ElevatedButton(
              key: const Key('primary_action'),
              onPressed: wsConnected ? onDisconnectPressed : onConnectPressed,
              style: ElevatedButton.styleFrom(
                backgroundColor: AudioConsoleColors.teal,
                foregroundColor: AudioConsoleColors.bg,
                shape: RoundedRectangleBorder(
                  borderRadius: AudioConsoleRadius.button,
                ),
                padding: const EdgeInsets.symmetric(vertical: 14),
                elevation: 0,
              ),
              child: Text(
                wsConnected
                    ? tr('断开连接', 'Disconnect')
                    : tr('连接服务器', 'Connect'),
                style: AudioConsoleType.buttonLabel(
                  color: AudioConsoleColors.bg,
                ),
              ),
            ),
          ),
          const SizedBox(height: 12),
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
          // One-line metrics — monospace gray, visible only when connected
          if (wsConnected)
            AnimatedOpacity(
              opacity: wsConnected ? 1.0 : 0.0,
              duration: AudioConsoleMotion.metricFade,
              child: Text(
                formatMetricsLine(
                  bufferMs: int.tryParse(metricBufferText) ?? 0,
                  underrunCount: int.tryParse(metricUnderrunText) ?? 0,
                  rttMs: tcpRoundTripMs,
                  lossPercent: lossPercent,
                ),
                style: AudioConsoleType.monoMeta(),
                textAlign: TextAlign.center,
              ),
            ),
          // Bluetooth latency warning
          if (isBluetoothOutput)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                tr('蓝牙输出会增加额外延迟', 'Bluetooth output adds extra latency'),
                style: AudioConsoleType.monoMeta().copyWith(
                  color: AudioConsoleColors.amber,
                  fontSize: 11,
                ),
                textAlign: TextAlign.center,
              ),
            ),
          const SizedBox(height: 14),
          // Latency comparison chart — visible only when connected
          if (wsConnected)
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

  @override
  Widget build(BuildContext context) {
    final parts = formatAudioQualityStrip(
      codec: codec,
      sampleRate: sampleRate,
      channels: channels,
      isZh: isZh,
    );
    final monoStyle = AudioConsoleType.monoMeta();
    final dotStyle = monoStyle.copyWith(color: AudioConsoleColors.text2);
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Text(parts.codec, style: monoStyle),
        Text(' · ', style: dotStyle),
        Text(parts.sampleRate, style: monoStyle),
        Text(' · ', style: dotStyle),
        Text(parts.channels, style: monoStyle),
      ],
    );
  }
}
