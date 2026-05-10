import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/main.dart';

void main() {
  test('parseLanAudioUri accepts host and ws port', () {
    final target = parseLanAudioUri('lan-audio://192.168.1.20:39991');

    expect(target, isNotNull);
    expect(target!.host, '192.168.1.20');
    expect(target.wsPort, 39991);
    expect(target.udpPort, 39992);
  });

  test('parseLanAudioUri rejects unrelated schemes', () {
    expect(parseLanAudioUri('https://example.com'), isNull);
  });
}
