import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/update_version.dart';

void main() {
  test('v1.7 greater than v1.6 reports update', () {
    expect(isRemoteVersionNewer('v1.7', 'v1.6'), isTrue);
  });

  test('v1.7 equal to v1.7 reports no update', () {
    expect(isRemoteVersionNewer('v1.7', 'v1.7'), isFalse);
  });

  test('v1.6 older than current v1.7 reports no update', () {
    expect(isRemoteVersionNewer('v1.6', 'v1.7'), isFalse);
  });
}
