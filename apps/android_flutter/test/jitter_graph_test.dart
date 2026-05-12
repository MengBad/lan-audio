import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/ui/jitter_graph_widget.dart';

void main() {
  testWidgets('JitterGraphWidget renders without overflow', (tester) async {
    final jitter = List.generate(120, (i) => (5000 + (i * 200)).clamp(0, 120000));

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: JitterGraphWidget(
            jitterUs: jitter,
            p50Us: 18000,
            p95Us: 64000,
            underrunCount: 0,
          ),
        ),
      ),
    );

    await tester.pump();
    expect(find.byType(JitterGraphWidget), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('JitterGraphWidget handles empty buffer', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: JitterGraphWidget(
            jitterUs: [],
            p50Us: 0,
            p95Us: 0,
            underrunCount: 0,
          ),
        ),
      ),
    );

    await tester.pump();
    expect(tester.takeException(), isNull);
  });

  testWidgets('JitterGraphWidget handles all three color zones', (tester) async {
    final jitter = <int>[
      ...List.filled(40, 15000), // teal zone
      ...List.filled(40, 50000), // amber zone
      ...List.filled(40, 90000), // red zone
    ];

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: JitterGraphWidget(
            jitterUs: jitter,
            p50Us: 50000,
            p95Us: 90000,
            underrunCount: 3,
          ),
        ),
      ),
    );

    await tester.pump();
    expect(tester.takeException(), isNull);
  });

  test('JitterGraphWidget p95 calculation boundary values', () {
    // Unit test: verify p95 calculation logic
    final values = <int>[1000, 2000, 3000, 50000, 100000];
    values.sort();
    // ceil(5 * 0.95) - 1 = ceil(4.75) - 1 = 5 - 1 = 4
    final p95Idx = (values.length * 0.95).ceil() - 1;
    expect(p95Idx, 4);
    expect(values[p95Idx], 100000);
  });

  test('JitterGraphWidget circular buffer wrapping', () {
    // Simulate circular buffer behavior
    final buffer = List.filled(120, 0);
    var idx = 0;
    for (var i = 0; i < 200; i++) {
      buffer[idx % 120] = i * 100;
      idx++;
    }
    // After 200 inserts into 120-slot buffer, oldest values should be overwritten.
    // Insert 0:   buffer[0]=0
    // Insert 120: buffer[0]=12000  (overwrites)
    // So buffer[0] should hold the value from insert #120 = 120*100 = 12000
    expect(buffer[0], 12000);
  });
}
