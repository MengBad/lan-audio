import 'package:flutter/material.dart';

import '../../services/mic_capture_service.dart';
import '../audio_console_theme.dart';
import '../mic_status_widget.dart';

class MorePage extends StatelessWidget {
  const MorePage({
    super.key,
    required this.isZh,
    // Connection
    required this.connectMode,
    required this.onConnectModeChanged,
    required this.isConnecting,
    required this.probeRunning,
    required this.nsdDiscoveryRunning,
    required this.discoveryTimedOut,
    required this.manualHostController,
    required this.servers,
    required this.selectedServerId,
    required this.onServerSelected,
    required this.isRecentHost,
    required this.onConnectSelected,
    required this.onConnectManual,
    required this.onConnectUsb,
    required this.onScanLan,
    required this.connectActionLabel,
    required this.wsConnected,
    required this.connectionStatusText,
    // Equalizer
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
    // App
    required this.onOpenPowerSavingGuide,
    required this.onCheckUpdate,
    required this.updateCheckRunning,
    // Debug
    required this.protocolVersion,
    required this.currentAudioModeLabel,
    required this.protocolPath,
    required this.experimentalPath,
    required this.effectiveCodec,
    required this.serverPlatform,
    required this.serverAppVersion,
    required this.negotiatedCapabilities,
    required this.sampleRate,
    required this.channels,
    required this.uiBufferedMs,
    required this.uiJitterBufferedMs,
    required this.uiTrackQueuedMs,
    required this.uiAudioTrackLatencyMs,
    required this.uiUnderrun,
    required this.uiDropped,
    required this.uiLate,
    required this.uiFloorHoldCount,
    required this.uiJitterP95Ms,
    required this.playbackBackend,
    required this.connectionPathLabel,
    required this.transportMode,
    required this.connectedClientCount,
    required this.tcpRoundTripMs,
    required this.tcpRoundTripMedianMs,
    required this.modeProfile,
    required this.loudnessGainDb,
    required this.isPlaying,
    required this.uiUdpPackets,
    required this.uiUdpBytes,
    required this.uiUdpLoss,
    required this.uiLastSeq,
    required this.wsLog,
  });

  final bool isZh;
  // Connection
  final int connectMode;
  final ValueChanged<int> onConnectModeChanged;
  final bool isConnecting;
  final bool probeRunning;
  final bool nsdDiscoveryRunning;
  final bool discoveryTimedOut;
  final TextEditingController manualHostController;
  final List<MoreServerData> servers;
  final String? selectedServerId;
  final ValueChanged<String> onServerSelected;
  final bool Function(String host) isRecentHost;
  final VoidCallback onConnectSelected;
  final VoidCallback onConnectManual;
  final VoidCallback onConnectUsb;
  final VoidCallback onScanLan;
  final String connectActionLabel;
  final bool wsConnected;
  final String connectionStatusText;
  // Equalizer
  final bool eqEnabled;
  final int eqLowDb;
  final int eqMidDb;
  final int eqHighDb;
  final void Function({bool? enabled, int? lowDb, int? midDb, int? highDb})
      onSetEq;
  final void Function(String preset) onApplyEqPreset;
  // Mic
  final MicCaptureService micService;
  final bool micEnabled;
  final String? serviceTargetHost;
  final int reverseChannelPort;
  final Future<void> Function() onToggleMic;
  // Loudness
  final bool loudnessNormalizationEnabled;
  final ValueChanged<bool> onSetLoudnessNormalization;
  // App
  final VoidCallback onOpenPowerSavingGuide;
  final VoidCallback onCheckUpdate;
  final bool updateCheckRunning;
  // Debug
  final int? protocolVersion;
  final String currentAudioModeLabel;
  final String protocolPath;
  final bool experimentalPath;
  final String effectiveCodec;
  final String? serverPlatform;
  final String? serverAppVersion;
  final Map<String, bool> negotiatedCapabilities;
  final int sampleRate;
  final int channels;
  final int uiBufferedMs;
  final int uiJitterBufferedMs;
  final int uiTrackQueuedMs;
  final int? uiAudioTrackLatencyMs;
  final int uiUnderrun;
  final int uiDropped;
  final int uiLate;
  final int uiFloorHoldCount;
  final int? uiJitterP95Ms;
  final String playbackBackend;
  final String connectionPathLabel;
  final String transportMode;
  final int connectedClientCount;
  final int? tcpRoundTripMs;
  final int? tcpRoundTripMedianMs;
  final Map<String, dynamic> modeProfile;
  final double loudnessGainDb;
  final bool isPlaying;
  final int uiUdpPackets;
  final int uiUdpBytes;
  final int uiUdpLoss;
  final int? uiLastSeq;
  final String wsLog;

