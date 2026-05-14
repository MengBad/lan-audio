import 'package:flutter/material.dart';

import '../../connect_history.dart';
import '../audio_console_status.dart';
import '../audio_console_theme.dart';
import '../widgets/danger_action_button.dart';
import '../widgets/metric_chip_widget.dart';
import '../widgets/mode_selector_widget.dart';

class HomePage extends StatelessWidget {
  const HomePage({
    super.key,
    required this.isZh,
    required this.consoleState,
    required this.statusChipLabel,
    required this.statusText,
    required this.isConnecting,
    required this.wsConnected,
    required this.playbackStopped,
    required this.metricBufferText,
    required this.metricFpsText,
    required this.metricUnderrunText,
    required this.underrunCount,
    required this.modeItems,
    required this.currentModeId,
    required this.modeSelectorEnabled,
    required this.onModeSelected,
    required this.onStopPlayback,
    required this.onRetryConnection,
    required this.servers,
    required this.mostRecentHost,
    required this.onConnectQuickRecent,
    required this.connectHistory,
    required this.onConnectHistoryEntry,
    required this.onRemoveHistoryEntry,
    required this.onShowHistoryActions,
  });

  final bool isZh;
  final ConsoleUiState consoleState;
  final String statusChipLabel;
  final String statusText;
  final bool isConnecting;
  final bool wsConnected;
  final bool playbackStopped;
  final String metricBufferText;
  final String metricFpsText;
  final String metricUnderrunText;
  final int underrunCount;
  final List<ModeSelectorItem> modeItems;
  final String currentModeId;
  final bool modeSelectorEnabled;
  final ValueChanged<String> onModeSelected;
  final VoidCallback onStopPlayback;
  final VoidCallback onRetryConnection;
  final List<_QuickConnectData> servers;
  final String? mostRecentHost;
  final VoidCallback onConnectQuickRecent;
  final List<ConnectHistoryEntry> connectHistory;
  final void Function(ConnectHistoryEntry) onConnectHistoryEntry;
  final void Function(ConnectHistoryEntry) onRemoveHistoryEntry;
  final void Function(ConnectHistoryEntry) onShowHistoryActions;

