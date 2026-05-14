import 'package:flutter/material.dart';

class SettingsPage extends StatelessWidget {
  const SettingsPage({
    super.key,
    required this.isZh,
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
    required this.onOpenPowerSavingGuide,
    required this.onCheckUpdate,
    required this.updateCheckRunning,
    // Debug metrics
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
  final int connectMode; // 0=discovered, 1=manual, 2=usb
  final ValueChanged<int> onConnectModeChanged;
  final bool isConnecting;
  final bool probeRunning;
  final bool nsdDiscoveryRunning;
  final bool discoveryTimedOut;
  final TextEditingController manualHostController;
  final List<SettingsServerData> servers;
  final String? selectedServerId;
  final ValueChanged<String> onServerSelected;
  final bool Function(String host) isRecentHost;
  final VoidCallback onConnectSelected;
  final VoidCallback onConnectManual;
  final VoidCallback onConnectUsb;
  final VoidCallback onScanLan;
  final String connectActionLabel;
  final bool wsConnected;
  final VoidCallback onOpenPowerSavingGuide;
  final VoidCallback onCheckUpdate;
  final bool updateCheckRunning;
  // Debug metrics
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
        // Connection card
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(tr('连接', 'Connection'),
                    style: const TextStyle(
                        fontWeight: FontWeight.w700, fontSize: 16)),
                const SizedBox(height: 8),
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
                if (connectMode == 1) // manual
                  TextField(
                    controller: manualHostController,
                    decoration: InputDecoration(
                      border: const OutlineInputBorder(),
                      labelText:
                          tr('手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
                      hintText: tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
                    ),
                  )
                else if (servers.isEmpty)
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      border: Border.all(color: Colors.black12),
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
                                color: Colors.black54,
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
                          style: const TextStyle(color: Colors.black54),
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
                                hintText: tr(
                                    '例如 192.168.1.23', 'e.g. 192.168.1.23'),
                              ),
                            ),
                            const SizedBox(height: 8),
                            Align(
                              alignment: Alignment.centerLeft,
                              child: FilledButton.tonal(
                                onPressed:
                                    isConnecting ? null : onConnectManual,
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
                                  child:
                                      Text('${s.serverName} (${s.host})')),
                              if (recent)
                                Container(
                                  padding: const EdgeInsets.symmetric(
                                      horizontal: 8, vertical: 2),
                                  decoration: BoxDecoration(
                                    color:
                                        Colors.teal.withValues(alpha: 0.16),
                                    borderRadius: BorderRadius.circular(10),
                                  ),
                                  child: Text(
                                    tr('最近连接', 'Recent'),
                                    style: const TextStyle(
                                      color: Colors.teal,
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
                          labelText: tr(
                              '手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
                          hintText:
                              tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
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
              ],
            ),
          ),
        ),
        const SizedBox(height: 10),
        // Connection help
        Card(
          child: ExpansionTile(
            title: Text(tr('连接帮助', 'Connection Help')),
            subtitle: Text(tr(
              '发现失败或延迟偏高时先检查这里',
              'Check this when discovery fails or latency is high',
            )),
            childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
            children: [
              Text(
                tr(
                  '确保手机和电脑在同一网络；访客网络、AP 隔离或客户端隔离会阻止发现。',
                  'Keep phone and PC on the same network; guest Wi-Fi, AP isolation, or client isolation can block discovery.',
                ),
              ),
              const SizedBox(height: 6),
              Text(
                tr(
                  '如果自动发现失败，请使用"扫描局域网"或手动输入 Windows 端地址。',
                  'If discovery fails, use Scan LAN or enter the Windows address manually.',
                ),
              ),
              const SizedBox(height: 6),
              Text(
                tr(
                  '追求低延迟时优先尝试 USB tethering 或 5GHz Wi-Fi；高音质模式会更稳但延迟更高。',
                  'For lower latency, prefer USB tethering or 5GHz Wi-Fi; High Quality is smoother but may add latency.',
                ),
              ),
              const SizedBox(height: 6),
              Text(
                tr(
                  '若后台后无声或断流，请关闭 Android 电池优化或保持 App 前台播放。',
                  'If audio stops in background, disable battery optimization or keep the app in foreground.',
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 10),
        // App settings card
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        tr('设置', 'Settings'),
                        style: const TextStyle(
                          fontWeight: FontWeight.w700,
                          fontSize: 16,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        tr('应用更新', 'App update'),
                        style: const TextStyle(color: Colors.black54),
                      ),
                    ],
                  ),
                ),
                OutlinedButton(
                  onPressed: onOpenPowerSavingGuide,
                  child: Text(tr('后台播放', 'Background')),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: updateCheckRunning ? null : onCheckUpdate,
                  child: Text(tr('检查更新', 'Check Update')),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 10),
        // Debug metrics
        Card(
          child: ExpansionTile(
            title: Text(tr('调试指标', 'Debug Metrics')),
            childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
            children: [
              Text(
                '${tr('协议版本', 'Protocol')}: v${protocolVersion ?? 1}  ·  '
                '${tr('当前模式', 'Mode')}: $currentAudioModeLabel',
                style: const TextStyle(fontSize: 12),
              ),
              const SizedBox(height: 4),
              Text(
                '${tr('协议路径', 'Protocol path')}: $protocolPath'
                '${experimentalPath ? ' (${tr('灰度', 'gray')})' : ''}',
                style: const TextStyle(fontSize: 12, color: Colors.black54),
              ),
              const SizedBox(height: 4),
              Text(
                'Codec: $effectiveCodec',
                style: const TextStyle(fontSize: 12, color: Colors.black54),
              ),
              const SizedBox(height: 4),
              if (serverPlatform != null || serverAppVersion != null)
                Text(
                  '${tr('服务端', 'Server')}: '
                  '${serverPlatform ?? 'unknown'}'
                  '${serverAppVersion == null ? '' : ' ($serverAppVersion)'}',
                  style: const TextStyle(fontSize: 12),
                ),
              if (serverPlatform != null || serverAppVersion != null)
                const SizedBox(height: 4),
              Text(
                '${tr('能力协商', 'Capabilities')}: '
                '${negotiatedCapabilities.entries.where((e) => e.value).map((e) => e.key).join(', ')}',
                style: const TextStyle(fontSize: 12, color: Colors.black54),
              ),
              const SizedBox(height: 6),
              _metricTile(tr('采样率', 'Sample rate'), '$sampleRate'),
              const SizedBox(height: 4),
              _metricTile(tr('声道', 'Channels'), '$channels'),
              const SizedBox(height: 4),
              _metricTile(tr('总缓冲', 'Total buffered'), '$uiBufferedMs ms'),
              const SizedBox(height: 4),
              _metricTile(tr('抖动', 'Jitter'), '$uiJitterBufferedMs ms'),
              const SizedBox(height: 4),
              _metricTile(tr('轨道缓冲', 'Track buffered'), '$uiTrackQueuedMs ms'),
              const SizedBox(height: 4),
              _metricTile(
                tr('AudioTrack 延迟', 'AudioTrack latency'),
                uiAudioTrackLatencyMs == null
                    ? '-'
                    : '$uiAudioTrackLatencyMs ms',
              ),
              const SizedBox(height: 4),
              _metricTile(tr('欠载', 'Underrun'), '$uiUnderrun'),
              const SizedBox(height: 4),
              _metricTile(tr('丢弃', 'Dropped'), '$uiDropped'),
              const SizedBox(height: 4),
              _metricTile(tr('延迟帧', 'Late'), '$uiLate'),
              const SizedBox(height: 4),
              _metricTile(tr('低水位保持', 'Floor holds'), '$uiFloorHoldCount'),
              const SizedBox(height: 4),
              _metricTile(
                tr('P95 抖动', 'P95 jitter'),
                uiJitterP95Ms == null ? '-' : '$uiJitterP95Ms ms',
              ),
              const SizedBox(height: 4),
              _metricTile(
                tr('播放后端', 'Playback backend'),
                playbackBackend,
              ),
              const SizedBox(height: 4),
              _metricTile(
                tr('连接来源', 'Connection path'),
                connectionPathLabel,
              ),
              const SizedBox(height: 4),
              _metricTile(
                tr('传输模式', 'Transport mode'),
                transportMode == 'usb' ? 'USB' : 'WiFi',
              ),
              const SizedBox(height: 4),
              _metricTile(
                tr('当前设备数', 'Connected devices'),
                '$connectedClientCount',
              ),
              const SizedBox(height: 4),
              _metricTile(
                'TCP RTT',
                tcpRoundTripMs == null
                    ? '-'
                    : '$tcpRoundTripMs ms / $tcpRoundTripMedianMs ms (med)',
              ),
              const SizedBox(height: 4),
              _metricTile(
                tr('策略', 'Strategy'),
                '${modeProfile['startBufferMs'] ?? '-'} / ${modeProfile['maxBufferMs'] ?? '-'} ms',
              ),
              const SizedBox(height: 4),
              _metricTile(
                tr('响度增益', 'Loudness gain'),
                isPlaying
                    ? '${loudnessGainDb >= 0 ? '+' : ''}${loudnessGainDb.toStringAsFixed(1)} dB'
                    : '-',
              ),
              const SizedBox(height: 4),
              _metricTile(tr('UDP 包数', 'UDP packets'), '$uiUdpPackets'),
              const SizedBox(height: 4),
              _metricTile(tr('UDP 字节', 'UDP bytes'), '$uiUdpBytes'),
              const SizedBox(height: 4),
              _metricTile(tr('丢包估计', 'Loss estimate'), '$uiUdpLoss'),
              const SizedBox(height: 4),
              _metricTile(tr('最后序号', 'Last seq'), '${uiLastSeq ?? '-'}'),
              const SizedBox(height: 6),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(6),
                color: Colors.black,
                child: Text(
                  wsLog.isEmpty ? '(empty)' : wsLog,
                  style:
                      const TextStyle(color: Colors.greenAccent, fontSize: 11),
                  maxLines: 4,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _metricTile(String label, String value) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        border: Border.all(color: Colors.black12),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label,
              style: const TextStyle(fontSize: 11, color: Colors.black54)),
          const SizedBox(height: 2),
          Text(value, style: const TextStyle(fontWeight: FontWeight.w700)),
        ],
      ),
    );
  }
}

/// Lightweight data class for server info passed to SettingsPage.
class SettingsServerData {
  const SettingsServerData({
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
