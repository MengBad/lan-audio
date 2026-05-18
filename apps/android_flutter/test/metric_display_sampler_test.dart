import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/ui/metric_display_sampler.dart';

void main() {
  test('buffer metric sampler publishes at most once per second', () {
    const sampler = MetricDisplaySampler();
    final start = DateTime.fromMillisecondsSinceEpoch(1000);

    expect(
      sampler.canPublish(
        now: start,
        lastPublishedAt: DateTime.fromMillisecondsSinceEpoch(0),
        runtimeState: 'streaming',
      ),
      isTrue,
    );
    expect(
      sampler.canPublish(
        now: start.add(const Duration(milliseconds: 999)),
        lastPublishedAt: start,
        runtimeState: 'streaming',
      ),
      isFalse,
    );
    expect(
      sampler.canPublish(
        now: start.add(const Duration(seconds: 1)),
        lastPublishedAt: start,
        runtimeState: 'streaming',
      ),
      isTrue,
    );
  });

  test('buffer metric sampler pauses outside active runtime states', () {
    const sampler = MetricDisplaySampler();
    final now = DateTime.fromMillisecondsSinceEpoch(3000);
    final last = DateTime.fromMillisecondsSinceEpoch(1000);

    expect(
      sampler.canPublish(
        now: now,
        lastPublishedAt: last,
        runtimeState: 'disconnected',
      ),
      isFalse,
    );
    expect(
      sampler.canPublish(
        now: now,
        lastPublishedAt: last,
        runtimeState: 'recovering',
      ),
      isTrue,
    );
  });
}
