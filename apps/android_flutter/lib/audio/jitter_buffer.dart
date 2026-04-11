import 'dart:collection';
import 'dart:typed_data';

import 'las_packet.dart';

class JitterStats {
  int bufferedFrames = 0;
  int underrunCount = 0;
  int droppedFrames = 0;
  int lateFrames = 0;
}

class PcmFrame {
  PcmFrame({
    required this.sequence,
    required this.payload,
    required this.sampleRate,
    required this.channels,
    required this.frameDurationMs,
  });

  final int sequence;
  final Uint8List payload;
  final int sampleRate;
  final int channels;
  final int frameDurationMs;
}

class JitterBuffer {
  JitterBuffer({required this.startBufferMs, required this.maxBufferMs});

  final int startBufferMs;
  final int maxBufferMs;

  final SplayTreeMap<int, PcmFrame> _frames = SplayTreeMap<int, PcmFrame>();
  final JitterStats stats = JitterStats();

  bool _playoutStarted = false;
  int? _expectedSequence;
  int _frameDurationMs = 10;

  void clear() {
    _frames.clear();
    _playoutStarted = false;
    _expectedSequence = null;
    stats.bufferedFrames = 0;
  }

  void push(LasPacket packet) {
    _frameDurationMs = packet.frameDurationMs <= 0 ? 10 : packet.frameDurationMs;
    final frame = PcmFrame(
      sequence: packet.sequence,
      payload: packet.payload,
      sampleRate: packet.sampleRate,
      channels: packet.channels,
      frameDurationMs: _frameDurationMs,
    );

    if (_frames.containsKey(frame.sequence)) {
      stats.droppedFrames += 1;
      return;
    }

    if (_expectedSequence != null && _isOlder(frame.sequence, _expectedSequence!)) {
      stats.lateFrames += 1;
      return;
    }

    _frames[frame.sequence] = frame;
    _trimIfNeeded();
    stats.bufferedFrames = _frames.length;
  }

  PcmFrame? pop() {
    if (!_playoutStarted) {
      final startFrames = (startBufferMs / _frameDurationMs).ceil();
      if (_frames.length < startFrames) {
        stats.bufferedFrames = _frames.length;
        return null;
      }
      _playoutStarted = true;
      _expectedSequence = _frames.firstKey();
    }

    final expected = _expectedSequence;
    if (expected == null) {
      return null;
    }

    final frame = _frames.remove(expected);
    if (frame == null) {
      stats.underrunCount += 1;
      _expectedSequence = _nextSeq(expected);
      stats.bufferedFrames = _frames.length;
      return null;
    }

    _expectedSequence = _nextSeq(expected);
    stats.bufferedFrames = _frames.length;
    return frame;
  }

  int get bufferedMs => _frames.length * _frameDurationMs;

  void _trimIfNeeded() {
    final maxFrames = (maxBufferMs / _frameDurationMs).ceil().clamp(8, 2000) as int;
    while (_frames.length > maxFrames) {
      _frames.remove(_frames.firstKey());
      stats.droppedFrames += 1;
    }
  }

  bool _isOlder(int a, int b) {
    final diff = (a - b) & 0xFFFFFFFF;
    return diff > 0x80000000;
  }

  int _nextSeq(int seq) => (seq + 1) & 0xFFFFFFFF;
}
