import 'dart:typed_data';

class LasPacket {
  LasPacket({
    this.version = 1,
    this.flags = 0,
    required this.sequence,
    required this.timestampMs,
    required this.sampleRate,
    required this.channels,
    required this.framesPerPacket,
    required this.payload,
  });

  final int version;
  final int flags;
  final int sequence;
  final int timestampMs;
  final int sampleRate;
  final int channels;
  final int framesPerPacket;
  final Uint8List payload;

  static LasPacket? parse(Uint8List bytes) {
    if (bytes.length < 4) {
      return null;
    }
    final magic = String.fromCharCodes(bytes.sublist(0, 4));
    if (magic == 'LAS1') {
      return _parseV1(bytes);
    }
    if (magic == 'LAV2') {
      return _parseV2(bytes);
    }
    return null;
  }

  static LasPacket? _parseV1(Uint8List bytes) {
    if (bytes.length < 27) {
      return null;
    }
    final bd = ByteData.sublistView(bytes);

    final version = bd.getUint8(4);
    final flags = bd.getUint8(5);
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
      version: version,
      flags: flags,
      sequence: sequence,
      timestampMs: timestampMs,
      sampleRate: sampleRate,
      channels: channels,
      framesPerPacket: framesPerPacket,
      payload: payload,
    );
  }

  static LasPacket? _parseV2(Uint8List bytes) {
    if (bytes.length < 33) {
      return null;
    }
    final bd = ByteData.sublistView(bytes);
    final version = bd.getUint8(4);
    if (version != 2) {
      return null;
    }
    final headerSize = bd.getUint16(5, Endian.little);
    if (headerSize < 33 || bytes.length < headerSize) {
      return null;
    }
    final flags16 = bd.getUint16(7, Endian.little);
    final sequence = bd.getUint32(9, Endian.little);
    final timestampMs = bd.getUint64(13, Endian.little);
    final codec = bd.getUint8(21);
    if (codec != 1) {
      // Gray stage: v2 data plane currently validates only PCM16.
      return null;
    }
    final channels = bd.getUint8(22);
    final sampleRate = bd.getUint32(23, Endian.little);
    final frameDurationMs = bd.getUint16(27, Endian.little);
    final payloadLen = bd.getUint16(29, Endian.little);
    if (bytes.length != headerSize + payloadLen) {
      return null;
    }
    final payload = Uint8List.sublistView(bytes, headerSize, headerSize + payloadLen);
    final framesPerPacket = sampleRate <= 0
        ? 0
        : ((sampleRate * frameDurationMs) / 1000).round();
    return LasPacket(
      version: version,
      flags: flags16 & 0xFF,
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

  bool get isSilence => (flags & 0x01) != 0;

  // TODO(protocol-v2): wire these two bits to runtime behavior after v2 header is enabled.
  bool get hasConfigChanged => (flags & 0x02) != 0;
  bool get hasDiscontinuity => (flags & 0x04) != 0;
}
