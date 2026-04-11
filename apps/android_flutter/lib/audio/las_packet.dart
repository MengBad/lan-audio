import 'dart:typed_data';

class LasPacket {
  LasPacket({
    required this.sequence,
    required this.timestampMs,
    required this.sampleRate,
    required this.channels,
    required this.framesPerPacket,
    required this.payload,
  });

  final int sequence;
  final int timestampMs;
  final int sampleRate;
  final int channels;
  final int framesPerPacket;
  final Uint8List payload;

  static LasPacket? parse(Uint8List bytes) {
    if (bytes.length < 27) {
      return null;
    }
    final bd = ByteData.sublistView(bytes);
    final magic = String.fromCharCodes(bytes.sublist(0, 4));
    if (magic != 'LAS1') {
      return null;
    }

    final sequence = bd.getUint32(6, Endian.little);
    final timestampMs = bd.getUint64(10, Endian.little);
    final sampleRate = bd.getUint32(18, Endian.little);
    final channels = bd.getUint8(22);
    final framesPerPacket = bd.getUint16(23, Endian.little);
    final payloadLen = bd.getUint16(25, Endian.little);
    if (bytes.length != 27 + payloadLen) {
      return null;
    }

    final payload = Uint8List.sublistView(bytes, 27, 27 + payloadLen);
    return LasPacket(
      sequence: sequence,
      timestampMs: timestampMs,
      sampleRate: sampleRate,
      channels: channels,
      framesPerPacket: framesPerPacket,
      payload: payload,
    );
  }

  int get frameDurationMs {
    if (sampleRate <= 0) {
      return 0;
    }
    return (framesPerPacket * 1000 / sampleRate).round();
  }
}
