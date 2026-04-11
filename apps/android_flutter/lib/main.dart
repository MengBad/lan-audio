import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'audio/audio_track_output.dart';
import 'audio/jitter_buffer.dart';
import 'audio/las_packet.dart';

const String kUiBuildTag = 'UI build: playback-diagnostics-v23';

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
  static const MethodChannel _platformChannel = MethodChannel('lan_audio/platform');
  static bool _firstLaunchHintConsumed = false;

  final Map<String, DiscoveryServer> _servers = {};
  final JitterBuffer _jitter = JitterBuffer(startBufferMs: 60, maxBufferMs: 300);
  final AudioTrackOutput _audioOutput = AudioTrackOutput();
  final TextEditingController _manualHostController = TextEditingController();

  RawDatagramSocket? _discoverySocket;
  RawDatagramSocket? _udpSocket;
  WebSocket? _ws;
  Timer? _pingTimer;
  Timer? _playTimer;
  Timer? _probeTimer;

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

  PlaybackState _playbackState = PlaybackState.stopped;
  bool _audioTrackInitialized = false;
  bool _audioTrackStarted = false;
  bool _playTickBusy = false;

  int _sampleRate = 48000;
  int _channels = 2;
  int _framesPerPacket = 480;

  int _udpPackets = 0;
  int _udpBytes = 0;
  int _udpLoss = 0;
  int? _lastSeq;
  final Map<String, DateTime> _recentConnectedHosts = {};

  @override
  void initState() {
    super.initState();
    final sysLang = PlatformDispatcher.instance.locale.languageCode.toLowerCase();
    _lang = sysLang.startsWith('zh') ? AppLang.zh : AppLang.en;
    debugPrint('ui_build $kUiBuildTag');
    _acquireMulticastLock();
    _startDiscovery();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _maybeShowFirstUseHint();
    });
  }

  bool get _isZh => _lang == AppLang.zh;

  String tr(String zh, String en) => _isZh ? zh : en;

  String? _mostRecentHost() {
    if (_recentConnectedHosts.isEmpty) {
      return null;
    }
    final entries = _recentConnectedHosts.entries.toList()
      ..sort((a, b) => b.value.compareTo(a.value));
    return entries.first.key;
  }

  bool _isRecentHost(String host) => _recentConnectedHosts.containsKey(host);

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
    if (!mounted || _firstUseHintShown || _firstLaunchHintConsumed) {
      return;
    }
    _firstUseHintShown = true;
    _firstLaunchHintConsumed = true;
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

  @override
  void dispose() {
    _stopPlayback();
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
      final socket = await RawDatagramSocket.bind(InternetAddress.anyIPv4, 39990);
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
      final shouldProbe =
          _connectMode == ConnectMode.discovered && !_wsConnected && _servers.isEmpty;
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
        addresses.addAll(itf.addresses.where((a) => a.type == InternetAddressType.IPv4));
      }
      final local = addresses.where((a) {
        final ip = a.address;
        return ip.startsWith('192.168.') || ip.startsWith('10.') || ip.startsWith('172.');
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
          socket = await Socket.connect(ip, wsPort, timeout: const Duration(milliseconds: 160));
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
      final jsonObj = jsonDecode(utf8.decode(datagram.data)) as Map<String, dynamic>;
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

  Future<void> _connectQuickRecent() async {
    final host = _mostRecentHost();
    if (host == null) {
      return;
    }
    final known = _servers.values.where((s) => s.host == host).toList();
    final wsPort = known.isNotEmpty ? known.first.wsPort : 39991;
    final udpPort = known.isNotEmpty ? known.first.udpPort : 39992;
    final serverName = known.isNotEmpty ? known.first.serverName : 'recent:$host';
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
  }) async {
    setState(() {
      _isConnecting = true;
      _status = '${tr('连接中', 'connecting')}: $serverName ($host)';
    });
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
        'type': 'client_hello',
        'client_name': 'flutter-android',
        'udp_port': localUdpPort,
        'desired_sample_rate': 48000,
        'channels': 2,
      };
      ws.add(jsonEncode(hello));

      ws.listen((data) {
        setState(() {
          _wsLog = '$data';
        });
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
        _status = '${tr('已连接', 'connected')}: $serverName ($host ws:$wsPort udp:$udpPort)';
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

  void _handleUdpPacket(Uint8List bytes) {
    final packet = LasPacket.parse(bytes);
    if (packet == null) {
      return;
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
    _framesPerPacket = packet.framesPerPacket;

    _jitter.push(packet);

    if (_playbackState != PlaybackState.stopped) {
      final newState = _jitter.bufferedMs >= 40 ? PlaybackState.playing : PlaybackState.buffering;
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
            frameSamplesPerChannel: frame.frameDurationMs * frame.sampleRate ~/ 1000,
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
    if (_wsConnected && _playbackState == PlaybackState.playing) return tr('推流中', 'STREAMING');
    if (_wsConnected && _playbackState == PlaybackState.buffering) return tr('缓冲中', 'BUFFERING');
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
    if (_connectMode == ConnectMode.manual) return tr('手动连接', 'Connect Manual');
    if (_selectedServerId != null) {
      final selected = _servers[_selectedServerId!];
      if (selected != null && _isRecentHost(selected.host)) {
        return tr('快速连接', 'Quick Connect');
      }
    }
    return tr('连接所选', 'Connect Selected');
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
          Text(label, style: const TextStyle(fontSize: 11, color: Colors.black54)),
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
    final name = matched.isNotEmpty ? matched.first.serverName : 'recent:$recentHost';

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
                Text(tr('快速连接', 'Quick Connect'), style: const TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(width: 8),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.primary,
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Text(tr('最近连接', 'Recent'), style: const TextStyle(color: Colors.white, fontSize: 11)),
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
          Text(kUiBuildTag, style: const TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              Chip(
                backgroundColor: _statusChipColor(context).withValues(alpha: 0.18),
                label: Text(
                  _statusChipLabel(),
                  style: TextStyle(color: _statusChipColor(context), fontWeight: FontWeight.w700),
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
                      style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 16)),
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
                        labelText: tr('手动服务器地址 (IPv4)', 'Manual server host (IPv4)'),
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
                            onPressed: _probeRunning ? null : _probeSubnetForServers,
                            child: Text(
                              _probeRunning ? tr('扫描中...', 'Scanning...') : tr('扫描局域网', 'Scan LAN'),
                            ),
                          ),
                          const SizedBox(height: 6),
                          Text(
                            tr('提示：可切换到“手动地址”输入 IP。', 'Tip: switch to Manual and enter server IP.'),
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
                                Expanded(child: Text('${s.serverName} (${s.host})')),
                                if (isRecent)
                                  Container(
                                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                                    decoration: BoxDecoration(
                                      color: Colors.teal.withValues(alpha: 0.16),
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
                                  } else {
                                    _connectManual();
                                  }
                                },
                          child: Text(_connectActionLabel()),
                        ),
                      ),
                      const SizedBox(width: 8),
                      OutlinedButton(
                        onPressed: _probeRunning ? null : _probeSubnetForServers,
                        child: Text(
                          _probeRunning ? tr('扫描中...', 'Scanning...') : tr('扫描局域网', 'Scan LAN'),
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
                      style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 16)),
                  const SizedBox(height: 8),
                  FilledButton(
                    onPressed: !_wsConnected
                        ? null
                        : (_playbackState == PlaybackState.stopped ? _startPlayback : _stopPlayback),
                    child: Text(
                      _playbackState == PlaybackState.stopped
                          ? tr('开始播放', 'Start Playback')
                          : tr('停止播放', 'Stop Playback'),
                    ),
                  ),
                  const SizedBox(height: 10),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      _metricTile(tr('状态', 'Status'), _playbackLabel()),
                      _metricTile(tr('缓冲', 'Buffered'), '${_jitter.bufferedMs} ms'),
                      _metricTile(tr('欠载', 'Underrun'), '${_jitter.stats.underrunCount}'),
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
            child: ExpansionTile(
              title: Text(tr('调试指标', 'Debug Metrics')),
              childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
              children: [
                _metricTile('sample_rate', '$_sampleRate'),
                const SizedBox(height: 8),
                _metricTile('channels', '$_channels'),
                const SizedBox(height: 8),
                _metricTile('buffered_ms', '${_jitter.bufferedMs}'),
                const SizedBox(height: 8),
                _metricTile('jitter_underrun', '${_jitter.stats.underrunCount}'),
                const SizedBox(height: 8),
                _metricTile('jitter_dropped', '${_jitter.stats.droppedFrames}'),
                const SizedBox(height: 8),
                _metricTile('jitter_late', '${_jitter.stats.lateFrames}'),
                const SizedBox(height: 8),
                _metricTile(tr('UDP 包数', 'UDP packets'), '$_udpPackets'),
                const SizedBox(height: 8),
                _metricTile('UDP bytes', '$_udpBytes'),
                const SizedBox(height: 8),
                _metricTile(tr('丢包估计', 'Loss estimate'), '$_udpLoss'),
                const SizedBox(height: 8),
                _metricTile(tr('最后序号', 'Last seq'), '${_lastSeq ?? '-'}'),
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







