import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'audio/audio_track_output.dart';
import 'audio/background_playback_service.dart';
import 'audio/jitter_buffer.dart';
import 'audio/las_packet.dart';
import 'connect_history.dart';
import 'services/mic_capture_service.dart';
import 'ui/audio_console_status.dart';
import 'ui/audio_console_theme.dart';
import 'ui/pages/more_page.dart';
import 'ui/pages/play_page.dart';
import 'ui/pages/power_saving_guide_page.dart';
import 'ui/widgets/mode_selector_widget.dart';

const String kAppVersion = '1.8.3';
const String kUiBuildTag = 'UI build: audio-console-dark-v$kAppVersion';
const bool kUseBackgroundPlaybackService = true;

void main() {
  runApp(const LanAudioApp());
}

class LanAudioApp extends StatelessWidget {
  const LanAudioApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'LAN Audio Console',
      theme: buildAudioConsoleTheme(),
      home: const MainShell(),
      routes: {
        PowerSavingGuidePage.routeName: (_) => const PowerSavingGuidePage(),
      },
    );
  }
}

QrConnectionTarget? parseLanAudioUri(String? raw) {
  if (raw == null || raw.trim().isEmpty) return null;
  final uri = Uri.tryParse(raw.trim());
  if (uri == null || uri.scheme != 'lan-audio') return null;
  final host = uri.host;
  if (host.isEmpty) return null;
  final wsPort = uri.hasPort ? uri.port : 39991;
  final udpPort = int.tryParse(uri.queryParameters['udp'] ?? '') ?? 39992;
  return QrConnectionTarget(host: host, wsPort: wsPort, udpPort: udpPort);
}

FirewallGuidance? firewallGuidanceForMessage(String? raw) {
  if (raw == null || raw.trim().isEmpty) return null;
  final message = raw.toLowerCase();
  if (message.contains('connection refused') ||
      message.contains('econnrefused') ||
      message.contains('refused')) {
    return const FirewallGuidance(
      titleZh: 'PC 端服务未启动或端口被防火墙拦截',
      titleEn: 'PC service is not running or the port is blocked',
      body: 'Windows 防火墙放行步骤 / Windows Firewall steps:\n'
          '1. 打开 Windows Defender 防火墙 / Open Windows Defender Firewall\n'
          '2. 入站规则 -> 新建规则 / Inbound Rules -> New Rule\n'
          '3. 端口 39991，TCP+UDP，允许连接 / Allow TCP+UDP port 39991',
    );
  }
  if (message.contains('timeout') ||
      message.contains('timed out') ||
      message.contains('etimedout')) {
    return const FirewallGuidance(
      titleZh: '设备不在同一局域网，或 PC 防火墙未放行 UDP/TCP 39991',
      titleEn:
          'Device is not on the same LAN or Windows Firewall blocks UDP/TCP 39991',
      body: 'Windows 防火墙放行步骤 / Windows Firewall steps:\n'
          '1. 打开 Windows Defender 防火墙 / Open Windows Defender Firewall\n'
          '2. 入站规则 -> 新建规则 / Inbound Rules -> New Rule\n'
          '3. 端口 39991，TCP+UDP，允许连接 / Allow TCP+UDP port 39991',
    );
  }
  if (message.contains('auth') ||
      message.contains('version') ||
      message.contains('incompatible')) {
    return const FirewallGuidance(
      titleZh: '版本不兼容，请检查双端版本号',
      titleEn: 'Version mismatch. Check both app versions',
      body: '确认 Windows 与 Android 都是同一发布版本。\n'
          'Make sure Windows and Android are running the same LAN Audio release.',
    );
  }
  return null;
}

class FirewallGuidance {
  const FirewallGuidance({
    required this.titleZh,
    required this.titleEn,
    required this.body,
  });

  final String titleZh;
  final String titleEn;
  final String body;
}

class QrConnectionTarget {
  QrConnectionTarget({
    required this.host,
    required this.wsPort,
    required this.udpPort,
  });

  final String host;
  final int wsPort;
  final int udpPort;
}

class DiscoveryServer {
  DiscoveryServer({
    required this.serverId,
    required this.serverName,
    required this.host,
    required this.wsPort,
    required this.udpPort,
    required this.lastSeen,
    this.latencyMs,
    this.source = 'udp',
  });

  final String serverId;
  final String serverName;
  final String host;
  final int wsPort;
  final int udpPort;
  final DateTime lastSeen;
  final int? latencyMs;
  final String source;
}

enum PlaybackState {
  stopped,
  buffering,
  playing,
}

enum ConnectMode {
  discovered,
  manual,
  usb,
}

enum AudioModePreference {
  lowLatency,
  balanced,
  highQuality,
}

enum AppLang {
  zh,
  en,
}

class MainShell extends StatefulWidget {
  const MainShell({super.key});

  @override
  State<MainShell> createState() => _MainShellState();
}

/// Keep legacy name as alias for smoke tests that reference DebugPage.
typedef DebugPage = MainShell;

class _MainShellState extends State<MainShell> {
  static const MethodChannel _platformChannel =
      MethodChannel('lan_audio/platform');

  int _currentTabIndex = 0;

  final Map<String, DiscoveryServer> _servers = {};
  final JitterBuffer _jitter =
      JitterBuffer(startBufferMs: 60, maxBufferMs: 300);
  final AudioTrackOutput _audioOutput = AudioTrackOutput();
  final BackgroundPlaybackService _backgroundService =
      BackgroundPlaybackService();
  final MicCaptureService _micService = MicCaptureService();
  bool _micEnabled = false;
  final int _reverseChannelPort = 7878;

  int _androidVolumePct = 50;
  bool _showVolumePill = false;
  final TextEditingController _manualHostController = TextEditingController();

  RawDatagramSocket? _discoverySocket;
  RawDatagramSocket? _udpSocket;
  WebSocket? _ws;
  Timer? _pingTimer;
  Timer? _playTimer;
  Timer? _probeTimer;
  Timer? _nsdPollTimer;
  Timer? _discoveryTimeoutTimer;
  StreamSubscription<PlaybackServiceSnapshot>? _serviceEventsSub;

  String _status = 'idle';
  String _wsLog = '';
  String? _selectedServerId;
  ConnectMode _connectMode = ConnectMode.discovered;
  bool _isConnecting = false;
  bool _wsConnected = false;
  AppLang _lang = AppLang.en;
  bool _probeRunning = false;
  DateTime _lastProbeAt = DateTime.fromMillisecondsSinceEpoch(0);
  bool _firstUseHintShown = false;
  AudioModePreference _currentAudioMode = AudioModePreference.balanced;

  PlaybackState _playbackState = PlaybackState.stopped;

  int _sampleRate = 48000;
  int _channels = 2;

  int _udpPackets = 0;
  int _udpBytes = 0;
  int _udpLoss = 0;
  int? _lastSeq;
  final Map<String, DateTime> _recentConnectedHosts = {};
  String? _serviceTargetHost;
  String? _serviceTargetName;
  int _serviceBufferedMs = 0;
  int _serviceJitterBufferedMs = 0;
  int _serviceTrackQueuedMs = 0;
  int _serviceUnderrun = 0;
  int _serviceDropped = 0;
  int _serviceLate = 0;
  int _serviceFloorHoldCount = 0;
  int _serviceUdpPackets = 0;
  int _serviceUdpBytes = 0;
  int _serviceLoss = 0;
  int? _serviceLastSeq;
  int? _serviceAudioTrackLatencyMs;
  int? _protocolVersion;
  Map<String, bool> _negotiatedCapabilities = const {};
  String? _serverPlatform;
  String? _serverAppVersion;
  Map<String, dynamic> _modeProfile = const {};
  String _connectionPath = 'lan_ip_wifi_or_usb';
  String _transportMode = 'wifi';
  int _connectedClientCount = 0;
  int? _tcpRoundTripMs;
  int? _tcpRoundTripMedianMs;
  int? _serviceJitterP95Ms;
  String _protocolPath = 'legacy_or_v2_auto';
  String _playbackBackend = 'audiotrack_stable';
  String _effectiveCodec = 'pcm16';

