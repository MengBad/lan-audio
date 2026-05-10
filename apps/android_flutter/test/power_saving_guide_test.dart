import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/power_saving_guide.dart';

void main() {
  test('orders Xiaomi instructions first for Xiaomi devices', () {
    final steps = orderedPowerSavingGuideSteps('Xiaomi');

    expect(steps.first.brand, 'xiaomi');
    expect(steps.first.zh, contains('小米设备'));
    expect(steps.first.en, contains('Xiaomi'));
  });

  test('keeps generic instructions visible for unknown brands', () {
    final steps = orderedPowerSavingGuideSteps('unknown');

    expect(steps.map((step) => step.brand), contains('generic'));
  });
}
