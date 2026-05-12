import 'package:flutter/services.dart';

enum MicStatus { off, connecting, streaming, error }

class MicCaptureService {
  static const MethodChannel _channel = MethodChannel('lan_audio/mic');
  static const MethodChannel _platformChannel = MethodChannel('lan_audio/platform');

  MicStatus status = MicStatus.off;
  double peakDb = -96.0;
  double rmsDb = -96.0;
  String errorMessage = '';

  Future<void> start({required String host, required int port}) async {
    status = MicStatus.connecting;
    try {
      await _channel.invokeMethod('startMicCapture', {
        'host': host,
        'reversePort': port,
      });
      status = MicStatus.streaming;
    } catch (e) {
      status = MicStatus.error;
      errorMessage = e.toString();
      rethrow;
    }
  }

  Future<void> stop() async {
    try {
      await _channel.invokeMethod('stopMicCapture');
    } catch (_) {}
    status = MicStatus.off;
  }

  Future<void> getLevel() async {
    try {
      final result = await _channel.invokeMapMethod<String, double>('getMicLevel');
      if (result != null) {
        peakDb = (result['peakDb'] ?? peakDb).toDouble();
        rmsDb = (result['rmsDb'] ?? rmsDb).toDouble();
      }
    } catch (_) {}
  }

  Future<String> getRationaleString() async {
    try {
      final result = await _platformChannel.invokeMethod<String>('getMicRationaleString');
      return result ?? 'Microphone access is needed to stream your voice to the PC audio system. No audio is recorded or saved.';
    } catch (_) {
      return 'Microphone access is needed to stream your voice to the PC audio system. No audio is recorded or saved.';
    }
  }

  Future<bool> requestPermission() async {
    try {
      final result = await _platformChannel.invokeMethod<bool>('requestMicPermission');
      return result ?? false;
    } catch (_) {
      return false;
    }
  }
}
