import 'package:flutter/services.dart';

class AudioTrackOutput {
  static const MethodChannel _channel = MethodChannel('lan_audio/audio_track');

  Future<void> init({
    required int sampleRate,
    required int channels,
    required int frameSamplesPerChannel,
  }) async {
    await _channel.invokeMethod('init', <String, dynamic>{
      'sampleRate': sampleRate,
      'channels': channels,
      'frameSamplesPerChannel': frameSamplesPerChannel,
    });
  }

  Future<void> start() async {
    await _channel.invokeMethod('start');
  }

  Future<void> writePcm16(Uint8List bytes) async {
    await _channel.invokeMethod('writePcm16', bytes);
  }

  Future<Map<String, int>> stats() async {
    final raw = await _channel.invokeMapMethod<String, dynamic>('stats');
    final values = raw ?? const <String, dynamic>{};
    return values
        .map((key, value) => MapEntry(key, (value as num?)?.toInt() ?? 0));
  }

  Future<void> stop() async {
    await _channel.invokeMethod('stop');
  }

  Future<void> release() async {
    await _channel.invokeMethod('release');
  }
}
