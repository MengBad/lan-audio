import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/main.dart';

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

  test('firewall guidance covers connection refused and timeout', () {
    final refused =
        firewallGuidanceForMessage('java.net.ConnectException: ECONNREFUSED');
    final timeout =
        firewallGuidanceForMessage('SocketTimeoutException: timed out');

    expect(refused, isNotNull);
    expect(refused!.body, contains('Windows Firewall steps'));
    expect(timeout, isNotNull);
    expect(timeout!.body, contains('TCP+UDP port 39991'));
  });
}
