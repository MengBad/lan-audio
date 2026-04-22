import 'dart:async';

import 'package:flutter/services.dart';

class PlaybackServiceSnapshot {
  PlaybackServiceSnapshot({
    required this.transport,
    required this.mode,
    required this.dataPlane,
    required this.activeDataPlane,
    required this.rollbackAvailable,
    required this.codec,
    required this.effectiveCodec,
    required this.state,
    required this.rollbackState,
    required this.metrics,
  });

  final String transport;
  final String mode;
  final String dataPlane;
  final String activeDataPlane;
  final bool rollbackAvailable;
  final String codec;
  final String effectiveCodec;
  final String state;
  final String rollbackState;
  final Map<String, dynamic> metrics;

  factory PlaybackServiceSnapshot.fromMap(Map<dynamic, dynamic> map) {
    final normalized = map.map(
      (key, value) => MapEntry('$key', value),
    );
    return PlaybackServiceSnapshot(
      transport: '${normalized['transport'] ?? 'wifi'}',
      mode: '${normalized['mode'] ?? 'balanced'}',
      dataPlane: '${normalized['data_plane'] ?? 'legacy_las1'}',
      activeDataPlane: '${normalized['active_data_plane'] ?? normalized['data_plane'] ?? 'legacy_las1'}',
      rollbackAvailable: normalized['rollback_available'] == true,
      codec: '${normalized['codec'] ?? 'pcm16'}',
      effectiveCodec: '${normalized['effective_codec'] ?? 'pcm16'}',
      state: '${normalized['state'] ?? 'disconnected'}',
      rollbackState: '${normalized['rollback_state'] ?? 'main_path_active'}',
      metrics: (normalized['metrics'] as Map?)?.map(
            (key, value) => MapEntry('$key', value),
          ) ??
              const <String, dynamic>{},
    );
  }

  Map<String, dynamic> toMap() {
    return <String, dynamic>{
      'transport': transport,
      'mode': mode,
      'data_plane': dataPlane,
      'active_data_plane': activeDataPlane,
      'rollback_available': rollbackAvailable,
      'codec': codec,
      'effective_codec': effectiveCodec,
      'state': state,
      'rollback_state': rollbackState,
      'metrics': metrics,
    };
  }
}

class BackgroundPlaybackService {
  static const MethodChannel _methodChannel =
      MethodChannel('lan_audio/playback_service');
  static const EventChannel _eventChannel =
      EventChannel('lan_audio/playback_events');

  Stream<PlaybackServiceSnapshot>? _events;

  Stream<PlaybackServiceSnapshot> events() {
    _events ??= _eventChannel.receiveBroadcastStream().map((dynamic event) {
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
    String transportMode = 'wifi',
  }) async {
    await _methodChannel.invokeMethod<void>('startPlayback', <String, dynamic>{
      'host': host,
      'wsPort': wsPort,
      'udpPort': udpPort,
      'serverName': serverName,
      'transportMode': transportMode,
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

  Future<void> setAudioMode({
    required String mode,
    String reason = 'ui_request',
  }) async {
    await _methodChannel.invokeMethod<void>('setAudioMode', <String, dynamic>{
      'mode': mode,
      'reason': reason,
    });
  }

  Future<PlaybackServiceSnapshot> getSnapshot() async {
    final raw =
        await _methodChannel.invokeMethod<Map<dynamic, dynamic>>('getSnapshot');
    return PlaybackServiceSnapshot.fromMap(raw ?? const <String, dynamic>{});
  }
}
