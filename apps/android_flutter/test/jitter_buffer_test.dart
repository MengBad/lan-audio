import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/audio/jitter_buffer.dart';
import 'package:lan_audio_android/audio/las_packet.dart';

void main() {
  test('jitter buffer starts after enough buffered frames', () {
    final jb = JitterBuffer(startBufferMs: 40, maxBufferMs: 200);
    for (var i = 0; i < 3; i++) {
      jb.push(_pkt(seq: i, payloadSize: 1920));
    }
    expect(jb.pop(), isNull);

    jb.push(_pkt(seq: 3, payloadSize: 1920));
    expect(jb.pop(), isNotNull);
  });

  test('jitter buffer records underrun when expected frame missing', () {
    final jb = JitterBuffer(startBufferMs: 10, maxBufferMs: 200);
    jb.push(_pkt(seq: 10, payloadSize: 1920));
    expect(jb.pop(), isNotNull);
    expect(jb.pop(), isNull);
    expect(jb.stats.underrunCount, 1);
  });
}

LasPacket _pkt({required int seq, required int payloadSize}) {
  return LasPacket(
    sequence: seq,
    timestampMs: 1,
    sampleRate: 48000,
    channels: 2,
    framesPerPacket: 480,
    payload: Uint8List(payloadSize),
  );
}