  String tr(String zh, String en) => isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 20),
      children: [
        // ─── Section: 连接 (Connection) ───
        _sectionHeader(tr('连接', 'Connection')),
        const SizedBox(height: 8),
        Text(
          wsConnected
              ? connectionStatusText
              : tr('未连接', 'Disconnected'),
          style: TextStyle(
            fontSize: 14,
            color: wsConnected
                ? AudioConsoleColors.teal
                : AudioConsoleColors.text2,
          ),
        ),
        const SizedBox(height: 10),
        SegmentedButton<int>(
          segments: [
            ButtonSegment(
              value: 0,
              label: Text(tr('发现设备', 'Discovered')),
            ),
            ButtonSegment(
              value: 1,
              label: Text(tr('手动地址', 'Manual')),
            ),
            ButtonSegment(
              value: 2,
              label: Text(tr('USB（adb）', 'USB (adb)')),
            ),
          ],
          selected: <int>{connectMode},
          onSelectionChanged: (selection) {
            onConnectModeChanged(selection.first);
          },
        ),
        const SizedBox(height: 10),
        if (probeRunning || nsdDiscoveryRunning)
          Row(
            children: [
              const SizedBox(
                width: 14,
                height: 14,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  tr('正在搜索设备...', 'Searching for devices...'),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
        if (probeRunning || nsdDiscoveryRunning)
          const SizedBox(height: 10),
        if (connectMode == 1)
          TextField(
            controller: manualHostController,
            decoration: InputDecoration(
              border: const OutlineInputBorder(),
              labelText: tr('手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
              hintText: tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
            ),
          )
        else if (servers.isEmpty)
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              border: Border.all(color: AudioConsoleColors.border),
              borderRadius: BorderRadius.circular(10),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  tr(
                    '自动发现失败，可点击"扫描局域网"或手动输入服务器地址',
                    'Auto discovery failed. Try "Scan LAN" or enter server address manually.',
                  ),
                ),
                if (discoveryTimedOut)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Text(
                      tr('未发现设备，请手动输入',
                          'No devices found. Enter host manually.'),
                      style: const TextStyle(
                        color: AudioConsoleColors.text2,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                const SizedBox(height: 8),
                FilledButton.tonal(
                  onPressed: (probeRunning || nsdDiscoveryRunning)
                      ? null
                      : onScanLan,
                  child: Text(
                    (probeRunning || nsdDiscoveryRunning)
                        ? tr('扫描中...', 'Scanning...')
                        : tr('扫描局域网', 'Scan LAN'),
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  tr('提示：可切换到"手动地址"输入 IP。',
                      'Tip: switch to Manual and enter server IP.'),
                  style: const TextStyle(color: AudioConsoleColors.text2),
                ),
                ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  title: Text(tr('高级选项', 'Advanced')),
                  children: [
                    TextField(
                      controller: manualHostController,
                      decoration: InputDecoration(
                        border: const OutlineInputBorder(),
                        labelText: tr('手动服务器地址 (IPv4)',
                            'Manual server host (IPv4)'),
                        hintText:
                            tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
                      ),
                    ),
                    const SizedBox(height: 8),
                    Align(
                      alignment: Alignment.centerLeft,
                      child: FilledButton.tonal(
                        onPressed: isConnecting ? null : onConnectManual,
                        child: Text(tr('手动连接', 'Connect Manual')),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          )
        else
          SizedBox(
            height: 170,
            child: ListView.builder(
              itemCount: servers.length,
              itemBuilder: (context, index) {
                final s = servers[index];
                final selected = s.serverId == selectedServerId;
                final recent = isRecentHost(s.host);
                return ListTile(
                  dense: true,
                  selected: selected,
                  onTap: () => onServerSelected(s.serverId),
                  title: Row(
                    children: [
                      Expanded(
                          child: Text('${s.serverName} (${s.host})')),
                      if (recent)
                        Container(
                          padding: const EdgeInsets.symmetric(
                              horizontal: 8, vertical: 2),
                          decoration: BoxDecoration(
                            color: AudioConsoleColors.tealDim,
                            borderRadius: BorderRadius.circular(10),
                          ),
                          child: Text(
                            tr('最近连接', 'Recent'),
                            style: const TextStyle(
                              color: AudioConsoleColors.teal,
                              fontSize: 11,
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                        ),
                    ],
                  ),
                  subtitle: Text(
                    '${s.host}  ws:${s.wsPort}  ping:${s.latencyMs == null ? '-' : '${s.latencyMs}ms'}',
                  ),
                );
              },
            ),
          ),
        if (connectMode == 0 && servers.isNotEmpty)
          ExpansionTile(
            tilePadding: EdgeInsets.zero,
            title: Text(tr('高级选项', 'Advanced')),
            children: [
              TextField(
                controller: manualHostController,
                decoration: InputDecoration(
                  border: const OutlineInputBorder(),
                  labelText:
                      tr('手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
                  hintText: tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
                ),
              ),
            ],
          ),
        const SizedBox(height: 10),
        Row(
          children: [
            Expanded(
              child: FilledButton(
                onPressed: isConnecting
                    ? null
                    : () {
                        if (connectMode == 0) {
                          onConnectSelected();
                        } else if (connectMode == 2) {
                          onConnectUsb();
                        } else {
                          onConnectManual();
                        }
                      },
                child: Text(connectActionLabel),
              ),
            ),
            const SizedBox(width: 8),
            OutlinedButton(
              onPressed: (probeRunning || nsdDiscoveryRunning)
                  ? null
                  : onScanLan,
              child: Text(
                (probeRunning || nsdDiscoveryRunning)
                    ? tr('扫描中...', 'Scanning...')
                    : tr('扫描局域网', 'Scan LAN'),
              ),
            ),
          ],
        ),
        const SizedBox(height: 20),
        // ─── Section: 均衡器 (Equalizer) ───
        _sectionHeader(tr('均衡器', 'Equalizer')),
        const SizedBox(height: 8),
        Row(
          children: [
            Expanded(
              child: Text(
                tr('启用均衡器', 'Enable EQ'),
                style: const TextStyle(fontSize: 14),
              ),
            ),
            Switch(
              value: eqEnabled,
              onChanged: (value) => onSetEq(enabled: value),
            ),
          ],
        ),
        const SizedBox(height: 6),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            _eqPresetButton(tr('平直', 'Flat'), 'flat'),
            _eqPresetButton(tr('低音增强', 'Bass'), 'bass'),
            _eqPresetButton(tr('人声清晰', 'Vocal'), 'vocal'),
            _eqPresetButton(tr('高频亮丽', 'Bright'), 'bright'),
          ],
        ),
        const SizedBox(height: 6),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceEvenly,
          children: [
            _eqSlider(
              label: tr('低频\n60Hz', 'Low\n60Hz'),
              value: eqLowDb,
              onChanged: (value) => onSetEq(lowDb: value),
            ),
            _eqSlider(
              label: tr('中频\n1kHz', 'Mid\n1kHz'),
              value: eqMidDb,
              onChanged: (value) => onSetEq(midDb: value),
            ),
            _eqSlider(
              label: tr('高频\n10kHz', 'High\n10kHz'),
              value: eqHighDb,
              onChanged: (value) => onSetEq(highDb: value),
            ),
          ],
        ),
        const SizedBox(height: 20),
        // ─── Section: 麦克风 (Mic) ───
        _sectionHeader(tr('麦克风', 'Microphone')),
        const SizedBox(height: 8),
        MicStatusWidget(
          service: micService,
          host: serviceTargetHost,
          reversePort: reverseChannelPort,
          enabled: micEnabled,
          onToggle: onToggleMic,
        ),
        const SizedBox(height: 20),
        // ─── Section: 响度归一化 ───
        _sectionHeader(tr('响度归一化', 'Loudness Normalization')),
        const SizedBox(height: 8),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          dense: true,
          title: Text(
            tr('启用响度归一化', 'Enable loudness normalization'),
            style: const TextStyle(fontSize: 14),
          ),
          subtitle: Text(
            tr(
              '均衡/高质量模式生效，低延迟模式自动旁路',
              'Active in balanced/high_quality; bypassed in low_latency',
            ),
            style: const TextStyle(fontSize: 11),
          ),
          value: loudnessNormalizationEnabled,
          onChanged: onSetLoudnessNormalization,
        ),
        const SizedBox(height: 20),
        // ─── Section: 应用 (App) ───
        _sectionHeader(tr('应用', 'App')),
        const SizedBox(height: 8),
        ListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(tr('后台播放', 'Background Playback')),
          trailing: const Icon(Icons.chevron_right),
          onTap: onOpenPowerSavingGuide,
        ),
        ListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(tr('检查更新', 'Check Update')),
          trailing: updateCheckRunning
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.chevron_right),
          onTap: updateCheckRunning ? null : onCheckUpdate,
        ),
        ListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(tr('版本', 'Version')),
          trailing: const Text(
            '1.8.2',
            style: TextStyle(color: AudioConsoleColors.text2),
          ),
        ),
        const SizedBox(height: 20),
        // ─── Section: 调试指标 (Debug, collapsed) ───
        ExpansionTile(
          tilePadding: EdgeInsets.zero,
          title: Text(
            tr('调试指标', 'Debug Metrics'),
            style: const TextStyle(
              fontWeight: FontWeight.w700,
              fontSize: 16,
            ),
          ),
          children: [
            _debugText(
              '${tr('协议版本', 'Protocol')}: v${protocolVersion ?? 1}  ·  '
              '${tr('当前模式', 'Mode')}: $currentAudioModeLabel',
            ),
            _debugText(
              '${tr('协议路径', 'Protocol path')}: $protocolPath'
              '${experimentalPath ? ' (${tr('灰度', 'gray')})' : ''}',
            ),
            _debugText('Codec: $effectiveCodec'),
            if (serverPlatform != null || serverAppVersion != null)
              _debugText(
                '${tr('服务端', 'Server')}: '
                '${serverPlatform ?? 'unknown'}'
                '${serverAppVersion == null ? '' : ' ($serverAppVersion)'}',
              ),
            _debugText(
              '${tr('能力协商', 'Capabilities')}: '
              '${negotiatedCapabilities.entries.where((e) => e.value).map((e) => e.key).join(', ')}',
            ),
            const SizedBox(height: 6),
            _debugMetric(tr('采样率', 'Sample rate'), '$sampleRate'),
            _debugMetric(tr('声道', 'Channels'), '$channels'),
            _debugMetric(tr('总缓冲', 'Total buffered'), '$uiBufferedMs ms'),
            _debugMetric(tr('抖动', 'Jitter'), '$uiJitterBufferedMs ms'),
            _debugMetric(tr('轨道缓冲', 'Track buffered'), '$uiTrackQueuedMs ms'),
            _debugMetric(
              tr('AudioTrack 延迟', 'AudioTrack latency'),
              uiAudioTrackLatencyMs == null
                  ? '-'
                  : '$uiAudioTrackLatencyMs ms',
            ),
            _debugMetric(tr('欠载', 'Underrun'), '$uiUnderrun'),
            _debugMetric(tr('丢弃', 'Dropped'), '$uiDropped'),
            _debugMetric(tr('延迟帧', 'Late'), '$uiLate'),
            _debugMetric(tr('低水位保持', 'Floor holds'), '$uiFloorHoldCount'),
            _debugMetric(
              tr('P95 抖动', 'P95 jitter'),
              uiJitterP95Ms == null ? '-' : '$uiJitterP95Ms ms',
            ),
            _debugMetric(tr('播放后端', 'Playback backend'), playbackBackend),
            _debugMetric(tr('连接来源', 'Connection path'), connectionPathLabel),
            _debugMetric(
              tr('传输模式', 'Transport mode'),
              transportMode == 'usb' ? 'USB' : 'WiFi',
            ),
            _debugMetric(
                tr('当前设备数', 'Connected devices'), '$connectedClientCount'),
            _debugMetric(
              'TCP RTT',
              tcpRoundTripMs == null
                  ? '-'
                  : '$tcpRoundTripMs ms / $tcpRoundTripMedianMs ms (med)',
            ),
            _debugMetric(
              tr('策略', 'Strategy'),
              '${modeProfile['startBufferMs'] ?? '-'} / ${modeProfile['maxBufferMs'] ?? '-'} ms',
            ),
            _debugMetric(
              tr('响度增益', 'Loudness gain'),
              isPlaying
                  ? '${loudnessGainDb >= 0 ? '+' : ''}${loudnessGainDb.toStringAsFixed(1)} dB'
                  : '-',
            ),
            _debugMetric(tr('UDP 包数', 'UDP packets'), '$uiUdpPackets'),
            _debugMetric(tr('UDP 字节', 'UDP bytes'), '$uiUdpBytes'),
            _debugMetric(tr('丢包估计', 'Loss estimate'), '$uiUdpLoss'),
            _debugMetric(tr('最后序号', 'Last seq'), '${uiLastSeq ?? '-'}'),
            const SizedBox(height: 6),
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(6),
              color: Colors.black,
              child: Text(
                wsLog.isEmpty ? '(empty)' : wsLog,
                style: const TextStyle(
                    color: Colors.greenAccent, fontSize: 11),
                maxLines: 4,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
      ],
    );
  }

  Widget _sectionHeader(String title) {
    return Text(
      title,
      style: const TextStyle(
        fontWeight: FontWeight.w700,
        fontSize: 16,
        color: AudioConsoleColors.text,
      ),
    );
  }

  Widget _eqSlider({
    required String label,
    required int value,
    required ValueChanged<int> onChanged,
  }) {
    return SizedBox(
      width: 86,
      height: 190,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(label, textAlign: TextAlign.center),
          const SizedBox(height: 4),
          Text(
            '${value >= 0 ? '+' : ''}$value dB',
            style: const TextStyle(fontWeight: FontWeight.w700),
          ),
          Expanded(
            child: RotatedBox(
              quarterTurns: -1,
              child: Slider(
                min: -10,
                max: 10,
                divisions: 20,
                value: value.toDouble(),
                onChanged: (next) => onChanged(next.round()),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _eqPresetButton(String label, String preset) {
    return OutlinedButton(
      onPressed: () => onApplyEqPreset(preset),
      child: Text(label),
    );
  }

  Widget _debugText(String text) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Text(
        text,
        style: const TextStyle(fontSize: 12, color: AudioConsoleColors.text2),
      ),
    );
  }

  Widget _debugMetric(String label, String value) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Row(
        children: [
          Text(label,
              style: const TextStyle(
                  fontSize: 11, color: AudioConsoleColors.text3)),
          const Spacer(),
          Text(value,
              style: const TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                  color: AudioConsoleColors.text)),
        ],
      ),
    );
  }
}

/// Lightweight data class for server info passed to MorePage.
class MoreServerData {
  const MoreServerData({
    required this.serverId,
    required this.serverName,
    required this.host,
    required this.wsPort,
    required this.udpPort,
    required this.latencyMs,
  });

  final String serverId;
  final String serverName;
  final String host;
  final int wsPort;
  final int udpPort;
  final int? latencyMs;
}
