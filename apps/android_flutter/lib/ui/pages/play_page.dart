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
          const Spacer(flex: 2),
        ],
      ),
    );
  }
}
