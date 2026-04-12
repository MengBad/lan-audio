import 'dart:async';

import 'package:flutter/services.dart';

class PlaybackServiceSnapshot {
  PlaybackServiceSnapshot({
    required this.serviceState,
    required this.connectionState,
    required this.playbackState,
    required this.targetHost,
    required this.targetName,
    required this.metrics,
    required this.recentLog,
    required this.error,
    required this.raw,
  });

  final String serviceState;
  final String connectionState;
  final String playbackState;
  final String? targetHost;
  final String? targetName;
  final Map<String, dynamic> metrics;
  final String recentLog;
  final Map<String, dynamic>? error;
  final Map<String, dynamic> raw;

  factory PlaybackServiceSnapshot.fromMap(Map<dynamic, dynamic> map) {
    final normalized = map.map(
      (key, value) => MapEntry('$key', value),
    );
    return PlaybackServiceSnapshot(
      serviceState: '${normalized['serviceState'] ?? 'idle'}',
      connectionState: '${normalized['connectionState'] ?? 'idle'}',
      playbackState: '${normalized['playbackState'] ?? 'stopped'}',
      targetHost: normalized['targetHost']?.toString(),
      targetName: normalized['targetName']?.toString(),
      metrics: (normalized['metrics'] as Map?)?.map(
            (key, value) => MapEntry('$key', value),
          ) ??
          const <String, dynamic>{},
      recentLog: '${normalized['recentLog'] ?? ''}',
      error: (normalized['error'] as Map?)?.map(
        (key, value) => MapEntry('$key', value),
      ),
      raw: normalized,
    );
  }
}

class BackgroundPlaybackService {
  static const MethodChannel _methodChannel =
      MethodChannel('lan_audio/playback_service');
  static const EventChannel _eventChannel =
      EventChannel('lan_audio/playback_events');

  Stream<PlaybackServiceSnapshot>? _events;

  Stream<PlaybackServiceSnapshot> events() {
    _events ??=
        _eventChannel.receiveBroadcastStream().map((dynamic event) {
      if (event is Map) {
        return PlaybackServiceSnapshot.fromMap(event);
      }
      return PlaybackServiceSnapshot.fromMap(const <String, dynamic>{});
    }).handleError((Object error) {
      // keep stream alive for UI subscription
    });
    return _events!;
  }

  Future<void> startPlayback({
    required String host,
    required int wsPort,
    required int udpPort,
    required String serverName,
  }) async {
    await _methodChannel.invokeMethod<void>('startPlayback', <String, dynamic>{
      'host': host,
      'wsPort': wsPort,
      'udpPort': udpPort,
      'serverName': serverName,
    });
  }

  Future<void> stopPlayback() async {
    await _methodChannel.invokeMethod<void>('stopPlayback');
  }

  Future<void> disconnect() async {
    await _methodChannel.invokeMethod<void>('disconnect');
  }

  Future<void> setOptions({
    required int startBufferMs,
    required int maxBufferMs,
    required int pingIntervalMs,
  }) async {
    await _methodChannel.invokeMethod<void>('setOptions', <String, dynamic>{
      'startBufferMs': startBufferMs,
      'maxBufferMs': maxBufferMs,
      'pingIntervalMs': pingIntervalMs,
    });
  }

  Future<PlaybackServiceSnapshot> getSnapshot() async {
    final raw =
        await _methodChannel.invokeMethod<Map<dynamic, dynamic>>('getSnapshot');
    return PlaybackServiceSnapshot.fromMap(raw ?? const <String, dynamic>{});
  }
}
