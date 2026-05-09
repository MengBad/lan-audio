import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'audio/background_playback_service.dart';
import 'ui/audio_console_status.dart';
import 'ui/audio_console_theme.dart';
import 'ui/metric_display_sampler.dart';
import 'ui/widgets/danger_action_button.dart';
import 'ui/widgets/hero_status_widget.dart';
import 'ui/widgets/metric_chip_widget.dart';
import 'ui/widgets/mode_selector_widget.dart';
import 'ui/widgets/server_card_widget.dart';

const String kUiBuildTag = 'UI build: audio-console-dark-v1';
const bool kUseBackgroundPlaybackService = true;

void main() {
  runApp(const LanAudioApp());
}

class LanAudioApp extends StatelessWidget {
  const LanAudioApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'LAN Audio Android MVP',
      theme: buildAudioConsoleTheme(),
      home: const DebugPage(),
    );
  }
}

class DiscoveryServer {
  DiscoveryServer({
    required this.serverId,
    required this.serverName,
    required this.host,
    required this.wsPort,
    required this.udpPort,
    required this.lastSeen,
  });

  final String serverId;
  final String serverName;
  final String host;
  final int wsPort;
  final int udpPort;
  final DateTime lastSeen;
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

class DebugPage extends StatefulWidget {
  const DebugPage({super.key});

  @override
  State<DebugPage> createState() => _DebugPageState();
}

class _DebugPageState extends State<DebugPage> {
  static const MethodChannel _platformChannel =
      MethodChannel('lan_audio/platform');

  final BackgroundPlaybackService _backgroundService =
      BackgroundPlaybackService();
  final MetricDisplaySampler _metricDisplaySampler =
      const MetricDisplaySampler();
  final Map<String, DiscoveryServer> _servers = {};
  final Map<String, DateTime> _recentConnectedHosts = {};
  final TextEditingController _manualHostController = TextEditingController();

  RawDatagramSocket? _discoverySocket;
  Timer? _probeTimer;
  Timer? _metricSamplerTimer;
  StreamSubscription<PlaybackServiceSnapshot>? _serviceEventsSub;

  String _status = 'idle';
  String _wsLog = '';
  String _audioLog = '';
  String _runtimeState = 'disconnected';
  String? _lastErrorMessage;
  String? _selectedServerId;
  String? _serviceTargetHost;
  String? _serviceTargetName;

  bool _isConnecting = false;
  bool _wsConnected = false;
  bool _probeRunning = false;
  bool _firstUseHintShown = false;
  bool _updateCheckRunning = false;

  AppLang _lang = AppLang.en;
  ConnectMode _connectMode = ConnectMode.discovered;
  AudioModePreference _currentAudioMode = AudioModePreference.balanced;
  PlaybackState _playbackState = PlaybackState.stopped;

  DateTime _lastProbeAt = DateTime.fromMillisecondsSinceEpoch(0);
  DateTime _lastBufferMetricPublishedAt =
      DateTime.fromMillisecondsSinceEpoch(0);

  int _sampleRate = 48000;
  int _channels = 2;
  int _serviceBufferedMs = 0;
  int _displayBufferedMs = 0;
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
  int _connectedClientCount = 0;
  int? _tcpRoundTripMs;
  int? _serviceJitterP95Ms;
  double? _serviceRxFramesPerSec;

  Map<String, bool> _negotiatedCapabilities = const {};
  Map<String, dynamic> _modeProfile = const {};
  String? _serverPlatform;
  String? _serverAppVersion;
  String _transportMode = 'wifi';
  String _playbackBackend = 'audiotrack_stable';
  String _effectiveCodec = 'pcm16';
  String _protocolPath = 'legacy_or_v2_auto';

  @override
  void initState() {
    super.initState();
    final sysLang =
        PlatformDispatcher.instance.locale.languageCode.toLowerCase();
    _lang = sysLang.startsWith('zh') ? AppLang.zh : AppLang.en;
    _acquireMulticastLock();
    _startDiscovery();

    if (kUseBackgroundPlaybackService) {
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
    _startMetricSampler();

    WidgetsBinding.instance.addPostFrameCallback((_) {
      _maybeShowFirstUseHint();
    });
    _scheduleSilentUpdateCheck();
  }

  @override
  void dispose() {
    _serviceEventsSub?.cancel();
    _metricSamplerTimer?.cancel();
    _probeTimer?.cancel();
    _discoverySocket?.close();
    _releaseMulticastLock();
    _manualHostController.dispose();
    super.dispose();
  }

  bool get _isZh => _lang == AppLang.zh;

  String tr(String zh, String en) => _isZh ? zh : en;

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

  void _onPlaybackServiceEvent(PlaybackServiceSnapshot snapshot) {
    if (!mounted) return;
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
      _serviceJitterP95Ms =
          (metrics['jitter_p95_ms'] as num?)?.toInt() ?? _serviceJitterP95Ms;
      _serviceRxFramesPerSec =
          (metrics['rx_frames_per_sec'] as num?)?.toDouble() ??
              _serviceRxFramesPerSec;
      _tcpRoundTripMs = (metrics['rtt_ms'] as num?)?.toInt();

      _runtimeState = runtimeState;
      _currentAudioMode = _audioModeFromWire(snapshot.mode);
      _protocolVersion = snapshot.protocolVersion ??
          (snapshot.dataPlane == 'v2_header' ? 2 : 1);
      _negotiatedCapabilities = snapshot.negotiatedCapabilities;
      _modeProfile = snapshot.modeProfile;
      _serverPlatform = snapshot.serverPlatform;
      _serverAppVersion = snapshot.serverAppVersion;
      _transportMode = snapshot.transportMode;
      _playbackBackend = snapshot.playbackBackend;
      _effectiveCodec = snapshot.effectiveCodec;
      _protocolPath = snapshot.dataPlane;
      _connectedClientCount = snapshot.connectedClientCount;

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
      _status = '${snapshot.state}/${snapshot.rollbackState}';
      _audioLog = snapshot.state;
      _wsLog = jsonEncode(snapshot.toMap());

      if (snapshot.lastError != null) {
        _lastErrorMessage = snapshot.lastError;
      } else if (_wsConnected || _playbackState != PlaybackState.stopped) {
        _lastErrorMessage = null;
      }
    });
  }

