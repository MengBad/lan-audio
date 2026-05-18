import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/audio/las_packet.dart';

void main() {
  test('parse legacy LAS1 packet', () {
    final payload = Uint8List.fromList([1, 2, 3, 4]);
    final bytes = BytesBuilder()
      ..add('LAS1'.codeUnits)
      ..add([1]) // version
      ..add([0]) // flags
      ..add(_u32(7))
      ..add(_u64(123))
      ..add(_u32(48000))
      ..add([2]) // channels
      ..add(_u16(480))
      ..add(_u16(payload.length))
      ..add(payload);

    final packet = LasPacket.parse(bytes.toBytes());
    expect(packet, isNotNull);
    expect(packet!.version, 1);
    expect(packet.codec, LasPacket.codecPcm16);
    expect(packet.sequence, 7);
    expect(packet.payload.length, 4);
  });

  test('parse v2 LAV2 packet', () {
    final payload = Uint8List.fromList([5, 6, 7, 8]);
    const headerSize = 33;
    final bytes = BytesBuilder()
      ..add('LAV2'.codeUnits)
      ..add([2]) // protocol_version
      ..add(_u16(headerSize))
      ..add(_u16(0x06)) // config_changed + discontinuity
      ..add(_u32(9))
      ..add(_u64(456))
      ..add([1]) // codec pcm16
      ..add([2]) // channels
      ..add(_u32(48000))
      ..add(_u16(10)) // frame_duration_ms
      ..add(_u16(payload.length))
      ..add(_u16(0)) // reserved
      ..add(payload);

    final packet = LasPacket.parse(bytes.toBytes());
    expect(packet, isNotNull);
    expect(packet!.version, 2);
    expect(packet.codec, LasPacket.codecPcm16);
    expect(packet.hasConfigChanged, isTrue);
    expect(packet.hasDiscontinuity, isTrue);
    expect(packet.sequence, 9);
  });


  test('parse v2 LAV2 packet preserves 16-bit flags', () {
    final payload = Uint8List.fromList([1, 1]);
    const headerSize = 33;
    const flags = 0x0206;
    final bytes = BytesBuilder()
      ..add('LAV2'.codeUnits)
      ..add([2])
      ..add(_u16(headerSize))
      ..add(_u16(flags))
      ..add(_u32(11))
      ..add(_u64(1000))
      ..add([LasPacket.codecPcm16])
      ..add([2])
      ..add(_u32(48000))
      ..add(_u16(10))
      ..add(_u16(payload.length))
      ..add(_u16(0))
      ..add(payload);

    final packet = LasPacket.parse(bytes.toBytes());

    expect(packet, isNotNull);
    expect(packet!.flagsV2, flags);
    expect(packet.hasConfigChanged, isTrue);
    expect(packet.hasDiscontinuity, isTrue);
  });

  test('parse v2 LAV2 opus experimental packet', () {
    final payload = Uint8List.fromList([9, 8, 7]);
    const headerSize = 33;
    final bytes = BytesBuilder()
      ..add('LAV2'.codeUnits)
      ..add([2])
      ..add(_u16(headerSize))
      ..add(_u16(0))
      ..add(_u32(10))
      ..add(_u64(789))
      ..add([LasPacket.codecOpusExperimental])
      ..add([2])
      ..add(_u32(48000))
      ..add(_u16(10))
      ..add(_u16(payload.length))
      ..add(_u16(0))
      ..add(payload);

    final packet = LasPacket.parse(bytes.toBytes());

    expect(packet, isNotNull);
    expect(packet!.codec, LasPacket.codecOpusExperimental);
    expect(packet.sequence, 10);
    expect(packet.payload.length, payload.length);
  });
}

List<int> _u16(int value) {
  final b = ByteData(2)..setUint16(0, value, Endian.little);
  return b.buffer.asUint8List();
}

List<int> _u32(int value) {
  final b = ByteData(4)..setUint32(0, value, Endian.little);
  return b.buffer.asUint8List();
}

List<int> _u64(int value) {
  final b = ByteData(8)..setUint64(0, value, Endian.little);
  return b.buffer.asUint8List();
}
