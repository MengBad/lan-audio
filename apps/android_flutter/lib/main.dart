import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import 'audio/audio_track_output.dart';
import 'audio/jitter_buffer.dart';
import 'audio/las_packet.dart';

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

class DebugPage extends StatefulWidget {
  const DebugPage({super.key});

  @override
  State<DebugPage> createState() => _DebugPageState();
}

class _DebugPageState extends State<DebugPage> {
  final Map<String, DiscoveryServer> _servers = {};
  final JitterBuffer _jitter = JitterBuffer(startBufferMs: 60, maxBufferMs: 300);
  final AudioTrackOutput _audioOutput = AudioTrackOutput();

  RawDatagramSocket? _discoverySocket;
  RawDatagramSocket? _udpSocket;
  WebSocket? _ws;
  Timer? _pingTimer;
  Timer? _playTimer;

  String _status = 'idle';
  String _wsLog = '';
  String _audioLog = '';
  String? _selectedServerId;

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

  @override
  void initState() {
    super.initState();
    _startDiscovery();
  }

  @override
  void dispose() {
    _stopPlayback();
    _pingTimer?.cancel();
    _ws?.close();
    _udpSocket?.close();
    _discoverySocket?.close();
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
          _status = 'discovery listening';
          _selectedServerId ??= parsed.serverId;
        });
      });
    } catch (e) {
      setState(() {
        _status = 'discovery error: $e';
      });
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
        _status = 'no server selected';
      });
      return;
    }

    final server = _servers[id]!;
    await _ws?.close();
    _pingTimer?.cancel();
    await _stopPlayback();

    _udpSocket?.close();
    _udpSocket = await RawDatagramSocket.bind(InternetAddress.anyIPv4, 0);
    final udpPort = _udpSocket!.port;

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

    final ws = await WebSocket.connect('ws://${server.host}:${server.wsPort}/');
    _ws = ws;

    final hello = {
      'type': 'client_hello',
      'client_name': 'flutter-android',
      'udp_port': udpPort,
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
        _status = 'ws error: $e';
      });
    }, onDone: () {
      setState(() {
        _status = 'ws closed';
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
      _status = 'connected: ${server.serverName}';
      _udpPackets = 0;
      _udpBytes = 0;
      _udpLoss = 0;
      _lastSeq = null;
      _audioLog = '';
    });
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
        return 'playing';
      case PlaybackState.buffering:
        return 'buffering';
      case PlaybackState.stopped:
        return 'stopped';
    }
  }

  @override
  Widget build(BuildContext context) {
    final servers = _servers.values.toList()
      ..sort((a, b) => b.lastSeen.compareTo(a.lastSeen));

    return Scaffold(
      appBar: AppBar(title: const Text('LAN Audio Android MVP')),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Status: $_status'),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                FilledButton(
                  onPressed: _connectSelected,
                  child: const Text('Connect Selected'),
                ),
                FilledButton(
                  onPressed: _startPlayback,
                  child: const Text('Start Playback'),
                ),
                FilledButton.tonal(
                  onPressed: _stopPlayback,
                  child: const Text('Stop Playback'),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text('Playback: ${_playbackLabel()}'),
            Text('sample_rate: $_sampleRate'),
            Text('channels: $_channels'),
            Text('buffered_ms: ${_jitter.bufferedMs}'),
            Text('jitter_underrun: ${_jitter.stats.underrunCount}'),
            Text('jitter_dropped: ${_jitter.stats.droppedFrames}'),
            Text('jitter_late: ${_jitter.stats.lateFrames}'),
            const SizedBox(height: 8),
            Text('Audio log: $_audioLog'),
            const SizedBox(height: 8),
            const Text('Discovered Servers'),
            const SizedBox(height: 8),
            Expanded(
              child: ListView.builder(
                itemCount: servers.length,
                itemBuilder: (context, index) {
                  final s = servers[index];
                  final selected = s.serverId == _selectedServerId;
                  return Card(
                    child: ListTile(
                      selected: selected,
                      onTap: () {
                        setState(() {
                          _selectedServerId = s.serverId;
                        });
                      },
                      title: Text('${s.serverName} (${s.host})'),
                      subtitle: Text('ws:${s.wsPort} udp:${s.udpPort} id:${s.serverId}'),
                    ),
                  );
                },
              ),
            ),
            const Divider(),
            Text('UDP packets: $_udpPackets'),
            Text('UDP bytes: $_udpBytes'),
            Text('Loss estimate: $_udpLoss'),
            Text('Last seq: ${_lastSeq ?? '-'}'),
            const SizedBox(height: 8),
            const Text('WS last message:'),
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(8),
              color: Colors.black,
              child: Text(
                _wsLog.isEmpty ? '(empty)' : _wsLog,
                style: const TextStyle(color: Colors.greenAccent),
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