  void _startMetricSampler() {
    _metricSamplerTimer?.cancel();
    _metricSamplerTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (!mounted) return;
      final now = DateTime.now();
      if (!_metricDisplaySampler.canPublish(
        now: now,
        lastPublishedAt: _lastBufferMetricPublishedAt,
        runtimeState: _runtimeState,
      )) {
        return;
      }
      if (_displayBufferedMs == _serviceBufferedMs) return;
      setState(() {
        _displayBufferedMs = _serviceBufferedMs;
        _lastBufferMetricPublishedAt = now;
      });
    });
  }

  String? _mostRecentHost() {
    if (_recentConnectedHosts.isEmpty) return null;
    final entries = _recentConnectedHosts.entries.toList()
      ..sort((a, b) => b.value.compareTo(a.value));
    return entries.first.key;
  }

  bool _isRecentHost(String host) => _recentConnectedHosts.containsKey(host);

  Future<void> _startDiscovery() async {
    try {
      final socket =
          await RawDatagramSocket.bind(InternetAddress.anyIPv4, 39990);
      _discoverySocket = socket;
      socket.listen((event) {
        if (event != RawSocketEvent.read) return;
        final datagram = socket.receive();
        if (datagram == null) return;
        final parsed = _parseDiscovery(datagram);
        if (parsed == null) return;
        setState(() {
          _servers[parsed.serverId] = parsed;
          _status = tr('发现服务监听中', 'discovery listening');
          _maybeSelectRecentOrFirst();
        });
      });
      _startProbeLoop();
    } catch (e) {
      setState(() {
        _lastErrorMessage = '$e';
        _status = '${tr('发现异常', 'discovery error')}: $e';
      });
    }
  }

  void _startProbeLoop() {
    _probeTimer?.cancel();
    _probeTimer = Timer.periodic(const Duration(seconds: 8), (_) async {
      if (!mounted) return;
      final shouldProbe = _connectMode == ConnectMode.discovered &&
          !_wsConnected &&
          _servers.isEmpty;
      if (shouldProbe) await _probeSubnetForServers();
    });
    _probeSubnetForServers();
  }

  Future<void> _probeSubnetForServers() async {
    if (_probeRunning) return;
    final now = DateTime.now();
    if (now.difference(_lastProbeAt).inSeconds < 4) return;
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
      if (local.isEmpty) return;

      final parts = local.first.address.split('.');
      if (parts.length != 4) return;
      final prefix = '${parts[0]}.${parts[1]}.${parts[2]}.';
      final selfHost = int.tryParse(parts[3]) ?? -1;
      const wsPort = 39991;
      const udpPort = 39992;
      final pending = <Future<void>>[];

      Future<void> probeHost(int host) async {
        if (host == selfHost) return;
        final ip = '$prefix$host';
        Socket? socket;
        try {
          socket = await Socket.connect(
            ip,
            wsPort,
            timeout: const Duration(milliseconds: 160),
          );
          if (!mounted) return;
          final serverId = 'probe-$ip';
          setState(() {
            _servers[serverId] = DiscoveryServer(
              serverId: serverId,
              serverName: tr('扫描发现', 'Scanned Server'),
              host: ip,
              wsPort: wsPort,
              udpPort: udpPort,
              lastSeen: DateTime.now(),
            );
            _status = tr('扫描发现服务', 'server discovered via probe');
            _maybeSelectRecentOrFirst();
          });
        } catch (_) {
          // ignore timeout/refused
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
      if (pending.isNotEmpty) await Future.wait(pending);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _status = '${tr('局域网扫描失败', 'LAN probe failed')}: $e';
      });
    } finally {
      _probeRunning = false;
    }
  }

  DiscoveryServer? _parseDiscovery(Datagram datagram) {
    try {
      final jsonObj =
          jsonDecode(utf8.decode(datagram.data)) as Map<String, dynamic>;
      if (jsonObj['type'] != 'lan_audio_discovery_v1') return null;
      return DiscoveryServer(
        serverId: jsonObj['server_id'] as String,
        serverName: jsonObj['server_name'] as String,
        host: datagram.address.address,
        wsPort: jsonObj['ws_port'] as int,
        udpPort: jsonObj['udp_port'] as int,
        lastSeen: DateTime.now(),
      );
    } catch (_) {
      return null;
    }
  }

  void _maybeSelectRecentOrFirst() {
    if (_servers.isEmpty) return;
    final current = _selectedServerId;
    if (current != null && _servers.containsKey(current)) return;
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

  Future<void> _connectSelected() async {
    final id = _selectedServerId;
    if (id == null || !_servers.containsKey(id)) {
      setState(() {
        _lastErrorMessage = tr(
          '未选择服务端',
          'No server selected',
        );
        _status = _lastErrorMessage!;
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
        _lastErrorMessage = tr('手动地址为空', 'Manual host is empty');
        _status = _lastErrorMessage!;
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

  Future<void> _connectQuickRecent() async {
    final host = _mostRecentHost();
    if (host == null) return;
    final known = _servers.values.where((s) => s.host == host).toList();
    final wsPort = known.isNotEmpty ? known.first.wsPort : 39991;
    final udpPort = known.isNotEmpty ? known.first.udpPort : 39992;
    final serverName =
        known.isNotEmpty ? known.first.serverName : 'recent:$host';
    await _connectToHost(
      host: host,
      wsPort: wsPort,
      udpPort: udpPort,
      serverName: serverName,
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
      _runtimeState = 'connecting';
      _lastErrorMessage = null;
      _status = '${tr('连接中', 'connecting')}: $serverName ($host)';
    });
    try {
      await _backgroundService.startPlayback(
        host: host,
        wsPort: wsPort,
        udpPort: udpPort,
        serverName: serverName,
        transportMode: transportMode,
      );
      if (!mounted) return;
      setState(() {
        _recentConnectedHosts[host] = DateTime.now();
        _serviceTargetHost = host;
        _serviceTargetName = serverName;
        _runtimeState = 'handshaking';
        _status =
            '${tr('后台服务已启动', 'background service started')}: $serverName ($host)';
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _runtimeState = 'error';
        _lastErrorMessage = '$e';
        _status = '${tr('后台服务启动失败', 'service start failed')}: $e';
      });
    } finally {
      if (!mounted) return;
      setState(() {
        _isConnecting = false;
      });
    }
  }

  Future<void> _setAudioMode(AudioModePreference mode) async {
    final modeWire = _audioModeWire(mode);
    try {
      await _backgroundService.setAudioMode(
          mode: modeWire, reason: 'ui_select');
      if (!mounted) return;
      setState(() {
        _currentAudioMode = mode;
        _lastErrorMessage = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _runtimeState = 'error';
        _lastErrorMessage = '$e';
        _status = '${tr('模式切换失败', 'Audio mode change failed')}: $e';
      });
    }
  }

  Future<void> _startPlayback() async {
    final selected =
        _selectedServerId == null ? null : _servers[_selectedServerId!];
    final manual = _manualHostController.text.trim();
    final host = selected?.host ??
        _serviceTargetHost ??
        (manual.isEmpty ? null : manual);
    if (host == null || host.isEmpty) {
      setState(() {
        _runtimeState = 'error';
        _lastErrorMessage = tr('请先选择服务器', 'please select server first');
        _status = _lastErrorMessage!;
      });
      return;
    }
    final wsPort = selected?.wsPort ?? 39991;
    final udpPort = selected?.udpPort ?? 39992;
    final serverName =
        selected?.serverName ?? _serviceTargetName ?? 'manual:$host';
    await _connectToHost(
      host: host,
      wsPort: wsPort,
      udpPort: udpPort,
      serverName: serverName,
    );
  }

  Future<void> _stopPlayback() async {
    try {
      await _backgroundService.stopPlayback();
    } catch (_) {}
    if (!mounted) return;
    setState(() {
      _playbackState = PlaybackState.stopped;
      _wsConnected = false;
      _runtimeState = 'disconnected';
      _status = tr('后台播放已停止', 'background playback stopped');
    });
  }

  Future<void> _retryFromError() async {
    await _startPlayback();
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
    return tr('连接已选', 'Connect Selected');
  }

  int get _uiBufferedMs => _displayBufferedMs;
  int get _uiJitterBufferedMs => _serviceJitterBufferedMs;
  int get _uiTrackQueuedMs => _serviceTrackQueuedMs;
  int get _uiUnderrun => _serviceUnderrun;
  int get _uiDropped => _serviceDropped;
  int get _uiLate => _serviceLate;
  int get _uiFloorHoldCount => _serviceFloorHoldCount;
  int? get _uiJitterP95Ms => _serviceJitterP95Ms;
  int get _uiUdpPackets => _serviceUdpPackets;
  int get _uiUdpBytes => _serviceUdpBytes;
  int get _uiUdpLoss => _serviceLoss;
  int? get _uiLastSeq => _serviceLastSeq;
  int? get _uiAudioTrackLatencyMs => _serviceAudioTrackLatencyMs;
  double? get _uiRxFramesPerSec => _serviceRxFramesPerSec;

  ConsoleUiState get _consoleState => ConsoleStatusMapper.map(
        isConnecting: _isConnecting,
        wsConnected: _wsConnected,
        isPlaying: _playbackState == PlaybackState.playing,
        isBuffering: _playbackState == PlaybackState.buffering,
        runtimeState: _runtimeState,
        hasError: _lastErrorMessage != null,
      );

  ConsoleStatusViewData get _consoleStatus =>
      ConsoleStatusMapper.viewData(_consoleState);

  String get _heroMeta {
    final mode = _audioModeWire(_currentAudioMode);
    final version = _protocolVersion == null ? 'v?' : 'v${_protocolVersion!}';
    return '$mode · $_effectiveCodec · $version';
  }

  String get _serverAddress {
    final selected =
        _selectedServerId == null ? null : _servers[_selectedServerId!];
    final host = selected?.host ??
        _serviceTargetHost ??
        _manualHostController.text.trim();
    final wsPort = selected?.wsPort ?? 39991;
    if (host.isEmpty) return tr('未连接服务器', 'No server connected');
    return '$host:$wsPort';
  }

  String get _transportBadge => _transportMode == 'usb' ? 'USB' : 'Wi-Fi';

  bool get _modeSelectorEnabled {
    if (!_wsConnected) return false;
    return _consoleState != ConsoleUiState.connecting;
  }

  bool get _showLegacyMainDebugContent => false;

  String get _metricBufferText {
    if (_consoleState != ConsoleUiState.streaming &&
        _consoleState != ConsoleUiState.buffering) {
      return '--';
    }
    return '$_uiBufferedMs';
  }

  String get _metricFpsText {
    final value = _uiRxFramesPerSec;
    if (value == null) return '--';
    return value.toStringAsFixed(1);
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
      default:
        return AudioModePreference.balanced;
    }
  }

  String _statusChipLabel() {
    if (_isConnecting) return tr('连接中', 'CONNECTING');
    if (_consoleState == ConsoleUiState.streaming)
      return tr('推流中', 'STREAMING');
    if (_consoleState == ConsoleUiState.buffering)
      return tr('缓冲中', 'BUFFERING');
    if (_consoleState == ConsoleUiState.error) return tr('异常', 'ERROR');
    if (_wsConnected) return tr('已连接', 'CONNECTED');
    return tr('未连接', 'DISCONNECTED');
  }

  Color _statusChipColor(BuildContext context) {
    switch (_consoleState) {
      case ConsoleUiState.streaming:
        return AudioConsoleColors.teal;
      case ConsoleUiState.connecting:
        return AudioConsoleColors.amber;
      case ConsoleUiState.buffering:
        return AudioConsoleColors.teal;
      case ConsoleUiState.error:
        return AudioConsoleColors.error;
      case ConsoleUiState.idle:
        return AudioConsoleColors.text2;
    }
  }

  Widget _buildQuickConnectCard(List<DiscoveryServer> servers) {
    final recentHost = _mostRecentHost();
    if (recentHost == null) return const SizedBox.shrink();
    final matched = servers.where((s) => s.host == recentHost).toList();
    final wsPort = matched.isNotEmpty ? matched.first.wsPort : 39991;
    final udpPort = matched.isNotEmpty ? matched.first.udpPort : 39992;
    final name =
        matched.isNotEmpty ? matched.first.serverName : 'recent:$recentHost';

    return Card(
      color: AudioConsoleColors.surface,
      shape: RoundedRectangleBorder(
        borderRadius: AudioConsoleRadius.card,
        side: const BorderSide(color: AudioConsoleColors.border),
      ),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.flash_on,
                    size: 18, color: AudioConsoleColors.teal),
                const SizedBox(width: 6),
                Text(tr('快速连接', 'Quick Connect')),
              ],
            ),
            const SizedBox(height: 8),
            Text('$name ($recentHost) ws:$wsPort udp:$udpPort'),
            const SizedBox(height: 10),
            FilledButton(
              onPressed: _isConnecting ? null : _connectQuickRecent,
              child: Text(tr('连接最近设备', 'Connect Recent Server')),
            ),
          ],
        ),
      ),
    );
  }

  void _showConnectionSheet(List<DiscoveryServer> servers) {
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      backgroundColor: AudioConsoleColors.surface,
      builder: (context) => StatefulBuilder(
        builder: (context, setSheetState) => Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 20),
          child: SingleChildScrollView(
            child: _buildConnectionControls(
              servers,
              onModeChanged: (mode) {
                setState(() => _connectMode = mode);
                setSheetState(() {});
              },
              closeAfterAction: true,
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildConnectionControls(
    List<DiscoveryServer> servers, {
    required ValueChanged<ConnectMode> onModeChanged,
    bool closeAfterAction = false,
  }) {
    Future<void> runAndClose(Future<void> Function() action) async {
      if (closeAfterAction && Navigator.of(context).canPop()) {
        Navigator.of(context).pop();
      }
      await action();
    }

    final recentHost = _mostRecentHost();
    return Column(
      key: const Key('connection_controls_panel'),
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(tr('连接控制', 'Connection Control'), style: AudioConsoleType.title()),
        const SizedBox(height: 8),
        Text(
          '${tr('当前目标', 'Current target')}: $_serverAddress',
          key: const Key('connection_current_target'),
          style: AudioConsoleType.monoMeta(color: AudioConsoleColors.text2),
        ),
        if (recentHost != null) const SizedBox(height: 4),
        if (recentHost != null)
          Text(
            '${tr('最近设备', 'Recent device')}: $recentHost',
            key: const Key('connection_recent_device'),
            style: AudioConsoleType.monoMeta(color: AudioConsoleColors.text2),
          ),
        const SizedBox(height: 12),
        SegmentedButton<ConnectMode>(
          segments: [
            ButtonSegment(
              value: ConnectMode.discovered,
              icon: const Icon(Icons.radar),
              label: Text(tr('发现', 'Discover')),
            ),
            ButtonSegment(
              value: ConnectMode.manual,
              icon: const Icon(Icons.edit_location_alt),
              label: Text(tr('手动', 'Manual')),
            ),
            ButtonSegment(
              value: ConnectMode.usb,
              icon: const Icon(Icons.usb),
              label: Text(tr('USB', 'USB')),
            ),
          ],
          selected: <ConnectMode>{_connectMode},
          onSelectionChanged: (selection) => onModeChanged(selection.first),
        ),
        const SizedBox(height: 12),
        if (_connectMode == ConnectMode.manual)
          TextField(
            key: const Key('manual_host_input'),
            controller: _manualHostController,
            decoration: InputDecoration(
              border: const OutlineInputBorder(),
              labelText: tr('手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
              hintText: tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
            ),
          )
        else if (_connectMode == ConnectMode.discovered && servers.isNotEmpty)
          SizedBox(
            height: 132,
            child: ListView.builder(
              itemCount: servers.length,
              itemBuilder: (context, index) {
                final s = servers[index];
                final selected = s.serverId == _selectedServerId;
                return ListTile(
                  key: Key('connection_server_${s.host}'),
                  dense: true,
                  selected: selected,
                  title: Text('${s.serverName} (${s.host})'),
                  subtitle: Text('ws:${s.wsPort} udp:${s.udpPort}'),
                  onTap: () {
                    setState(() => _selectedServerId = s.serverId);
                  },
                );
              },
            ),
          )
        else if (_connectMode == ConnectMode.discovered)
          Text(
            tr('暂无发现结果，可以扫描或手动输入。',
                'No discovered server yet. Scan or input manually.'),
            style: AudioConsoleType.body(color: AudioConsoleColors.text2),
          )
        else
          Text(
            tr('使用 adb reverse 后连接手机本机地址。',
                'Use adb reverse, then connect to phone localhost.'),
            style: AudioConsoleType.body(color: AudioConsoleColors.text2),
          ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            FilledButton.icon(
              key: const Key('connection_primary_action'),
              onPressed: _isConnecting
                  ? null
                  : () {
                      if (_connectMode == ConnectMode.discovered) {
                        runAndClose(_connectSelected);
                      } else if (_connectMode == ConnectMode.usb) {
                        runAndClose(_connectUsb);
                      } else {
                        runAndClose(_connectManual);
                      }
                    },
              icon: const Icon(Icons.link),
              label: Text(_connectActionLabel()),
            ),
            OutlinedButton.icon(
              key: const Key('connection_scan_action'),
              onPressed: _probeRunning ? null : _probeSubnetForServers,
              icon: const Icon(Icons.search),
              label: Text(
                _probeRunning
                    ? tr('扫描中...', 'Scanning...')
                    : tr('扫描局域网', 'Scan LAN'),
              ),
            ),
            if (recentHost != null)
              OutlinedButton.icon(
                key: const Key('connection_recent_action'),
                onPressed: _isConnecting
                    ? null
                    : () => runAndClose(_connectQuickRecent),
                icon: const Icon(Icons.history),
                label: Text(tr('重新连接最近设备', 'Reconnect recent')),
              ),
          ],
        ),
      ],
    );
  }

  void _showAdvancedSheet() {
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      backgroundColor: AudioConsoleColors.surface,
      builder: (context) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 20),
        child: SingleChildScrollView(child: _buildAdvancedPanel()),
      ),
    );
  }

  Widget _buildAdvancedPanel() {
    return Column(
      key: const Key('advanced_debug_panel'),
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(tr('高级与调试', 'Advanced & Debug'), style: AudioConsoleType.title()),
        const SizedBox(height: 12),
        SegmentedButton<AppLang>(
          segments: const [
            ButtonSegment(value: AppLang.zh, label: Text('中文')),
            ButtonSegment(value: AppLang.en, label: Text('English')),
          ],
          selected: <AppLang>{_lang},
          onSelectionChanged: (selection) {
            setState(() => _lang = selection.first);
          },
        ),
        const SizedBox(height: 12),
        OutlinedButton.icon(
          key: const Key('advanced_check_update_action'),
          onPressed: _updateCheckRunning
              ? null
              : () => _checkForUpdate(
                    silentDelayMs: 0,
                    showNoUpdateHint: true,
                  ),
          icon: const Icon(Icons.system_update),
          label: Text(tr('检查更新', 'Check Update')),
        ),
        const SizedBox(height: 14),
        _buildDebugMetricsPanel(),
      ],
    );
  }

  Widget _buildDebugMetricsPanel() {
    final caps = _negotiatedCapabilities.entries
        .where((e) => e.value)
        .map((e) => e.key)
        .join(', ');
    final profile =
        _modeProfile.entries.map((e) => '${e.key}=${e.value}').join(', ');
    final lines = <String>[
      '${tr('协议版本', 'Protocol')}: v${_protocolVersion ?? 1}  |  ${tr('当前模式', 'Mode')}: ${_audioModeWire(_currentAudioMode)}',
      'codec: $_effectiveCodec',
      'data_plane: $_protocolPath',
      'transport: $_transportMode',
      'connected_clients: $_connectedClientCount',
      'playback_backend: $_playbackBackend',
      'server: ${_serverPlatform ?? 'unknown'} ${_serverAppVersion ?? ''}',
      'capabilities: $caps',
      'mode_profile: $profile',
      'sample_rate: $_sampleRate',
      'channels: $_channels',
      'total_buffered_ms: ${_serviceBufferedMs} (jitter: ${_uiJitterBufferedMs} + track: ${_uiTrackQueuedMs})',
      'audio_track_latency_ms: ${_uiAudioTrackLatencyMs == null ? '-' : '${_uiAudioTrackLatencyMs} ms'}',
      'jitter_underrun: $_uiUnderrun',
      'jitter_dropped: $_uiDropped',
      'jitter_late: $_uiLate',
      'floor_hold_count: $_uiFloorHoldCount',
      'jitter_p95_ms: ${_uiJitterP95Ms == null ? '-' : '${_uiJitterP95Ms} ms'}',
      'rx_frames_per_sec: ${_uiRxFramesPerSec == null ? '-' : _uiRxFramesPerSec!.toStringAsFixed(2)}',
      'udp_packets: $_uiUdpPackets',
      'udp_bytes: $_uiUdpBytes',
      'loss_estimate: $_uiUdpLoss',
      'last_seq: ${_uiLastSeq ?? '-'}',
      'tcp_rtt_ms: ${_tcpRoundTripMs ?? '-'}',
    ];
    return Container(
      key: const Key('advanced_debug_metrics'),
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: AudioConsoleColors.bg2,
        borderRadius: AudioConsoleRadius.button,
        border: Border.all(color: AudioConsoleColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(tr('调试指标', 'Debug Metrics'),
              style: AudioConsoleType.caption(color: AudioConsoleColors.text2)),
          const SizedBox(height: 8),
          for (final line in lines)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Text(line, style: AudioConsoleType.monoMeta()),
            ),
          const SizedBox(height: 8),
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(8),
            color: Colors.black,
            child: Text(
              _wsLog.isEmpty ? '(empty)' : _wsLog,
              style: AudioConsoleType.debugConsole(color: Colors.greenAccent),
              maxLines: 4,
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
      ),
    );
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
            SnackBar(content: Text(tr('当前已是最新版', 'Already up to date'))),
          );
        }
        return;
      }
      final version = (update['latestVersion'] as String?) ?? '';
      final releaseUrl = (update['releaseUrl'] as String?) ?? '';
      if (version.isEmpty || releaseUrl.isEmpty) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content:
              Text(tr('发现新版本 v$version', 'New version v$version is available')),
          action: SnackBarAction(
            label: tr('打开', 'Open'),
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
      await _platformChannel
          .invokeMethod('setFirstUseHintConsumed', {'consumed': true});
    } catch (_) {}
  }

  Future<void> _maybeShowFirstUseHint() async {
    if (!mounted || _firstUseHintShown) return;
    final consumed = await _getFirstUseHintConsumed();
    if (!mounted || consumed) return;
    _firstUseHintShown = true;
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(tr('首次使用提示', 'First-time Setup Tips')),
        content: Text(
          tr(
            '1. 确保桌面端服务已启动。\n2. 手机和电脑连接同一 Wi-Fi。\n3. 点击扫描或输入手动地址。',
            '1. Ensure desktop server is running.\n2. Phone and desktop are on the same Wi-Fi.\n3. Tap scan or enter server manually.',
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
        return b.lastSeen.compareTo(a.lastSeen);
      });
    _maybeSelectRecentOrFirst();

    final status = _consoleStatus;
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
            onPressed: _showAdvancedSheet,
            icon: const Icon(Icons.tune),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 10, 16, 20),
        children: [
          Row(
            children: [
              Chip(
                backgroundColor:
                    _statusChipColor(context).withValues(alpha: 0.2),
                label: Text(
                  _statusChipLabel(),
                  style: AudioConsoleType.statusChip(
                    color: _statusChipColor(context),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  _status,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AudioConsoleType.monoMeta(),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Container(
            padding: const EdgeInsets.symmetric(vertical: 20),
            decoration: BoxDecoration(
              color: AudioConsoleColors.bg2,
              borderRadius: AudioConsoleRadius.card,
              border: Border.all(color: AudioConsoleColors.border),
            ),
            child: HeroStatusWidget(
              status: status,
              meta: _heroMeta,
              isZh: _isZh,
            ),
          ),
          const SizedBox(height: 12),
          ServerCardWidget(
            title: tr('连接到', 'Connected to'),
            badge: _transportBadge,
            address: _serverAddress,
            hint: tr('点击管理连接目标', 'Tap to manage connection'),
            onTap: () => _showConnectionSheet(servers),
          ),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: MetricChipWidget(
                  label: 'buffer ms',
                  value: _metricBufferText,
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: MetricChipWidget(
                  label: 'rx fps',
                  value: _metricFpsText,
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: MetricChipWidget(
                  label: 'underrun',
                  value: _metricUnderrunText,
                  valueColor: _uiUnderrun > 0
                      ? AudioConsoleColors.amber
                      : AudioConsoleColors.text,
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          ModeSelectorWidget(
            items: modeItems,
            selectedId: _modeId(_currentAudioMode),
            enabled: _modeSelectorEnabled,
            onSelected: (id) {
              _setAudioMode(_modeFromId(id));
            },
          ),
          const SizedBox(height: 12),
          if (_consoleState == ConsoleUiState.error)
            FilledButton.tonal(
              onPressed: _isConnecting ? null : _retryFromError,
              child: Text(tr('重试连接', 'Retry Connection')),
            ),
          if (_consoleState == ConsoleUiState.error) const SizedBox(height: 8),
          DangerActionButton(
            label: tr('停止播放', 'Stop Playback'),
            enabled: _wsConnected || _playbackState != PlaybackState.stopped,
            onPressed: (_wsConnected || _playbackState != PlaybackState.stopped)
                ? _stopPlayback
                : null,
          ),
          if (_lastErrorMessage != null) const SizedBox(height: 12),
          if (_lastErrorMessage != null)
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: const Color.fromRGBO(239, 68, 68, 0.12),
                borderRadius: AudioConsoleRadius.button,
                border:
                    Border.all(color: const Color.fromRGBO(239, 68, 68, 0.3)),
              ),
              child: Text(
                _lastErrorMessage!,
                style:
                    AudioConsoleType.monoMeta(color: AudioConsoleColors.error),
              ),
            ),
          const SizedBox(height: 8),
          if (_showLegacyMainDebugContent) ...[
            Text(
              tr('高级与调试', 'Advanced & Debug'),
              style: AudioConsoleType.caption(color: AudioConsoleColors.text3),
            ),
            const SizedBox(height: 6),
            Opacity(opacity: 0.82, child: _buildQuickConnectCard(servers)),
            if (_mostRecentHost() != null) const SizedBox(height: 10),
            Opacity(
              opacity: 0.82,
              child: Card(
                color: AudioConsoleColors.surface,
                shape: RoundedRectangleBorder(
                  borderRadius: AudioConsoleRadius.card,
                  side: const BorderSide(color: AudioConsoleColors.border),
                ),
                child: ExpansionTile(
                  title: Text(tr('连接控制', 'Connection Control')),
                  subtitle: Text(
                    tr('发现/手动/USB 连接入口', 'Discovery/manual/USB entry'),
                    style: AudioConsoleType.monoMeta(
                        color: AudioConsoleColors.text3),
                  ),
                  childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                  children: [
                    SegmentedButton<ConnectMode>(
                      segments: [
                        ButtonSegment(
                          value: ConnectMode.discovered,
                          label: Text(tr('发现设备', 'Discovered')),
                        ),
                        ButtonSegment(
                          value: ConnectMode.manual,
                          label: Text(tr('手动地址', 'Manual')),
                        ),
                        ButtonSegment(
                          value: ConnectMode.usb,
                          label: Text(tr('USB(adb)', 'USB (adb)')),
                        ),
                      ],
                      selected: <ConnectMode>{_connectMode},
                      onSelectionChanged: (selection) {
                        setState(() {
                          _connectMode = selection.first;
                        });
                      },
                    ),
                    const SizedBox(height: 10),
                    if (_connectMode == ConnectMode.manual)
                      TextField(
                        controller: _manualHostController,
                        decoration: InputDecoration(
                          border: const OutlineInputBorder(),
                          labelText:
                              tr('手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
                          hintText: tr('例如 192.168.1.23', 'e.g. 192.168.1.23'),
                        ),
                      )
                    else if (servers.isNotEmpty)
                      SizedBox(
                        height: 120,
                        child: ListView.builder(
                          itemCount: servers.length,
                          itemBuilder: (context, index) {
                            final s = servers[index];
                            final selected = s.serverId == _selectedServerId;
                            return ListTile(
                              dense: true,
                              selected: selected,
                              title: Text('${s.serverName} (${s.host})'),
                              subtitle: Text('ws:${s.wsPort} udp:${s.udpPort}'),
                              onTap: () {
                                setState(() {
                                  _selectedServerId = s.serverId;
                                });
                              },
                            );
                          },
                        ),
                      )
                    else
                      Text(
                        tr('暂无发现结果，可扫描或手动输入。',
                            'No discovered server yet. Scan or input manually.'),
                      ),
                    const SizedBox(height: 10),
                    Row(
                      children: [
                        Expanded(
                          child: FilledButton(
                            onPressed: _isConnecting
                                ? null
                                : () {
                                    if (_connectMode ==
                                        ConnectMode.discovered) {
                                      _connectSelected();
                                    } else if (_connectMode ==
                                        ConnectMode.usb) {
                                      _connectUsb();
                                    } else {
                                      _connectManual();
                                    }
                                  },
                            child: Text(_connectActionLabel()),
                          ),
                        ),
                        const SizedBox(width: 8),
                        OutlinedButton(
                          onPressed:
                              _probeRunning ? null : _probeSubnetForServers,
                          child: Text(
                            _probeRunning
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
          ],
          const SizedBox(height: 10),
          Opacity(
            opacity: 0.72,
            child: Card(
              color: AudioConsoleColors.surface,
              shape: RoundedRectangleBorder(
                borderRadius: AudioConsoleRadius.card,
                side: const BorderSide(color: AudioConsoleColors.border),
              ),
              child: ExpansionTile(
                title: Text(tr('调试指标', 'Debug Metrics')),
                childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                children: [
                  Text(
                    '${tr('协议版本', 'Protocol')}: v${_protocolVersion ?? 1}  ·  ${tr('当前模式', 'Mode')}: ${_audioModeWire(_currentAudioMode)}',
                  ),
                  Text('codec: $_effectiveCodec'),
                  Text('data_plane: $_protocolPath'),
                  Text('transport: $_transportMode'),
                  Text('connected_clients: $_connectedClientCount'),
                  Text('playback_backend: $_playbackBackend'),
                  Text(
                      'server: ${_serverPlatform ?? 'unknown'} ${_serverAppVersion ?? ''}'),
                  Text(
                      'capabilities: ${_negotiatedCapabilities.entries.where((e) => e.value).map((e) => e.key).join(', ')}'),
                  Text(
                      'mode_profile: ${_modeProfile.entries.map((e) => '${e.key}=${e.value}').join(', ')}'),
                  Text('sample_rate: $_sampleRate'),
                  Text('channels: $_channels'),
                  Text(
                    'total_buffered_ms: ${_uiBufferedMs} (jitter: ${_uiJitterBufferedMs} + track: ${_uiTrackQueuedMs})',
                  ),
                  Text(
                    'audio_track_latency_ms: ${_uiAudioTrackLatencyMs == null ? '-' : '${_uiAudioTrackLatencyMs} ms'}',
                  ),
                  Text('jitter_underrun: $_uiUnderrun'),
                  Text('jitter_dropped: $_uiDropped'),
                  Text('jitter_late: $_uiLate'),
                  Text('floor_hold_count: $_uiFloorHoldCount'),
                  Text(
                      'jitter_p95_ms: ${_uiJitterP95Ms == null ? '-' : '${_uiJitterP95Ms} ms'}'),
                  Text(
                      'rx_frames_per_sec: ${_uiRxFramesPerSec == null ? '-' : _uiRxFramesPerSec!.toStringAsFixed(2)}'),
                  Text('udp_packets: $_uiUdpPackets'),
                  Text('udp_bytes: $_uiUdpBytes'),
                  Text('loss_estimate: $_uiUdpLoss'),
                  Text('last_seq: ${_uiLastSeq ?? '-'}'),
                  Text('tcp_rtt_ms: ${_tcpRoundTripMs ?? '-'}'),
                  const SizedBox(height: 8),
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(8),
                    color: Colors.black,
                    child: Text(
                      _wsLog.isEmpty ? '(empty)' : _wsLog,
                      style: AudioConsoleType.debugConsole(
                        color: Colors.greenAccent,
                      ),
                      maxLines: 4,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
          Opacity(
            opacity: 0.72,
            child: Card(
              color: AudioConsoleColors.surface,
              shape: RoundedRectangleBorder(
                borderRadius: AudioConsoleRadius.card,
                side: const BorderSide(color: AudioConsoleColors.border),
              ),
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Row(
                  children: [
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(tr('设置', 'Settings')),
                          const SizedBox(height: 4),
                          Text(
                            tr('应用更新', 'App update'),
                            style: AudioConsoleType.monoMeta(
                                color: AudioConsoleColors.text3),
                          ),
                        ],
                      ),
                    ),
                    OutlinedButton(
                      onPressed: _updateCheckRunning
                          ? null
                          : () => _checkForUpdate(
                                silentDelayMs: 0,
                                showNoUpdateHint: true,
                              ),
                      child: Text(tr('检查更新', 'Check Update')),
                    ),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(height: 10),
          Text(
            kUiBuildTag,
            textAlign: TextAlign.center,
            style: AudioConsoleType.caption(color: AudioConsoleColors.text3),
          ),
          const SizedBox(height: 6),
          Text(
            _audioLog,
            textAlign: TextAlign.center,
            style: AudioConsoleType.monoMeta(color: AudioConsoleColors.text3),
          ),
        ],
      ),
    );
  }
}
