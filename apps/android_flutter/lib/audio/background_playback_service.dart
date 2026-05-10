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
    required this.protocolVersion,
    required this.modeProfile,
    required this.negotiatedCapabilities,
    required this.serverPlatform,
    required this.serverAppVersion,
    required this.transportMode,
    required this.playbackBackend,
    required this.connectedClientCount,
    required this.eqEnabled,
    required this.eqSettings,
    required this.loudnessNormalizationEnabled,
    required this.reconnectAttempts,
    required this.reconnectDelayMs,
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
  final int? protocolVersion;
  final Map<String, dynamic> modeProfile;
  final Map<String, bool> negotiatedCapabilities;
  final String? serverPlatform;
  final String? serverAppVersion;
  final String transportMode;
  final String playbackBackend;
  final int connectedClientCount;
  final bool eqEnabled;
  final Map<String, dynamic> eqSettings;
  final bool loudnessNormalizationEnabled;
  final int reconnectAttempts;
  final int reconnectDelayMs;
  final Map<String, dynamic> metrics;

  factory PlaybackServiceSnapshot.fromMap(Map<dynamic, dynamic> map) {
    final normalized = map.map(
      (key, value) => MapEntry('$key', value),
    );
    return PlaybackServiceSnapshot(
      transport: '${normalized['transport'] ?? 'wifi'}',
      mode: '${normalized['mode'] ?? 'balanced'}',
      dataPlane: '${normalized['data_plane'] ?? 'legacy_las1'}',
      activeDataPlane:
          '${normalized['active_data_plane'] ?? normalized['data_plane'] ?? 'legacy_las1'}',
      rollbackAvailable: normalized['rollback_available'] == true,
      codec: '${normalized['codec'] ?? 'pcm16'}',
      effectiveCodec: '${normalized['effective_codec'] ?? 'pcm16'}',
      state: '${normalized['state'] ?? 'disconnected'}',
      rollbackState: '${normalized['rollback_state'] ?? 'main_path_active'}',
      protocolVersion: (normalized['protocol_version'] as num?)?.toInt(),
      modeProfile: (normalized['mode_profile'] as Map?)?.map(
            (key, value) => MapEntry('$key', value),
          ) ??
          const <String, dynamic>{},
      negotiatedCapabilities:
          (normalized['negotiated_capabilities'] as Map?)?.map(
                (key, value) => MapEntry('$key', value == true),
              ) ??
              const <String, bool>{},
      serverPlatform: normalized['server_platform']?.toString(),
      serverAppVersion: normalized['server_app_version']?.toString(),
      transportMode:
          '${normalized['transport_mode'] ?? normalized['transport'] ?? 'wifi'}',
      playbackBackend:
          '${normalized['playback_backend'] ?? 'audiotrack_stable'}',
      connectedClientCount:
          (normalized['connected_client_count'] as num?)?.toInt() ?? 0,
      eqEnabled: normalized['eq_enabled'] == true,
      eqSettings: (normalized['eq_settings'] as Map?)?.map(
            (key, value) => MapEntry('$key', value),
          ) ??
          const <String, dynamic>{},
      loudnessNormalizationEnabled:
          normalized['loudness_normalization_enabled'] == true,
      reconnectAttempts: (normalized['reconnect_attempts'] as num?)?.toInt() ??
          (normalized['reconnectAttempts'] as num?)?.toInt() ??
          0,
      reconnectDelayMs: (normalized['reconnect_delay_ms'] as num?)?.toInt() ??
          (normalized['reconnectDelayMs'] as num?)?.toInt() ??
          0,
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
      'protocol_version': protocolVersion,
      'mode_profile': modeProfile,
      'negotiated_capabilities': negotiatedCapabilities,
      'server_platform': serverPlatform,
      'server_app_version': serverAppVersion,
      'transport_mode': transportMode,
      'playback_backend': playbackBackend,
      'connected_client_count': connectedClientCount,
      'eq_enabled': eqEnabled,
      'eq_settings': eqSettings,
      'loudness_normalization_enabled': loudnessNormalizationEnabled,
      'reconnect_attempts': reconnectAttempts,
      'reconnect_delay_ms': reconnectDelayMs,
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

  Future<void> setEqSettings({
    required bool enabled,
    required int lowDb,
    required int midDb,
    required int highDb,
  }) async {
    await _methodChannel.invokeMethod<void>('setEqSettings', <String, dynamic>{
      'enabled': enabled,
      'lowDb': lowDb,
      'midDb': midDb,
      'highDb': highDb,
    });
  }

  Future<void> setLoudnessNormalization(bool enabled) async {
    await _methodChannel.invokeMethod<void>(
      'setLoudnessNormalization',
      <String, dynamic>{'enabled': enabled},
    );
  }

  Future<PlaybackServiceSnapshot> getSnapshot() async {
    final raw =
        await _methodChannel.invokeMethod<Map<dynamic, dynamic>>('getSnapshot');
    return PlaybackServiceSnapshot.fromMap(raw ?? const <String, dynamic>{});
  }
}