  /// Phase 7 user-selected codec preference. `null` = "use server default
  /// for the current mode" (Opus on v2_header, Pcm16 elsewhere). Set
  /// explicitly via the codec picker. Persists in memory only — re-loads
  /// to null on app restart.
  String? _preferredCodec;
  bool _eqEnabled = false;
  int _eqLowDb = 0;
  int _eqMidDb = 0;
  int _eqHighDb = 0;
  bool _loudnessNormalizationEnabled = false;
  double _loudnessGainDb = 0.0;
  int _reconnectAttempts = 0;
  int _reconnectDelayMs = 0;
  bool _experimentalPath = false;
  bool _updateCheckRunning = false;
  bool _nsdDiscoveryRunning = false;
  bool _discoveryTimedOut = false;
  List<ConnectHistoryEntry> _connectHistory = const <ConnectHistoryEntry>[];

  @override
  void initState() {
    super.initState();
    final sysLang =
        PlatformDispatcher.instance.locale.languageCode.toLowerCase();
    _lang = sysLang.startsWith('zh') ? AppLang.zh : AppLang.en;
    debugPrint('ui_build $kUiBuildTag');
    _platformChannel.setMethodCallHandler(_handlePlatformCall);
    _acquireMulticastLock();
    _loadConnectHistory();
    _startDiscovery();
    if (kUseBackgroundPlaybackService) {
      debugPrint(
          'ui_init background playback service enabled; attach events/getSnapshot/setOptions');
      _serviceEventsSub =
          _backgroundService.events().listen(_onPlaybackServiceEvent);
      _backgroundService
          .getSnapshot()
          .then(_onPlaybackServiceEvent)
          .catchError((_) {});
      _backgroundService
          .setOptions(startBufferMs: 60, maxBufferMs: 300, pingIntervalMs: 1000)
          .catchError((_) {});
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _maybeShowFirstUseHint();
      _maybeOpenPowerGuideFromIntent();
    });
    _scheduleSilentUpdateCheck();
  }

  bool get _isZh => _lang == AppLang.zh;

  String tr(String zh, String en) => _isZh ? zh : en;

  Future<void> _maybeOpenPowerGuideFromIntent() async {
    try {
      final shouldOpen = await _platformChannel
              .invokeMethod<bool>('consumePowerGuideRequest') ??
          false;
      if (shouldOpen && mounted) {
        Navigator.of(context).pushNamed(PowerSavingGuidePage.routeName);
      }
    } catch (_) {
      // Platform support is optional on desktop/widget tests.
    }
  }

  void _openPowerSavingGuide() {
    Navigator.of(context).pushNamed(PowerSavingGuidePage.routeName);
  }

  AudioModePreference _audioModeFromWire(String value) {
    switch (value) {
      case 'low_latency':
        return AudioModePreference.lowLatency;
      case 'high_quality':
        return AudioModePreference.highQuality;
      default:
        return AudioModePreference.balanced;
    }
  }

  String _audioModeWire(AudioModePreference mode) {
    switch (mode) {
      case AudioModePreference.lowLatency:
        return 'low_latency';
      case AudioModePreference.highQuality:
        return 'high_quality';
      case AudioModePreference.balanced:
        return 'balanced';
    }
  }

  String _audioModeLabel(AudioModePreference mode) {
    switch (mode) {
      case AudioModePreference.lowLatency:
        return tr('低延迟', 'Low Latency');
      case AudioModePreference.highQuality:
        return tr('高音质', 'High Quality');
      case AudioModePreference.balanced:
        return tr('平衡', 'Balanced');
    }
  }

  void _scheduleSilentUpdateCheck() {
    _checkForUpdate(silentDelayMs: 5000, showNoUpdateHint: false);
  }

  Future<void> _checkForUpdate({
    required int silentDelayMs,
    required bool showNoUpdateHint,
  }) async {
    if (_updateCheckRunning) return;
    _updateCheckRunning = true;
    try {
      final update = await _platformChannel.invokeMapMethod<String, dynamic>(
        'checkForAppUpdate',
        {'delayMs': silentDelayMs},
      );
      if (!mounted) return;
      if (update == null) {
        if (showNoUpdateHint) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(tr('当前已是最新版本', 'Already up to date'))),
          );
        }
        return;
      }
      final version = (update['latestVersion'] as String?) ?? '';
      final releaseUrl = (update['releaseUrl'] as String?) ?? '';
      if (version.isEmpty || releaseUrl.isEmpty) {
        return;
      }
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            tr('发现新版本 v$version', 'New version v$version is available'),
          ),
          action: SnackBarAction(
            label: tr('前往下载', 'Open'),
            onPressed: () {
              _openExternalUrl(releaseUrl);
            },
          ),
        ),
      );
    } catch (_) {
      // silent ignore
    } finally {
      _updateCheckRunning = false;
    }
  }

  Future<void> _openExternalUrl(String url) async {
    try {
      await _platformChannel.invokeMethod('openExternalUrl', {'url': url});
    } catch (_) {
      // silent ignore
    }
  }

  String _connectionPathLabel(String path) {
    switch (path) {
      case 'lan_ip_wifi_or_usb':
        return tr('局域网 IP（Wi-Fi / USB）', 'LAN IP (Wi-Fi / USB)');
      case 'usb_tethering':
        return tr('USB 共享网络', 'USB tethering');
      case 'usb_localhost':
        return tr('USB（adb localhost）', 'USB (adb localhost)');
      case 'wifi':
        return tr('Wi-Fi', 'Wi-Fi');
      default:
        return path;
    }
  }

  String? _mostRecentHost() {
    if (_recentConnectedHosts.isEmpty) {
      return null;
    }
    final entries = _recentConnectedHosts.entries.toList()
      ..sort((a, b) => b.value.compareTo(a.value));
    return entries.first.key;
  }

  bool _isRecentHost(String host) => _recentConnectedHosts.containsKey(host);

  void _onPlaybackServiceEvent(PlaybackServiceSnapshot snapshot) {
    if (!mounted) {
      return;
    }
    final metrics = snapshot.metrics;
    final runtimeState = snapshot.state.toLowerCase();
    setState(() {
      _sampleRate = (metrics['sample_rate'] as num?)?.toInt() ?? _sampleRate;
      _channels = (metrics['channels'] as num?)?.toInt() ?? _channels;
      _serviceBufferedMs =
          (metrics['buffered_ms'] as num?)?.toInt() ?? _serviceBufferedMs;
      _serviceJitterBufferedMs =
          (metrics['jitter_buffered_ms'] as num?)?.toInt() ??
              _serviceJitterBufferedMs;
      _serviceTrackQueuedMs =
          (metrics['audio_track_queued_ms'] as num?)?.toInt() ??
              _serviceTrackQueuedMs;
      _serviceUnderrun =
          (metrics['underrun'] as num?)?.toInt() ?? _serviceUnderrun;
      _serviceDropped =
          (metrics['dropped_packets'] as num?)?.toInt() ?? _serviceDropped;
      _serviceLate = (metrics['late_packets'] as num?)?.toInt() ?? _serviceLate;
      _serviceFloorHoldCount = (metrics['floor_hold_count'] as num?)?.toInt() ??
          _serviceFloorHoldCount;
      _serviceUdpPackets =
          (metrics['udp_packets'] as num?)?.toInt() ?? _serviceUdpPackets;
      _serviceUdpBytes =
          (metrics['udp_bytes'] as num?)?.toInt() ?? _serviceUdpBytes;
      _serviceLoss =
          (metrics['loss_estimate'] as num?)?.toInt() ?? _serviceLoss;
      _serviceLastSeq =
          (metrics['last_seq'] as num?)?.toInt() ?? _serviceLastSeq;
      _serviceAudioTrackLatencyMs =
          (metrics['audio_track_latency_ms'] as num?)?.toInt() ??
              _serviceAudioTrackLatencyMs;
      _currentAudioMode = _audioModeFromWire(snapshot.mode);
      _protocolVersion = snapshot.protocolVersion ??
          (snapshot.dataPlane == 'v2_header' ? 2 : 1);
      _negotiatedCapabilities = snapshot.negotiatedCapabilities;
      _serverPlatform = snapshot.serverPlatform;
      _serverAppVersion = snapshot.serverAppVersion;
      _modeProfile = snapshot.modeProfile;
      _connectionPath =
          snapshot.transport == 'usb' ? 'usb_localhost' : 'lan_ip_wifi_or_usb';
      _transportMode = snapshot.transportMode;
      _connectedClientCount = snapshot.connectedClientCount;
      _protocolPath = snapshot.dataPlane;
      _playbackBackend = snapshot.playbackBackend;
      _effectiveCodec = snapshot.effectiveCodec;
      _eqEnabled = snapshot.eqEnabled;
      _eqLowDb = (snapshot.eqSettings['low_db'] as num?)?.toInt() ??
          (snapshot.eqSettings['lowDb'] as num?)?.toInt() ??
          _eqLowDb;
      _eqMidDb = (snapshot.eqSettings['mid_db'] as num?)?.toInt() ??
          (snapshot.eqSettings['midDb'] as num?)?.toInt() ??
          _eqMidDb;
      _eqHighDb = (snapshot.eqSettings['high_db'] as num?)?.toInt() ??
          (snapshot.eqSettings['highDb'] as num?)?.toInt() ??
          _eqHighDb;
      _loudnessNormalizationEnabled = snapshot.loudnessNormalizationEnabled;
      _loudnessGainDb =
          (metrics['loudness_gain_db'] as num?)?.toDouble() ?? _loudnessGainDb;
      _reconnectAttempts = snapshot.reconnectAttempts;
      _reconnectDelayMs = snapshot.reconnectDelayMs;
      _experimentalPath = snapshot.dataPlane == 'v2_header';
      _tcpRoundTripMs = (metrics['rtt_ms'] as num?)?.toInt();
      _tcpRoundTripMedianMs = _tcpRoundTripMs;
      _serviceJitterP95Ms =
          (metrics['jitter_p95_ms'] as num?)?.toInt() ?? _serviceJitterP95Ms;

      if (runtimeState == 'streaming') {
        _playbackState = PlaybackState.playing;
      } else if (runtimeState == 'handshaking' ||
          runtimeState == 'negotiated' ||
          runtimeState == 'reconfiguring' ||
          runtimeState == 'recovering') {
        _playbackState = PlaybackState.buffering;
      } else {
        _playbackState = PlaybackState.stopped;
      }

      _wsConnected = runtimeState != 'disconnected' && runtimeState != 'closed';

      _status = runtimeState == 'recovering'
          ? '${tr('重新连接中', 'Reconnecting')}... (${tr('第', '#')} $_reconnectAttempts${tr('次', '')}, ${_reconnectDelayMs}ms)'
          : '${snapshot.state}/${snapshot.rollbackState}';
      _wsLog = jsonEncode(snapshot.toMap());
    });
  }

  /// Dedupe across discovery sources (mDNS / UDP beacon / probe scan).
  /// We key by `host:wsPort` because every server is uniquely identified
  /// by its IP and websocket port, and the same server will surface on
  /// more than one source. Higher-priority sources (mDNS UUID > UDP UUID
  /// > probe-fallback) overwrite lower-priority ones; same-priority
  /// updates refresh the entry in place. Returns true if anything
  /// actually changed so the caller can decide whether to setState.
  bool _upsertDiscoveryServer(DiscoveryServer incoming) {
    final dedupeKey = '${incoming.host}:${incoming.wsPort}';
    DiscoveryServer? existing;
    String? existingMapKey;
    for (final entry in _servers.entries) {
      if ('${entry.value.host}:${entry.value.wsPort}' == dedupeKey) {
        existing = entry.value;
        existingMapKey = entry.key;
        break;
      }
    }
    int rank(String source) {
      switch (source) {
        case 'mdns':
          return 3;
        case 'udp':
          return 2;
        case 'probe':
          return 1;
        default:
          return 0;
      }
    }

    if (existing == null) {
      _servers[incoming.serverId] = incoming;
      return true;
    }
    final incomingRank = rank(incoming.source);
    final existingRank = rank(existing.source);
    if (incomingRank >= existingRank) {
      // Replace under the new (higher or equal) source's key.
      if (existingMapKey != null && existingMapKey != incoming.serverId) {
        _servers.remove(existingMapKey);
      }
      _servers[incoming.serverId] = incoming;
      return true;
    }
    // Lower-priority update — only refresh `lastSeen` so the entry stays
    // visible while the higher-priority source is alive.
    if (existingMapKey != null) {
      _servers[existingMapKey] = DiscoveryServer(
        serverId: existing.serverId,
        serverName: existing.serverName,
        host: existing.host,
        wsPort: existing.wsPort,
        udpPort: existing.udpPort,
        lastSeen: incoming.lastSeen,
        latencyMs: existing.latencyMs ?? incoming.latencyMs,
        source: existing.source,
      );
    }
    return false;
  }

  void _maybeSelectRecentOrFirst() {
    if (_servers.isEmpty) {
      return;
    }
    final current = _selectedServerId;
    if (current != null && _servers.containsKey(current)) {
      return;
    }
    final recentHost = _mostRecentHost();
    if (recentHost != null) {
      for (final server in _servers.values) {
        if (server.host == recentHost) {
          _selectedServerId = server.serverId;
          return;
        }
      }
    }
    _selectedServerId = _servers.keys.first;
  }

  Future<void> _maybeShowFirstUseHint() async {
    if (!mounted || _firstUseHintShown) {
      return;
    }
    final consumed = await _getFirstUseHintConsumed();
    if (!mounted || consumed) {
      return;
    }
    _firstUseHintShown = true;
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(tr('首次使用提示', 'First-time Setup Tips')),
        content: Text(
          tr(
            '1. 确保电脑端服务已启动。\n2. 手机与电脑连接同一 Wi-Fi。\n3. 点击“扫描局域网”或手动输入 IP。',
            '1. Ensure desktop server is running.\n2. Phone and desktop are on the same Wi-Fi.\n3. Tap "Scan LAN" or enter IP manually.',
          ),
        ),
        actions: [
          FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(tr('知道了', 'Got it')),
          ),
        ],
      ),
    );
    await _setFirstUseHintConsumed();
  }

  Future<void> _acquireMulticastLock() async {
    try {
      await _platformChannel.invokeMethod('acquireMulticastLock');
    } catch (_) {}
  }

  Future<void> _releaseMulticastLock() async {
    try {
      await _platformChannel.invokeMethod('releaseMulticastLock');
    } catch (_) {}
  }

  Future<void> _startNsdDiscovery() async {
    _nsdPollTimer?.cancel();
    _discoveryTimeoutTimer?.cancel();
    setState(() {
      _nsdDiscoveryRunning = true;
      _discoveryTimedOut = false;
      _status = tr('正在发现附近设备...', 'Discovering nearby devices...');
    });
    try {
      await _platformChannel.invokeMethod('startNsdDiscovery');
      _nsdPollTimer = Timer.periodic(
        const Duration(seconds: 1),
        (_) => _pollNsdServices(),
      );
      _discoveryTimeoutTimer = Timer(const Duration(seconds: 10), () {
        if (!mounted || _servers.isNotEmpty || _wsConnected) {
          return;
        }
        setState(() {
          _nsdDiscoveryRunning = false;
          _discoveryTimedOut = true;
          _status = tr('未发现设备，请手动输入', 'No devices found. Enter host manually.');
        });
      });
      await _pollNsdServices();
    } catch (_) {
      if (mounted) {
        setState(() {
          _nsdDiscoveryRunning = false;
          _discoveryTimedOut = true;
        });
      }
    }
  }

  Future<void> _pollNsdServices() async {
    try {
      final services = await _platformChannel
              .invokeMethod<List<dynamic>>('getNsdDiscoveredServices') ??
          const <dynamic>[];
      if (services.isEmpty) {
        return;
      }
      var changed = false;
      for (final raw in services) {
        if (raw is! Map) {
          continue;
        }
        final parsed = await _parseNsdService(raw);
        if (parsed == null) {
          continue;
        }
        if (_upsertDiscoveryServer(parsed)) {
          changed = true;
        }
      }
      if (changed && mounted) {
        setState(() {
          _nsdDiscoveryRunning = false;
          _discoveryTimedOut = false;
          _status = tr('已发现附近设备', 'Nearby device discovered');
          _maybeSelectRecentOrFirst();
        });
      }
    } catch (_) {}
  }

  Future<DiscoveryServer?> _parseNsdService(Map raw) async {
    final host = (raw['host'] as String?)?.trim() ?? '';
    final serverId = (raw['serverId'] as String?)?.trim();
    final serverName = (raw['serverName'] as String?)?.trim();
    final wsPort = (raw['wsPort'] as num?)?.toInt() ?? 39991;
    final udpPort = (raw['udpPort'] as num?)?.toInt() ?? 39992;
    if (host.isEmpty || serverId == null || serverId.isEmpty) {
      return null;
    }
    final latency = await _measureTcpLatencyMs(host, wsPort);
    return DiscoveryServer(
      serverId: serverId,
      serverName: serverName?.isNotEmpty == true
          ? serverName!
          : tr('附近设备', 'Nearby Device'),
      host: host,
      wsPort: wsPort,
      udpPort: udpPort,
      lastSeen: DateTime.now(),
      latencyMs: latency,
      source: 'mdns',
    );
  }

  Future<int?> _measureTcpLatencyMs(String host, int port) async {
    final start = DateTime.now();
    Socket? socket;
    try {
      socket = await Socket.connect(
        host,
        port,
        timeout: const Duration(milliseconds: 500),
      );
      return DateTime.now().difference(start).inMilliseconds;
    } catch (_) {
      return null;
    } finally {
      await socket?.close();
    }
  }

  Future<bool> _getFirstUseHintConsumed() async {
    try {
      return await _platformChannel
              .invokeMethod<bool>('getFirstUseHintConsumed') ??
          false;
    } catch (_) {
      return false;
    }
  }

  Future<void> _setFirstUseHintConsumed() async {
    try {
      await _platformChannel.invokeMethod('setFirstUseHintConsumed', {
        'consumed': true,
      });
    } catch (_) {}
  }

  Future<void> _loadConnectHistory() async {
    try {
      final raw =
          await _platformChannel.invokeMethod<String>('getConnectHistory') ??
              '';
      if (!mounted) {
        return;
      }
      setState(() {
        _connectHistory = ConnectHistoryStore.decode(raw);
      });
    } catch (_) {}
  }

  Future<void> _persistConnectHistory() async {
    try {
      await _platformChannel.invokeMethod('setConnectHistory', {
        'json': ConnectHistoryStore.encode(_connectHistory),
      });
    } catch (_) {}
  }

  Future<void> _recordConnectHistory({
    required String host,
    required int wsPort,
    required String serverName,
  }) async {
    final known = _servers.values.where((s) => s.host == host).toList();
    final latency = known.isEmpty ? 0 : (known.first.latencyMs ?? 0);
    setState(() {
      _connectHistory = ConnectHistoryStore.upsert(
        _connectHistory,
        ip: host,
        port: wsPort,
        hostname: serverName,
        connectedAt: DateTime.now(),
        latencyMs: latency,
      );
    });
    await _persistConnectHistory();
  }

  @override
  void dispose() {
    if (kUseBackgroundPlaybackService) {
      debugPrint(
          'ui_dispose detach only; background playback service is not stopped by UI dispose');
    } else {
      debugPrint('ui_dispose legacy path; stopping UI-owned playback');
    }
    if (!kUseBackgroundPlaybackService) {
      _stopPlayback();
    }
    _serviceEventsSub?.cancel();
    _probeTimer?.cancel();
    _nsdPollTimer?.cancel();
    _discoveryTimeoutTimer?.cancel();
    _pingTimer?.cancel();
    _ws?.close();
    _udpSocket?.close();
    _discoverySocket?.close();
    _platformChannel.invokeMethod('stopNsdDiscovery').catchError((_) {});
    _releaseMulticastLock();
    _manualHostController.dispose();
    super.dispose();
  }

  Future<void> _startDiscovery() async {
    await _startNsdDiscovery();
    try {
      final socket =
          await RawDatagramSocket.bind(InternetAddress.anyIPv4, 39990);
      _discoverySocket = socket;
      socket.listen((event) {
        if (event != RawSocketEvent.read) {
          return;
        }
        final datagram = socket.receive();
        if (datagram == null) {
          return;
        }
        final parsed = _parseDiscovery(datagram);
        if (parsed == null) {
          return;
        }
        setState(() {
          if (_upsertDiscoveryServer(parsed)) {
            _status = tr('正在监听设备发现', 'discovery listening');
          }
          _maybeSelectRecentOrFirst();
        });
      });
      _startProbeLoop();
    } catch (e) {
      setState(() {
        _status = '${tr('发现异常', 'discovery error')}: $e';
      });
    }
  }

  void _startProbeLoop() {
    _probeTimer?.cancel();
    _probeTimer = Timer.periodic(const Duration(seconds: 8), (_) async {
      if (!mounted) {
        return;
      }
      final shouldProbe = _connectMode == ConnectMode.discovered &&
          !_wsConnected &&
          _servers.isEmpty;
      if (!shouldProbe) {
        return;
      }
      await _probeSubnetForServers();
    });
    _probeSubnetForServers();
  }

  Future<void> _probeSubnetForServers() async {
    if (_probeRunning) {
      return;
    }
    final now = DateTime.now();
    if (now.difference(_lastProbeAt).inSeconds < 4) {
      return;
    }
    _lastProbeAt = now;
    _probeRunning = true;
    try {
      final interfaces = await NetworkInterface.list(
        type: InternetAddressType.IPv4,
        includeLoopback: false,
      );
      final addresses = <InternetAddress>[];
      for (final itf in interfaces) {
        addresses.addAll(
            itf.addresses.where((a) => a.type == InternetAddressType.IPv4));
      }
      final local = addresses.where((a) {
        final ip = a.address;
        return ip.startsWith('192.168.') ||
            ip.startsWith('10.') ||
            ip.startsWith('172.');
      }).toList();
      if (local.isEmpty) {
        return;
      }

      final parts = local.first.address.split('.');
      if (parts.length != 4) {
        return;
      }
      final prefix = '${parts[0]}.${parts[1]}.${parts[2]}.';
      final selfHost = int.tryParse(parts[3]) ?? -1;

      if (mounted) {
        setState(() {
          _status = tr('正在扫描局域网...', 'Scanning LAN...');
        });
      }

      const wsPort = 39991;
      const udpPort = 39992;
      final pending = <Future<void>>[];

      Future<void> probeHost(int host) async {
        if (host == selfHost) {
          return;
        }
        final ip = '$prefix$host';
        Socket? socket;
        final started = DateTime.now();
        try {
          socket = await Socket.connect(ip, wsPort,
              timeout: const Duration(milliseconds: 160));
          if (!mounted) {
            return;
          }
          final serverId = 'probe-$ip';
          setState(() {
            _upsertDiscoveryServer(DiscoveryServer(
              serverId: serverId,
              serverName: tr('扫描发现', 'Scanned Server'),
              host: ip,
              wsPort: wsPort,
              udpPort: udpPort,
              lastSeen: DateTime.now(),
              latencyMs: DateTime.now().difference(started).inMilliseconds,
              source: 'probe',
            ));
            _status = tr('已通过扫描发现服务器', 'server discovered via probe');
            _maybeSelectRecentOrFirst();
          });
        } catch (_) {
          // ignore connect timeout/refused
        } finally {
          await socket?.close();
        }
      }

      for (var i = 1; i <= 254; i++) {
        pending.add(probeHost(i));
        if (pending.length >= 32) {
          await Future.wait(pending);
          pending.clear();
        }
      }
      if (pending.isNotEmpty) {
        await Future.wait(pending);
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _status = '${tr('局域网扫描失败', 'LAN probe failed')}: $e';
        });
      }
    } finally {
      _probeRunning = false;
    }
  }

  DiscoveryServer? _parseDiscovery(Datagram datagram) {
    try {
      final jsonObj =
          jsonDecode(utf8.decode(datagram.data)) as Map<String, dynamic>;
      if (jsonObj['type'] != 'lan_audio_discovery_v1') {
        return null;
      }
      return DiscoveryServer(
        serverId: jsonObj['server_id'] as String,
        serverName: jsonObj['server_name'] as String,
        host: datagram.address.address,
        wsPort: jsonObj['ws_port'] as int,
        udpPort: jsonObj['udp_port'] as int,
        lastSeen: DateTime.now(),
        source: 'udp',
      );
    } catch (_) {
      return null;
    }
  }

  Future<void> _connectSelected() async {
    final id = _selectedServerId;
    if (id == null || !_servers.containsKey(id)) {
      setState(() {
        _status = tr(
          '未选择服务器（点击发现设备或使用手动地址）',
          'no server selected (tap a discovered server or use manual host)',
        );
      });
      await _probeSubnetForServers();
      return;
    }

    final server = _servers[id]!;
    await _connectToHost(
      host: server.host,
      wsPort: server.wsPort,
      udpPort: server.udpPort,
      serverName: server.serverName,
    );
  }

  Future<void> _connectManual() async {
    final host = _manualHostController.text.trim();
    if (host.isEmpty) {
      setState(() {
        _status = tr('手动地址不能为空', 'manual host is empty');
      });
      return;
    }
    await _connectToHost(
      host: host,
      wsPort: 39991,
      udpPort: 39992,
      serverName: 'manual:$host',
    );
  }

  Future<void> _connectUsb() async {
    await _connectToHost(
      host: '127.0.0.1',
      wsPort: 39991,
      udpPort: 39992,
      serverName: 'usb:localhost',
      transportMode: 'usb',
    );
  }

  Future<void> _connectToHost({
    required String host,
    required int wsPort,
    required int udpPort,
    required String serverName,
    String transportMode = 'wifi',
  }) async {
    setState(() {
      _isConnecting = true;
      _status = '${tr('连接中', 'connecting')}: $serverName ($host)';
    });
    if (kUseBackgroundPlaybackService) {
      try {
        debugPrint(
            'ui_startPlayback forwarding to service host=$host ws=$wsPort udp=$udpPort server=$serverName');
        await _backgroundService.startPlayback(
          host: host,
          wsPort: wsPort,
          udpPort: udpPort,
          serverName: serverName,
          transportMode: transportMode,
        );
        setState(() {
          _recentConnectedHosts[host] = DateTime.now();
          _serviceTargetHost = host;
          _serviceTargetName = serverName;
          _status =
              '${tr('后台服务已启动', 'background service started')}: $serverName ($host)';
        });
        await _recordConnectHistory(
          host: host,
          wsPort: wsPort,
          serverName: serverName,
        );
      } catch (e) {
        setState(() {
          _status = '${tr('后台服务启动失败', 'service start failed')}: $e';
        });
      } finally {
        if (mounted) {
          setState(() {
            _isConnecting = false;
          });
        }
      }
      return;
    }
    try {
      await _ws?.close();
      _pingTimer?.cancel();
      await _stopPlayback();

      _udpSocket?.close();
      _udpSocket = await RawDatagramSocket.bind(InternetAddress.anyIPv4, 0);
      final localUdpPort = _udpSocket!.port;

      _udpSocket!.listen((event) {
        if (event != RawSocketEvent.read) {
          return;
        }
        final datagram = _udpSocket!.receive();
        if (datagram == null) {
          return;
        }
        _handleUdpPacket(datagram.data);
      });

      final ws = await WebSocket.connect('ws://$host:$wsPort/');
      _ws = ws;

      final hello = {
        'type': 'hello',
        'protocol_version': 2,
        'device_name': 'flutter-android',
        'client_id': 'flutter-${Platform.localHostname}',
        'udp_port': localUdpPort,
        'desired_sample_rate': 48000,
        'channels': 2,
        'preferred_audio_mode': _audioModeWire(_currentAudioMode),
        'capabilities': {
          'supports_pcm16': true,
          'supports_f32': false,
          'supports_modes': true,
          'supports_metrics': true,
          'supports_opus_future': false,
          'supports_opus': false,
          'supports_opus_experimental': false,
          'supports_low_latency': true,
          'supports_high_quality': true,
          'supports_native_audio_track': true,
        },
      };
      ws.add(jsonEncode(hello));
      ws.add(jsonEncode({
        'type': 'client_info',
        'client_name': 'flutter-android',
        'platform': Platform.operatingSystem,
        'app_version': kUiBuildTag,
        'udp_port': localUdpPort,
      }));

      ws.listen((data) {
        try {
          final text = '$data';
          final decoded = jsonDecode(text);
          if (decoded is Map<String, dynamic>) {
            final type = decoded['type']?.toString();
            if (type == 'hello_ack') {
              _currentAudioMode = _audioModeFromWire(
                decoded['current_audio_mode']?.toString() ?? 'balanced',
              );
            } else if (type == 'audio_mode_changed') {
              _currentAudioMode = _audioModeFromWire(
                decoded['mode']?.toString() ?? 'balanced',
              );
            }
          }
          setState(() {
            _wsLog = text;
          });
        } catch (_) {
          setState(() {
            _wsLog = '$data';
          });
        }
      }, onError: (Object e) {
        setState(() {
          _wsConnected = false;
          _status = 'WS ${tr('错误', 'error')}: $e';
        });
      }, onDone: () {
        setState(() {
          _wsConnected = false;
          _status = tr('WS 已关闭', 'ws closed');
        });
      });

      int pingSeq = 0;
      _pingTimer = Timer.periodic(const Duration(seconds: 1), (_) {
        _ws?.add(jsonEncode({
          'type': 'client_ping',
          'seq': pingSeq++,
          'ts_unix_ms': DateTime.now().millisecondsSinceEpoch,
        }));
      });

      setState(() {
        _wsConnected = true;
        _status =
            '${tr('已连接', 'connected')}: $serverName ($host ws:$wsPort udp:$udpPort)';
        _recentConnectedHosts[host] = DateTime.now();
        _udpPackets = 0;
        _udpBytes = 0;
        _udpLoss = 0;
        _lastSeq = null;
      });
      await _recordConnectHistory(
        host: host,
        wsPort: wsPort,
        serverName: serverName,
      );
    } catch (e) {
      setState(() {
        _wsConnected = false;
        _status = '${tr('连接失败', 'connect failed')}: $e';
      });
    } finally {
      if (mounted) {
        setState(() {
          _isConnecting = false;
        });
      }
    }
  }

  Future<void> _setAudioMode(AudioModePreference mode) async {
    final modeWire = _audioModeWire(mode);
    try {
      if (kUseBackgroundPlaybackService) {
        await _backgroundService.setAudioMode(
          mode: modeWire,
          reason: 'ui_select',
          preferredCodec: _preferredCodec,
        );
      } else {
        _ws?.add(jsonEncode({
          'type': 'set_audio_mode',
          'mode': modeWire,
          'reason': 'ui_select',
          if (_preferredCodec != null) 'preferred_codec': _preferredCodec,
        }));
      }
      if (!mounted) {
        return;
      }
      setState(() {
        _currentAudioMode = mode;
      });
    } catch (e) {
      if (!mounted) {
        return;
      }
      setState(() {
        _status = '${tr('模式切换失败', 'Audio mode change failed')}: $e';
      });
    }
  }

  /// Phase 7 codec picker. Sends a `set_audio_mode` with the new codec
  /// preference. The server resolves the actual codec (downgrading if
  /// the data plane can't carry the request) and reflects it back via
  /// `audio_mode_changed.effective_codec`.
  Future<void> _setPreferredCodec(String? codec) async {
    setState(() {
      _preferredCodec = codec;
    });
    final modeWire = _audioModeWire(_currentAudioMode);
    try {
      if (kUseBackgroundPlaybackService) {
        await _backgroundService.setAudioMode(
          mode: modeWire,
          reason: 'codec_change',
          preferredCodec: codec,
        );
      } else {
        _ws?.add(jsonEncode({
          'type': 'set_audio_mode',
          'mode': modeWire,
          'reason': 'codec_change',
          if (codec != null) 'preferred_codec': codec,
        }));
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _status = '${tr('编码器切换失败', 'Codec change failed')}: $e';
      });
    }
  }

  Future<void> _setEq({
    bool? enabled,
    int? lowDb,
    int? midDb,
    int? highDb,
  }) async {
    final nextEnabled = enabled ?? _eqEnabled;
    final nextLow = (lowDb ?? _eqLowDb).clamp(-10, 10).toInt();
    final nextMid = (midDb ?? _eqMidDb).clamp(-10, 10).toInt();
    final nextHigh = (highDb ?? _eqHighDb).clamp(-10, 10).toInt();
    setState(() {
      _eqEnabled = nextEnabled;
      _eqLowDb = nextLow;
      _eqMidDb = nextMid;
      _eqHighDb = nextHigh;
    });
    try {
      await _backgroundService.setEqSettings(
        enabled: nextEnabled,
        lowDb: nextLow,
        midDb: nextMid,
        highDb: nextHigh,
      );
    } catch (e) {
      if (!mounted) {
        return;
      }
      setState(() {
        _status = '${tr('均衡器设置失败', 'EQ update failed')}: $e';
      });
    }
  }

  Future<void> _applyEqPreset(String preset) async {
    switch (preset) {
      case 'bass':
        await _setEq(enabled: true, lowDb: 6, midDb: 0, highDb: 0);
        return;
      case 'vocal':
        await _setEq(enabled: true, lowDb: -2, midDb: 4, highDb: 1);
        return;
      case 'bright':
        await _setEq(enabled: true, lowDb: 0, midDb: 0, highDb: 5);
        return;
      default:
        await _setEq(enabled: false, lowDb: 0, midDb: 0, highDb: 0);
    }
  }

  Future<void> _setLoudnessNormalization(bool enabled) async {
    setState(() {
      _loudnessNormalizationEnabled = enabled;
      if (!enabled) {
        _loudnessGainDb = 0.0;
      }
    });
    try {
      await _backgroundService.setLoudnessNormalization(enabled);
    } catch (e) {
      if (!mounted) {
        return;
      }
      setState(() {
        _status =
            '${tr('响度归一化设置失败', 'Loudness normalization update failed')}: $e';
      });
    }
  }

  void _handleUdpPacket(Uint8List bytes) {
    final packet = LasPacket.parse(bytes);
    if (packet == null) {
      return;
    }

    if (packet.hasConfigChanged) {
      // TODO(protocol-v2): when LAS2/LAV2 header is enabled, refresh playback config here.
    }
    if (packet.hasDiscontinuity) {
      // TODO(protocol-v2): reset jitter/decoder state on discontinuity.
    }

    _udpPackets += 1;
    _udpBytes += bytes.length;
    if (_lastSeq != null) {
      final expected = (_lastSeq! + 1) & 0xFFFFFFFF;
      if (packet.sequence != expected) {
        _udpLoss += (packet.sequence - expected) & 0xFFFFFFFF;
      }
    }
    _lastSeq = packet.sequence;

    _sampleRate = packet.sampleRate;
    _channels = packet.channels;

    _jitter.push(packet);

    if (_playbackState != PlaybackState.stopped) {
      final newState = _jitter.bufferedMs >= 40
          ? PlaybackState.playing
          : PlaybackState.buffering;
      if (newState != _playbackState) {
        setState(() {
          _playbackState = newState;
        });
      } else {
        setState(() {});
      }
    } else {
      setState(() {});
    }
  }

  Future<void> _stopPlayback() async {
    // stop mic if active
    if (_micEnabled) {
      try {
        await _micService.stop();
      } catch (_) {}
      _micEnabled = false;
    }
    if (kUseBackgroundPlaybackService) {
      try {
        debugPrint('ui_stopPlayback forwarding to background service');
        await _backgroundService.stopPlayback();
      } catch (_) {}
      if (mounted) {
        setState(() {
          _playbackState = PlaybackState.stopped;
          _wsConnected = false;
          _status = tr('后台播放已停止', 'background playback stopped');
        });
      }
      return;
    }
    _playTimer?.cancel();
    _playTimer = null;
    try {
      await _audioOutput.stop();
    } catch (_) {}
    try {
      await _audioOutput.release();
    } catch (_) {}
    if (mounted) {
      setState(() {
        _playbackState = PlaybackState.stopped;
      });
    }
  }

  Future<dynamic> _handlePlatformCall(MethodCall call) async {
    switch (call.method) {
      case 'volumeChanged':
        final args = call.arguments as Map<dynamic, dynamic>?;
        final pct = args == null ? 50 : (args['volume_pct'] as int?) ?? 50;
        if (mounted) {
          setState(() {
            _androidVolumePct = pct;
            _showVolumePill = true;
          });
          Future.delayed(const Duration(seconds: 2), () {
            if (mounted) {
              setState(() => _showVolumePill = false);
            }
          });
        }
        break;
    }
  }

  Future<void> _toggleMicCapture() async {
    if (_micEnabled) {
      await _micService.stop();
      _micEnabled = false;
      setState(() {});
      return;
    }
    final rationale = await _micService.getRationaleString();
    if (!mounted) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(tr('麦克风权限', 'Microphone Permission')),
        content: Text(rationale),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: Text(tr('取消', 'Cancel')),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: Text(tr('允许', 'Allow')),
          ),
        ],
      ),
    );
    if (ok != true) return;
    final granted = await _micService.requestPermission();
    if (!granted) {
      if (!mounted) return;
      showDialog(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text(tr('权限被拒', 'Permission Denied')),
          content: Text(tr(
            '需要麦克风权限才能使用此功能。请在系统设置中授予权限。',
            'Microphone permission is required. Please grant it in system settings.',
          )),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: Text(tr('确定', 'OK')),
            ),
          ],
        ),
      );
      return;
    }
    final host = _serviceTargetHost;
    if (host == null || host.isEmpty) {
      _micService.status = MicStatus.error;
      _micService.errorMessage = 'No server connected';
      setState(() {});
      return;
    }
    _micEnabled = true;
    setState(() {});
    try {
      await _micService.start(host: host, port: _reverseChannelPort);
    } catch (_) {
      _micEnabled = false;
      _micService.status = MicStatus.error;
      _micService.errorMessage = 'Failed to start mic capture';
    }
    setState(() {});
  }

  String _playbackLabel() {
    switch (_playbackState) {
      case PlaybackState.playing:
        return tr('播放中', 'playing');
      case PlaybackState.buffering:
        return tr('缓冲中', 'buffering');
      case PlaybackState.stopped:
        return tr('已停止', 'stopped');
    }
  }

  ConsoleUiState get _consoleState => ConsoleStatusMapper.map(
        isConnecting: _isConnecting,
        wsConnected: _wsConnected,
        isPlaying: _playbackState == PlaybackState.playing,
        isBuffering: _playbackState == PlaybackState.buffering,
        runtimeState: _playbackLabel(),
        hasError: _status.toLowerCase().contains('fail') ||
            _status.toLowerCase().contains('error'),
      );

  bool get _modeSelectorEnabled =>
      _wsConnected && _consoleState != ConsoleUiState.connecting;

  String get _metricBufferText {
    if (_consoleState != ConsoleUiState.streaming &&
        _consoleState != ConsoleUiState.buffering) {
      return '--';
    }
    return '$_uiBufferedMs';
  }

  String get _metricUnderrunText => '$_uiUnderrun';

  String _modeId(AudioModePreference mode) {
    switch (mode) {
      case AudioModePreference.lowLatency:
        return 'low_latency';
      case AudioModePreference.balanced:
        return 'balanced';
      case AudioModePreference.highQuality:
        return 'high_quality';
    }
  }

  AudioModePreference _modeFromId(String id) {
    switch (id) {
      case 'low_latency':
        return AudioModePreference.lowLatency;
      case 'high_quality':
        return AudioModePreference.highQuality;
      case 'balanced':
      default:
        return AudioModePreference.balanced;
    }
  }

  String _statusChipLabel() {
    if (_isConnecting) return tr('连接中', 'CONNECTING');
    if (_wsConnected &&
        _playbackState == PlaybackState.buffering &&
        _reconnectAttempts > 0) {
      return tr('重连中', 'RECONNECTING');
    }
    if (_wsConnected && _playbackState == PlaybackState.playing)
      return tr('推流中', 'STREAMING');
    if (_wsConnected && _playbackState == PlaybackState.buffering)
      return tr('缓冲中', 'BUFFERING');
    if (_wsConnected) return tr('已连接', 'CONNECTED');
    return tr('未连接', 'DISCONNECTED');
  }

  String _connectActionLabel() {
    if (_isConnecting) return tr('连接中...', 'Connecting...');
    if (_connectMode == ConnectMode.usb) return tr('USB 连接', 'USB Connect');
    if (_connectMode == ConnectMode.manual) return tr('手动连接', 'Connect Manual');
    if (_selectedServerId != null) {
      final selected = _servers[_selectedServerId!];
      if (selected != null && _isRecentHost(selected.host)) {
        return tr('快速连接', 'Quick Connect');
      }
    }
    return tr('连接所选', 'Connect Selected');
  }

  int get _uiBufferedMs =>
      kUseBackgroundPlaybackService ? _serviceBufferedMs : _jitter.bufferedMs;

  int get _uiJitterBufferedMs => kUseBackgroundPlaybackService
      ? _serviceJitterBufferedMs
      : _jitter.bufferedMs;

  int get _uiTrackQueuedMs =>
      kUseBackgroundPlaybackService ? _serviceTrackQueuedMs : 0;

  int get _uiUnderrun => kUseBackgroundPlaybackService
      ? _serviceUnderrun
      : _jitter.stats.underrunCount;

  int get _uiDropped => kUseBackgroundPlaybackService
      ? _serviceDropped
      : _jitter.stats.droppedFrames;

  int get _uiLate =>
      kUseBackgroundPlaybackService ? _serviceLate : _jitter.stats.lateFrames;

  int get _uiFloorHoldCount =>
      kUseBackgroundPlaybackService ? _serviceFloorHoldCount : 0;

  int? get _uiJitterP95Ms =>
      kUseBackgroundPlaybackService ? _serviceJitterP95Ms : null;

  int get _uiUdpPackets =>
      kUseBackgroundPlaybackService ? _serviceUdpPackets : _udpPackets;

  int get _uiUdpBytes =>
      kUseBackgroundPlaybackService ? _serviceUdpBytes : _udpBytes;

  int get _uiUdpLoss => kUseBackgroundPlaybackService ? _serviceLoss : _udpLoss;

  int? get _uiLastSeq =>
      kUseBackgroundPlaybackService ? _serviceLastSeq : _lastSeq;

  int? get _uiAudioTrackLatencyMs =>
      kUseBackgroundPlaybackService ? _serviceAudioTrackLatencyMs : null;

  void _showAdvancedSheet() {
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (context) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 20),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(tr('快捷操作', 'Quick Actions'),
                  style: AudioConsoleType.title()),
              const SizedBox(height: 16),
              OutlinedButton(
                onPressed: _updateCheckRunning
                    ? null
                    : () {
                        Navigator.of(context).pop();
                        _checkForUpdate(
                          silentDelayMs: 0,
                          showNoUpdateHint: true,
                        );
                      },
                child: Text(tr('检查更新', 'Check Update')),
              ),
              const SizedBox(height: 8),
              OutlinedButton(
                onPressed: () {
                  Navigator.of(context).pop();
                  _openPowerSavingGuide();
                },
                child: Text(tr('后台播放', 'Background playback')),
              ),
              const SizedBox(height: 8),
              Text(
                tr('调试信息请查看「更多」页底部', 'Debug info is in the "More" tab'),
                style: TextStyle(
                  fontSize: 12,
                  color: AudioConsoleColors.text2,
                ),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final servers = _servers.values.toList()
      ..sort((a, b) {
        final aRecent = _recentConnectedHosts[a.host];
        final bRecent = _recentConnectedHosts[b.host];
        if (aRecent != null || bRecent != null) {
          if (aRecent == null) return 1;
          if (bRecent == null) return -1;
          return bRecent.compareTo(aRecent);
        }
        // Stable secondary order: by host (IP) so the list does not
        // visually reshuffle every time a beacon refreshes lastSeen.
        return a.host.compareTo(b.host);
      });

    _maybeSelectRecentOrFirst();
    final modeItems = <ModeSelectorItem>[
      ModeSelectorItem(
        id: 'low_latency',
        name: tr('低延迟', 'Low Latency'),
        desc: tr('游戏/视频', 'Games/Video'),
      ),
      ModeSelectorItem(
        id: 'balanced',
        name: tr('均衡', 'Balanced'),
        desc: tr('日常使用', 'Daily'),
      ),
      ModeSelectorItem(
        id: 'high_quality',
        name: tr('高质量', 'High Quality'),
        desc: tr('音乐欣赏', 'Music'),
      ),
    ];

    return Scaffold(
      appBar: AppBar(
        title: Text(tr('LAN Audio 控制台', 'LAN Audio Console')),
        actions: [
          IconButton(
            key: const Key('advanced_debug_entry'),
            tooltip: tr('高级与调试', 'Advanced & Debug'),
            onPressed: () => _showAdvancedSheet(),
            icon: const Icon(Icons.tune),
          ),
          PopupMenuButton<AppLang>(
            initialValue: _lang,
            onSelected: (lang) => setState(() => _lang = lang),
            itemBuilder: (context) => const [
              PopupMenuItem(value: AppLang.zh, child: Text('中文')),
              PopupMenuItem(value: AppLang.en, child: Text('English')),
            ],
            icon: const Icon(Icons.language),
          ),
        ],
      ),
      body: Stack(
        children: [
          IndexedStack(
            index: _currentTabIndex,
            children: [
              _buildPlayPage(modeItems),
              _buildMorePage(servers),
            ],
          ),
          // Volume pill overlay
          Positioned(
            top: 8,
            left: 0,
            right: 0,
            child: IgnorePointer(
              ignoring: !_showVolumePill,
              child: AnimatedOpacity(
                opacity: _showVolumePill ? 1.0 : 0.0,
                duration: const Duration(milliseconds: 300),
                child: Center(
                  child: Container(
                    padding:
                        const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                    decoration: BoxDecoration(
                      color: const Color(0xFF1A2035),
                      borderRadius: BorderRadius.circular(20),
                    ),
                    child: Text(
                      '${_isZh ? '音量' : 'Vol'}: $_androidVolumePct%',
                      style: const TextStyle(
                        color: Color(0xFF00D4AA),
                        fontFamily: 'monospace',
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
      bottomNavigationBar: BottomNavigationBar(
        currentIndex: _currentTabIndex,
        onTap: (index) => setState(() => _currentTabIndex = index),
        backgroundColor: AudioConsoleColors.surface,
        selectedItemColor: AudioConsoleColors.teal,
        unselectedItemColor: AudioConsoleColors.text2,
        type: BottomNavigationBarType.fixed,
        items: [
          BottomNavigationBarItem(
            icon: const Icon(Icons.play_circle_outline),
            label: _isZh ? '播放' : 'Play',
          ),
          BottomNavigationBarItem(
            icon: const Icon(Icons.more_horiz),
            label: _isZh ? '更多' : 'More',
          ),
        ],
      ),
    );
  }

  Widget _buildPlayPage(List<ModeSelectorItem> modeItems) {
    return PlayPage(
      isZh: _isZh,
      consoleState: _consoleState,
      statusChipLabel: _statusChipLabel(),
      statusText: _status,
      isConnecting: _isConnecting,
      wsConnected: _wsConnected,
      playbackStopped: _playbackState == PlaybackState.stopped,
      metricBufferText: _metricBufferText,
      metricUnderrunText: _metricUnderrunText,
      tcpRoundTripMs: _tcpRoundTripMs,
      modeItems: modeItems,
      currentModeId: _modeId(_currentAudioMode),
      modeSelectorEnabled: _modeSelectorEnabled,
      onModeSelected: (id) => _setAudioMode(_modeFromId(id)),
      onStopPlayback: _stopPlayback,
      onRetryConnection: _connectSelected,
      serverName: _serviceTargetName,
      currentLatencyMs: () =>
          _wsConnected ? _serviceBufferedMs.toDouble() : null,
      baselineLatencyMs: () => _baselineLatencyForMode(_currentAudioMode),
      effectiveCodec: _effectiveCodec,
      sampleRate: _sampleRate,
      channels: _channels,
    );
  }

  // Pre-optimization baseline latency for the active mode. These reflect the
  // historical (v1.7-and-earlier) buffer targets before the Kalman/PID and
  // soft-limiter pipeline landed; they form the static reference curve in the
  // latency comparison chart.
  double? _baselineLatencyForMode(AudioModePreference mode) {
    if (!_wsConnected) {
      return null;
    }
    switch (mode) {
      case AudioModePreference.lowLatency:
        return 110.0;
      case AudioModePreference.balanced:
        return 180.0;
      case AudioModePreference.highQuality:
        return 320.0;
    }
  }

  Widget _buildMorePage(List<DiscoveryServer> servers) {
    return MorePage(
      isZh: _isZh,
      connectMode: _connectMode == ConnectMode.discovered
          ? 0
          : _connectMode == ConnectMode.manual
              ? 1
              : 2,
      onConnectModeChanged: (mode) {
        setState(() {
          _connectMode = mode == 0
              ? ConnectMode.discovered
              : mode == 1
                  ? ConnectMode.manual
                  : ConnectMode.usb;
        });
      },
      isConnecting: _isConnecting,
      probeRunning: _probeRunning,
      nsdDiscoveryRunning: _nsdDiscoveryRunning,
      discoveryTimedOut: _discoveryTimedOut,
      manualHostController: _manualHostController,
      servers: servers
          .map((s) => MoreServerData(
                serverId: s.serverId,
                serverName: s.serverName,
                host: s.host,
                wsPort: s.wsPort,
                udpPort: s.udpPort,
                latencyMs: s.latencyMs,
              ))
          .toList(),
      selectedServerId: _selectedServerId,
      onServerSelected: (id) {
        setState(() {
          _selectedServerId = id;
        });
      },
      isRecentHost: _isRecentHost,
      onConnectSelected: _connectSelected,
      onConnectManual: _connectManual,
      onConnectUsb: _connectUsb,
      onScanLan: () {
        _startNsdDiscovery();
        _probeSubnetForServers();
      },
      connectActionLabel: _connectActionLabel(),
      wsConnected: _wsConnected,
      connectionStatusText: _wsConnected
          ? '${tr('已连接', 'Connected')} ${_serviceTargetHost ?? ''}'
          : tr('未连接', 'Disconnected'),
      preferredCodec: _preferredCodec,
      onPreferredCodecChanged: _setPreferredCodec,
      eqEnabled: _eqEnabled,
      eqLowDb: _eqLowDb,
      eqMidDb: _eqMidDb,
      eqHighDb: _eqHighDb,
      onSetEq: _setEq,
      onApplyEqPreset: _applyEqPreset,
      micService: _micService,
      micEnabled: _micEnabled,
      serviceTargetHost: _serviceTargetHost,
      reverseChannelPort: _reverseChannelPort,
      onToggleMic: _toggleMicCapture,
      loudnessNormalizationEnabled: _loudnessNormalizationEnabled,
      onSetLoudnessNormalization: _setLoudnessNormalization,
      onOpenPowerSavingGuide: _openPowerSavingGuide,
      onCheckUpdate: () => _checkForUpdate(
        silentDelayMs: 0,
        showNoUpdateHint: true,
      ),
      updateCheckRunning: _updateCheckRunning,
      protocolVersion: _protocolVersion,
      currentAudioModeLabel: _audioModeLabel(_currentAudioMode),
      protocolPath: _protocolPath,
      experimentalPath: _experimentalPath,
      effectiveCodec: _effectiveCodec,
      serverPlatform: _serverPlatform,
      serverAppVersion: _serverAppVersion,
      negotiatedCapabilities: _negotiatedCapabilities,
      sampleRate: _sampleRate,
      channels: _channels,
      uiBufferedMs: _uiBufferedMs,
      uiJitterBufferedMs: _uiJitterBufferedMs,
      uiTrackQueuedMs: _uiTrackQueuedMs,
      uiAudioTrackLatencyMs: _uiAudioTrackLatencyMs,
      uiUnderrun: _uiUnderrun,
      uiDropped: _uiDropped,
      uiLate: _uiLate,
      uiFloorHoldCount: _uiFloorHoldCount,
      uiJitterP95Ms: _uiJitterP95Ms,
      playbackBackend: _playbackBackend,
      connectionPathLabel: _connectionPathLabel(_connectionPath),
      transportMode: _transportMode,
      connectedClientCount: _connectedClientCount,
      tcpRoundTripMs: _tcpRoundTripMs,
      tcpRoundTripMedianMs: _tcpRoundTripMedianMs,
      modeProfile: _modeProfile,
      loudnessGainDb: _loudnessGainDb,
      isPlaying: _playbackState == PlaybackState.playing,
      uiUdpPackets: _uiUdpPackets,
      uiUdpBytes: _uiUdpBytes,
      uiUdpLoss: _uiUdpLoss,
      uiLastSeq: _uiLastSeq,
      wsLog: _wsLog,
    );
  }
}

