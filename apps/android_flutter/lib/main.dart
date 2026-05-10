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
import 'power_saving_guide.dart';

const String kUiBuildTag = 'UI build: playback-diagnostics-v31';
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
      theme: ThemeData(colorSchemeSeed: Colors.teal, useMaterial3: true),
      home: const DebugPage(),
      routes: {
        PowerSavingGuidePage.routeName: (_) => const PowerSavingGuidePage(),
      },
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

  final Map<String, DiscoveryServer> _servers = {};
  final JitterBuffer _jitter =
      JitterBuffer(startBufferMs: 60, maxBufferMs: 300);
  final AudioTrackOutput _audioOutput = AudioTrackOutput();
  final BackgroundPlaybackService _backgroundService =
      BackgroundPlaybackService();
  final TextEditingController _manualHostController = TextEditingController();

  RawDatagramSocket? _discoverySocket;
  RawDatagramSocket? _udpSocket;
  WebSocket? _ws;
  Timer? _pingTimer;
  Timer? _playTimer;
  Timer? _probeTimer;
  StreamSubscription<PlaybackServiceSnapshot>? _serviceEventsSub;

  String _status = 'idle';
  String _wsLog = '';
  String _audioLog = '';
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
  bool _audioTrackInitialized = false;
  bool _audioTrackStarted = false;
  bool _playTickBusy = false;

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
  bool _eqEnabled = false;
  int _eqLowDb = 0;
  int _eqMidDb = 0;
  int _eqHighDb = 0;
  bool _loudnessNormalizationEnabled = false;
  double _loudnessGainDb = 0.0;
  bool _experimentalPath = false;
  bool _updateCheckRunning = false;

  @override
  void initState() {
    super.initState();
    final sysLang =
        PlatformDispatcher.instance.locale.languageCode.toLowerCase();
    _lang = sysLang.startsWith('zh') ? AppLang.zh : AppLang.en;
    debugPrint('ui_build $kUiBuildTag');
    _acquireMulticastLock();
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
          (metrics['loudness_gain_db'] as num?)?.toDouble() ??
              _loudnessGainDb;
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

      _status = '${snapshot.state}/${snapshot.rollbackState}';
      _audioLog = snapshot.state;
      _wsLog = jsonEncode(snapshot.toMap());
    });
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
    _pingTimer?.cancel();
    _ws?.close();
    _udpSocket?.close();
    _discoverySocket?.close();
    _releaseMulticastLock();
    _manualHostController.dispose();
    super.dispose();
  }

  Future<void> _startDiscovery() async {
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
          _servers[parsed.serverId] = parsed;
          _status = tr('正在监听设备发现', 'discovery listening');
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
        try {
          socket = await Socket.connect(ip, wsPort,
              timeout: const Duration(milliseconds: 160));
          if (!mounted) {
            return;
          }
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

  Future<void> _connectQuickRecent() async {
    final host = _mostRecentHost();
    if (host == null) {
      return;
    }
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
        'client_id': 'flutter-${DateTime.now().millisecondsSinceEpoch}',
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
        _audioLog = '';
      });
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
            mode: modeWire, reason: 'ui_select');
      } else {
        _ws?.add(jsonEncode({
          'type': 'set_audio_mode',
          'mode': modeWire,
          'reason': 'ui_select',
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
      _audioLog = 'protocol hint: config_changed';
    }
    if (packet.hasDiscontinuity) {
      // TODO(protocol-v2): reset jitter/decoder state on discontinuity.
      _audioLog = 'protocol hint: discontinuity';
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

  Future<void> _startPlayback() async {
    if (kUseBackgroundPlaybackService) {
      final selected =
          _selectedServerId == null ? null : _servers[_selectedServerId!];
      final manual = _manualHostController.text.trim();
      final host = selected?.host ??
          _serviceTargetHost ??
          (manual.isEmpty ? null : manual);
      if (host == null || host.isEmpty) {
        setState(() {
          _status = tr('请先选择服务器', 'please select server first');
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
      return;
    }
    if (_playbackState != PlaybackState.stopped) {
      return;
    }
    _jitter.clear();
    _audioTrackInitialized = false;
    _audioTrackStarted = false;
    _playTickBusy = false;

    setState(() {
      _playbackState = PlaybackState.buffering;
      _audioLog = 'playback buffering';
    });

    _playTimer?.cancel();
    _playTimer = Timer.periodic(const Duration(milliseconds: 10), (_) async {
      if (_playTickBusy) {
        return;
      }
      _playTickBusy = true;
      try {
        final frame = _jitter.pop();
        if (frame == null) {
          if (_playbackState != PlaybackState.buffering) {
            setState(() {
              _playbackState = PlaybackState.buffering;
            });
          }
          _audioLog = 'buffer underrun or no packet';
          _playTickBusy = false;
          return;
        }

        if (!_audioTrackInitialized) {
          await _audioOutput.init(
            sampleRate: frame.sampleRate,
            channels: frame.channels,
            frameSamplesPerChannel:
                frame.frameDurationMs * frame.sampleRate ~/ 1000,
          );
          _audioTrackInitialized = true;
        }

        if (!_audioTrackStarted) {
          await _audioOutput.start();
          _audioTrackStarted = true;
        }

        await _audioOutput.writePcm16(frame.payload);

        if (_playbackState != PlaybackState.playing) {
          setState(() {
            _playbackState = PlaybackState.playing;
          });
        } else {
          setState(() {});
        }
        _audioLog = 'playing pcm frame';
      } catch (e) {
        setState(() {
          _audioLog = 'AudioTrack init/write failed: $e';
          _playbackState = PlaybackState.stopped;
        });
      } finally {
        _playTickBusy = false;
      }
    });
  }

  Future<void> _stopPlayback() async {
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
    _audioTrackInitialized = false;
    _audioTrackStarted = false;

    if (mounted) {
      setState(() {
        _playbackState = PlaybackState.stopped;
      });
    }
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

  String _statusChipLabel() {
    if (_isConnecting) return tr('连接中', 'CONNECTING');
    if (_wsConnected && _playbackState == PlaybackState.playing)
      return tr('推流中', 'STREAMING');
    if (_wsConnected && _playbackState == PlaybackState.buffering)
      return tr('缓冲中', 'BUFFERING');
    if (_wsConnected) return tr('已连接', 'CONNECTED');
    return tr('未连接', 'DISCONNECTED');
  }

  Color _statusChipColor(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final label = _statusChipLabel();
    if (label == 'STREAMING' || label == '推流中') return Colors.green.shade600;
    if (label == 'BUFFERING' || label == '缓冲中') return Colors.orange.shade700;
    if (label == 'CONNECTED' || label == '已连接') return scheme.primary;
    if (label == 'CONNECTING' || label == '连接中') return scheme.secondary;
    return scheme.outline;
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
      onPressed: () => _applyEqPreset(preset),
      child: Text(label),
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

  Widget _buildQuickConnectCard(List<DiscoveryServer> servers) {
    final recentHost = _mostRecentHost();
    if (recentHost == null) {
      return const SizedBox.shrink();
    }
    final matched = servers.where((s) => s.host == recentHost).toList();
    final wsPort = matched.isNotEmpty ? matched.first.wsPort : 39991;
    final udpPort = matched.isNotEmpty ? matched.first.udpPort : 39992;
    final name =
        matched.isNotEmpty ? matched.first.serverName : 'recent:$recentHost';

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
            Text('$name ($recentHost) ws:$wsPort udp:$udpPort'),
            const SizedBox(height: 10),
            FilledButton(
              onPressed: _isConnecting ? null : _connectQuickRecent,
              child: Text(tr('一键连接最近设备', 'Connect Recent Server')),
            ),
          ],
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
        return b.lastSeen.compareTo(a.lastSeen);
      });

    _maybeSelectRecentOrFirst();

    return Scaffold(
      appBar: AppBar(
        title: Text(tr('局域网手机音响 MVP', 'LAN Audio Android MVP')),
        actions: [
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
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(kUiBuildTag,
              style: const TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              Chip(
                backgroundColor:
                    _statusChipColor(context).withValues(alpha: 0.18),
                label: Text(
                  _statusChipLabel(),
                  style: TextStyle(
                      color: _statusChipColor(context),
                      fontWeight: FontWeight.w700),
                ),
              ),
              Text(_status),
            ],
          ),
          const SizedBox(height: 10),
          _buildQuickConnectCard(servers),
          const SizedBox(height: 10),
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
                        label: Text(tr('USB（adb）', 'USB (adb)')),
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
                  if (_probeRunning)
                    Row(
                      children: [
                        const SizedBox(
                          width: 14,
                          height: 14,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                        const SizedBox(width: 8),
                        Text(tr('正在扫描局域网...', 'Scanning LAN...')),
                      ],
                    ),
                  if (_probeRunning) const SizedBox(height: 10),
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
                              '自动发现失败，可点击“扫描局域网”或手动输入服务器地址',
                              'Auto discovery failed. Try "Scan LAN" or enter server address manually.',
                            ),
                          ),
                          const SizedBox(height: 8),
                          FilledButton.tonal(
                            onPressed:
                                _probeRunning ? null : _probeSubnetForServers,
                            child: Text(
                              _probeRunning
                                  ? tr('扫描中...', 'Scanning...')
                                  : tr('扫描局域网', 'Scan LAN'),
                            ),
                          ),
                          const SizedBox(height: 6),
                          Text(
                            tr('提示：可切换到“手动地址”输入 IP。',
                                'Tip: switch to Manual and enter server IP.'),
                            style: const TextStyle(color: Colors.black54),
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
                          final selected = s.serverId == _selectedServerId;
                          final isRecent = _isRecentHost(s.host);
                          return ListTile(
                            dense: true,
                            selected: selected,
                            onTap: () {
                              setState(() {
                                _selectedServerId = s.serverId;
                              });
                            },
                            title: Row(
                              children: [
                                Expanded(
                                    child: Text('${s.serverName} (${s.host})')),
                                if (isRecent)
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
                            subtitle: Text('ws:${s.wsPort} udp:${s.udpPort}'),
                          );
                        },
                      ),
                    ),
                  const SizedBox(height: 10),
                  Row(
                    children: [
                      Expanded(
                        child: FilledButton(
                          onPressed: _isConnecting
                              ? null
                              : () {
                                  if (_connectMode == ConnectMode.discovered) {
                                    _connectSelected();
                                  } else if (_connectMode == ConnectMode.usb) {
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
          const SizedBox(height: 10),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(tr('播放', 'Playback'),
                      style: const TextStyle(
                          fontWeight: FontWeight.w700, fontSize: 16)),
                  const SizedBox(height: 8),
                  FilledButton(
                    onPressed: !_wsConnected
                        ? null
                        : (_playbackState == PlaybackState.stopped
                            ? _startPlayback
                            : _stopPlayback),
                    child: Text(
                      _playbackState == PlaybackState.stopped
                          ? tr('开始播放', 'Start Playback')
                          : tr('停止播放', 'Stop Playback'),
                    ),
                  ),
                  const SizedBox(height: 10),
                  Text(
                    '${tr('当前模式', 'Current mode')}: ${_audioModeLabel(_currentAudioMode)}',
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    '${tr('协议路径', 'Protocol path')}: $_protocolPath'
                    '${_experimentalPath ? ' (${tr('灰度', 'gray')})' : ''}',
                    style: const TextStyle(color: Colors.black54),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    'Codec: $_effectiveCodec',
                    style: const TextStyle(color: Colors.black54),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    '${tr('播放后端', 'Playback backend')}: $_playbackBackend  ·  '
                    '${tr('连接来源', 'Connection path')}: ${_connectionPathLabel(_connectionPath)}',
                    style: const TextStyle(color: Colors.black54),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    '${tr('传输模式', 'Transport mode')}: ${_transportMode == 'usb' ? 'USB' : 'WiFi'}  ·  '
                    '${tr('当前共', 'Connected')}: $_connectedClientCount ${tr('台设备连接中', 'devices listening')}',
                    style: const TextStyle(color: Colors.black54),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    'TCP RTT: ${_tcpRoundTripMs == null ? '-' : '${_tcpRoundTripMs} ms'} / ${_tcpRoundTripMedianMs == null ? '-' : '${_tcpRoundTripMedianMs} ms'}(med)',
                    style: const TextStyle(color: Colors.black54),
                  ),
                  const SizedBox(height: 6),
                  SegmentedButton<AudioModePreference>(
                    segments: [
                      ButtonSegment(
                        value: AudioModePreference.lowLatency,
                        label: Text(tr('低延迟', 'Low Latency')),
                      ),
                      ButtonSegment(
                        value: AudioModePreference.balanced,
                        label: Text(tr('平衡', 'Balanced')),
                      ),
                      ButtonSegment(
                        value: AudioModePreference.highQuality,
                        label: Text(tr('高音质', 'High Quality')),
                      ),
                    ],
                    selected: <AudioModePreference>{_currentAudioMode},
                    onSelectionChanged: !_wsConnected
                        ? null
                        : (selection) {
                            final mode = selection.first;
                            _setAudioMode(mode);
                          },
                  ),
                  const SizedBox(height: 10),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      _metricTile(tr('状态', 'Status'), _playbackLabel()),
                      _metricTile(
                        'total_buffered',
                        '${_uiBufferedMs} ms (jitter: ${_uiJitterBufferedMs} ms + track: ${_uiTrackQueuedMs} ms)',
                      ),
                      _metricTile(tr('欠载', 'Underrun'), '$_uiUnderrun'),
                      _metricTile(
                        tr('策略', 'Strategy'),
                        '${_modeProfile['startBufferMs'] ?? '-'} / ${_modeProfile['maxBufferMs'] ?? '-'} ms',
                      ),
                      _metricTile(
                        tr('响度增益', 'Loudness gain'),
                        _playbackState == PlaybackState.playing
                            ? '${_loudnessGainDb >= 0 ? '+' : ''}${_loudnessGainDb.toStringAsFixed(1)} dB'
                            : '-',
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Text('${tr('音频日志', 'Audio log')}: $_audioLog'),
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          tr('均衡器', 'Equalizer'),
                          style: const TextStyle(
                            fontWeight: FontWeight.w700,
                            fontSize: 16,
                          ),
                        ),
                      ),
                      Switch(
                        value: _eqEnabled,
                        onChanged: (value) => _setEq(enabled: value),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      _eqPresetButton(tr('平直', 'Flat'), 'flat'),
                      _eqPresetButton(tr('低音增强', 'Bass'), 'bass'),
                      _eqPresetButton(tr('人声清晰', 'Vocal'), 'vocal'),
                      _eqPresetButton(tr('高频亮丽', 'Bright'), 'bright'),
                    ],
                  ),
                  const SizedBox(height: 10),
                  SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(tr('响度归一化', 'Loudness normalization')),
                    subtitle: Text(tr(
                      'balanced/high_quality 生效，low_latency 自动旁路',
                      'Active in balanced/high_quality; bypassed in low_latency',
                    )),
                    value: _loudnessNormalizationEnabled,
                    onChanged: _setLoudnessNormalization,
                  ),
                  const SizedBox(height: 10),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                    children: [
                      _eqSlider(
                        label: tr('低频\n60Hz', 'Low\n60Hz'),
                        value: _eqLowDb,
                        onChanged: (value) => _setEq(lowDb: value),
                      ),
                      _eqSlider(
                        label: tr('中频\n1kHz', 'Mid\n1kHz'),
                        value: _eqMidDb,
                        onChanged: (value) => _setEq(midDb: value),
                      ),
                      _eqSlider(
                        label: tr('高频\n10kHz', 'High\n10kHz'),
                        value: _eqHighDb,
                        onChanged: (value) => _setEq(highDb: value),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
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
                    '如果自动发现失败，请使用“扫描局域网”或手动输入 Windows 端地址。',
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
                    'If audio stops in background, disable Android battery optimization or keep the app foregrounded.',
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 10),
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
                    onPressed: _openPowerSavingGuide,
                    child: Text(tr('后台播放', 'Background')),
                  ),
                  const SizedBox(width: 8),
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
          const SizedBox(height: 10),
          Card(
            child: ExpansionTile(
              title: Text(tr('调试指标', 'Debug Metrics')),
              childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
              children: [
                Text(
                  '${tr('协议版本', 'Protocol')}: v${_protocolVersion ?? 1}  ·  '
                  '${tr('当前模式', 'Mode')}: ${_audioModeLabel(_currentAudioMode)}',
                ),
                const SizedBox(height: 6),
                Text('Codec: $_effectiveCodec'),
                const SizedBox(height: 6),
                if (_serverPlatform != null || _serverAppVersion != null)
                  Text(
                    '${tr('服务端', 'Server')}: '
                    '${_serverPlatform ?? 'unknown'}'
                    '${_serverAppVersion == null ? '' : ' (${_serverAppVersion})'}',
                  ),
                if (_serverPlatform != null || _serverAppVersion != null)
                  const SizedBox(height: 6),
                Text(
                  '${tr('能力协商', 'Capabilities')}: '
                  '${_negotiatedCapabilities.entries.where((e) => e.value).map((e) => e.key).join(', ')}',
                  style: const TextStyle(color: Colors.black54),
                ),
                const SizedBox(height: 8),
                _metricTile('sample_rate', '$_sampleRate'),
                const SizedBox(height: 8),
                _metricTile('channels', '$_channels'),
                const SizedBox(height: 8),
                _metricTile(
                  'total_buffered_ms',
                  '${_uiBufferedMs} (jitter: ${_uiJitterBufferedMs} + track: ${_uiTrackQueuedMs})',
                ),
                const SizedBox(height: 8),
                _metricTile(
                  tr('AudioTrack 延迟', 'AudioTrack reported latency'),
                  _uiAudioTrackLatencyMs == null
                      ? '-'
                      : '${_uiAudioTrackLatencyMs} ms',
                ),
                const SizedBox(height: 8),
                _metricTile('jitter_underrun', '$_uiUnderrun'),
                const SizedBox(height: 8),
                _metricTile('jitter_dropped', '$_uiDropped'),
                const SizedBox(height: 8),
                _metricTile('jitter_late', '$_uiLate'),
                const SizedBox(height: 8),
                _metricTile('floor_hold_count', '$_uiFloorHoldCount'),
                const SizedBox(height: 8),
                _metricTile(
                  'jitter_p95_ms',
                  _uiJitterP95Ms == null ? '-' : '${_uiJitterP95Ms} ms',
                ),
                const SizedBox(height: 8),
                _metricTile(tr('UDP 包数', 'UDP packets'), '$_uiUdpPackets'),
                const SizedBox(height: 8),
                _metricTile('UDP bytes', '$_uiUdpBytes'),
                const SizedBox(height: 8),
                _metricTile(tr('丢包估计', 'Loss estimate'), '$_uiUdpLoss'),
                const SizedBox(height: 8),
                _metricTile(tr('最后序号', 'Last seq'), '${_uiLastSeq ?? '-'}'),
                const SizedBox(height: 8),
                Container(
                  width: double.infinity,
                  padding: const EdgeInsets.all(8),
                  color: Colors.black,
                  child: Text(
                    _wsLog.isEmpty ? '(empty)' : _wsLog,
                    style: const TextStyle(color: Colors.greenAccent),
                    maxLines: 4,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class PowerSavingGuidePage extends StatefulWidget {
  const PowerSavingGuidePage({super.key});

  static const routeName = '/power-saving-guide';

  @override
  State<PowerSavingGuidePage> createState() => _PowerSavingGuidePageState();
}

class _PowerSavingGuidePageState extends State<PowerSavingGuidePage> {
  static const MethodChannel _platformChannel =
      MethodChannel('lan_audio/platform');

  String _manufacturer = '';

  @override
  void initState() {
    super.initState();
    _loadManufacturer();
  }

  Future<void> _loadManufacturer() async {
    try {
      final manufacturer =
          await _platformChannel.invokeMethod<String>('getDeviceManufacturer');
      if (mounted) {
        setState(() => _manufacturer = manufacturer ?? '');
      }
    } catch (_) {
      // Keep generic instructions when native platform data is unavailable.
    }
  }

  @override
  Widget build(BuildContext context) {
    final locale =
        PlatformDispatcher.instance.locale.languageCode.toLowerCase();
    final isZh = locale.startsWith('zh');
    final steps = orderedPowerSavingGuideSteps(_manufacturer);
    return Scaffold(
      appBar: AppBar(
        title: Text(isZh ? '后台播放' : 'Background playback'),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            isZh
                ? 'LAN Audio 被省电模式限制时，请按下面步骤允许后台播放。'
                : 'If battery saver limits LAN Audio, allow background playback with the steps below.',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 12),
          if (_manufacturer.isNotEmpty)
            Text(
              isZh
                  ? '已识别设备品牌：$_manufacturer'
                  : 'Detected manufacturer: $_manufacturer',
              style: const TextStyle(color: Colors.black54),
            ),
          if (_manufacturer.isNotEmpty) const SizedBox(height: 12),
          for (final step in steps) ...[
            ListTile(
              contentPadding: EdgeInsets.zero,
              leading: const Icon(Icons.battery_saver),
              title: Text(step.zh),
              subtitle: Text(step.en),
            ),
            const Divider(height: 1),
          ],
        ],
      ),
    );
  }
}