  String tr(String zh, String en) => isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 20),
      children: [
        // Hero status card
        Card(
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Row(
              children: [
                AnimatedContainer(
                  duration: const Duration(milliseconds: 300),
                  width: 48,
                  height: 48,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: consoleState == ConsoleUiState.streaming ||
                            consoleState == ConsoleUiState.buffering
                        ? const Color(0xFF00D4AA)
                        : consoleState == ConsoleUiState.error
                            ? const Color(0xFFFF4444)
                            : const Color(0xFF2A3550),
                    border: consoleState == ConsoleUiState.idle ||
                            consoleState == ConsoleUiState.connecting
                        ? Border.all(
                            color: const Color(0xFF00D4AA), width: 1.5)
                        : null,
                  ),
                  child: Icon(
                    consoleState == ConsoleUiState.streaming ||
                            consoleState == ConsoleUiState.buffering
                        ? Icons.volume_up
                        : consoleState == ConsoleUiState.error
                            ? Icons.error_outline
                            : Icons.wifi_off,
                    size: 28,
                    color: consoleState == ConsoleUiState.streaming ||
                            consoleState == ConsoleUiState.buffering
                        ? Colors.white
                        : consoleState == ConsoleUiState.error
                            ? Colors.white
                            : const Color(0xFF00D4AA),
                  ),
                ),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        statusChipLabel,
                        style: TextStyle(
                          fontSize: 20,
                          fontWeight: FontWeight.w700,
                          color: Theme.of(context).colorScheme.onSurface,
                        ),
                      ),
                      Text(
                        statusText,
                        style: TextStyle(
                          fontSize: 13,
                          color: Theme.of(context)
                              .colorScheme
                              .onSurface
                              .withValues(alpha: 0.6),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),
        // Metric chips row
        Row(
          children: [
            Expanded(
              child: MetricChipWidget(
                label: tr('缓冲', 'buffer ms'),
                value: metricBufferText,
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: MetricChipWidget(
                label: tr('接收', 'rx fps'),
                value: metricFpsText,
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: MetricChipWidget(
                label: tr('欠载', 'underrun'),
                value: metricUnderrunText,
                valueColor: underrunCount > 0
                    ? AudioConsoleColors.amber
                    : AudioConsoleColors.text,
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        // Mode selector
        ModeSelectorWidget(
          items: modeItems,
          selectedId: currentModeId,
          enabled: modeSelectorEnabled,
          onSelected: onModeSelected,
        ),
        const SizedBox(height: 12),
        // Retry button (error state)
        if (consoleState == ConsoleUiState.error)
          FilledButton.tonal(
            key: const Key('retry_action'),
            onPressed: isConnecting ? null : onRetryConnection,
            child: Text(tr('重试连接', 'Retry Connection')),
          ),
        if (consoleState == ConsoleUiState.error) const SizedBox(height: 8),
        // Stop playback button
        DangerActionButton(
          label: tr('停止播放', 'Stop Playback'),
          enabled: wsConnected || !playbackStopped,
          onPressed: (wsConnected || !playbackStopped) ? onStopPlayback : null,
        ),
        const SizedBox(height: 12),
        // Quick connect card
        _buildQuickConnectCard(context),
        const SizedBox(height: 10),
        // Connect history card
        _buildConnectHistoryCard(context),
      ],
    );
  }

  Widget _buildQuickConnectCard(BuildContext context) {
    if (mostRecentHost == null) {
      return const SizedBox.shrink();
    }
    final matched =
        servers.where((s) => s.host == mostRecentHost).toList();
    final wsPort = matched.isNotEmpty ? matched.first.wsPort : 39991;
    final udpPort = matched.isNotEmpty ? matched.first.udpPort : 39992;
    final name = matched.isNotEmpty
        ? matched.first.serverName
        : 'recent:$mostRecentHost';

    return Card(
      color: Theme.of(context).colorScheme.primaryContainer,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.flash_on, size: 18),
                const SizedBox(width: 6),
                Text(tr('快速连接', 'Quick Connect'),
                    style: const TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(width: 8),
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.primary,
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Text(tr('最近连接', 'Recent'),
                      style:
                          const TextStyle(color: Colors.white, fontSize: 11)),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text('$name ($mostRecentHost) ws:$wsPort udp:$udpPort'),
            const SizedBox(height: 10),
            FilledButton(
              onPressed: isConnecting ? null : onConnectQuickRecent,
              child: Text(tr('一键连接最近设备', 'Connect Recent Server')),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildConnectHistoryCard(BuildContext context) {
    if (connectHistory.isEmpty) {
      return const SizedBox.shrink();
    }
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(tr('连接历史', 'Connection History'),
                style:
                    const TextStyle(fontWeight: FontWeight.w700, fontSize: 16)),
            const SizedBox(height: 8),
            ...connectHistory.map((entry) {
              return Dismissible(
                key: ValueKey('${entry.ip}:${entry.port}'),
                direction: DismissDirection.endToStart,
                background: Container(
                  alignment: Alignment.centerRight,
                  padding: const EdgeInsets.only(right: 16),
                  color: Colors.red.shade600,
                  child: const Icon(Icons.delete, color: Colors.white),
                ),
                onDismissed: (_) => onRemoveHistoryEntry(entry),
                child: ListTile(
                  dense: true,
                  leading: Icon(
                    entry.isFavorite ? Icons.star : Icons.history,
                    color: entry.isFavorite ? Colors.amber.shade700 : null,
                  ),
                  title: Text(entry.hostname),
                  subtitle: Text(
                    '${entry.ip}:${entry.port}  ${tr('延迟', 'latency')}:${entry.lastLatencyMs}ms  ${tr('次数', 'count')}:${entry.connectCount}',
                  ),
                  onTap: isConnecting
                      ? null
                      : () => onConnectHistoryEntry(entry),
                  onLongPress: () => onShowHistoryActions(entry),
                ),
              );
            }),
          ],
        ),
      ),
    );
  }
}

/// Lightweight data class to pass server info to HomePage without
/// exposing the full DiscoveryServer model.
class QuickConnectServerData {
  const QuickConnectServerData({
    required this.host,
    required this.wsPort,
    required this.udpPort,
    required this.serverName,
  });

  final String host;
  final int wsPort;
  final int udpPort;
  final String serverName;
}

typedef _QuickConnectData = QuickConnectServerData;
