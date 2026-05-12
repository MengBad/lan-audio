import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/ui/audio_console_status.dart';
import 'package:lan_audio_android_mvp/ui/audio_console_theme.dart';
import 'package:lan_audio_android_mvp/ui/widgets/danger_action_button.dart';
import 'package:lan_audio_android_mvp/ui/widgets/hero_status_widget.dart';
import 'package:lan_audio_android_mvp/ui/widgets/metric_chip_widget.dart';
import 'package:lan_audio_android_mvp/ui/widgets/mode_selector_widget.dart';
import 'package:lan_audio_android_mvp/ui/widgets/server_card_widget.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final resolutions = <({String name, Size size})>[
    (name: 'phone_small', size: const Size(360, 780)),
    (name: 'phone_large', size: const Size(412, 915)),
    (name: 'tablet_portrait', size: const Size(800, 1280)),
  ];

  for (final resolution in resolutions) {
    testWidgets('golden idle ${resolution.name}', (tester) async {
      await _pumpHarness(
        tester,
        resolution.size,
        state: ConsoleUiState.idle,
      );
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(
          'goldens/ui_console_idle_${resolution.name}.png',
        ),
      );
    });

    testWidgets('golden connecting ${resolution.name}', (tester) async {
      await _pumpHarness(
        tester,
        resolution.size,
        state: ConsoleUiState.connecting,
        modeEnabled: false,
      );
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(
          'goldens/ui_console_connecting_${resolution.name}.png',
        ),
      );
    });

    testWidgets('golden streaming ${resolution.name}', (tester) async {
      await _pumpHarness(
        tester,
        resolution.size,
        state: ConsoleUiState.streaming,
      );
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(
          'goldens/ui_console_streaming_${resolution.name}.png',
        ),
      );
    });

    testWidgets('golden buffering ${resolution.name}', (tester) async {
      await _pumpHarness(
        tester,
        resolution.size,
        state: ConsoleUiState.buffering,
      );
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(
          'goldens/ui_console_buffering_${resolution.name}.png',
        ),
      );
    });

    testWidgets('golden error ${resolution.name}', (tester) async {
      await _pumpHarness(
        tester,
        resolution.size,
        state: ConsoleUiState.error,
      );
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(
          'goldens/ui_console_error_${resolution.name}.png',
        ),
      );
    });
  }
}

Future<void> _pumpHarness(
  WidgetTester tester,
  Size size, {
  required ConsoleUiState state,
  bool modeEnabled = true,
}) async {
  await tester.binding.setSurfaceSize(size);
  addTearDown(() => tester.binding.setSurfaceSize(null));

  final status = ConsoleStatusMapper.viewData(state);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildAudioConsoleTheme(),
      home: Scaffold(
        body: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            HeroStatusWidget(
              status: status,
              meta: 'balanced | opus | v2',
              isZh: false,
            ),
            const SizedBox(height: 12),
            const ServerCardWidget(
              title: 'Connected to',
              badge: 'Wi-Fi',
              address: '10.0.0.185:39991',
            ),
            const SizedBox(height: 12),
            const Row(
              children: [
                Expanded(
                  child: MetricChipWidget(label: 'buffer ms', value: '128'),
                ),
                SizedBox(width: 8),
                Expanded(
                  child: MetricChipWidget(label: 'rx fps', value: '50.1'),
                ),
                SizedBox(width: 8),
                Expanded(
                  child: MetricChipWidget(label: 'underrun', value: '0'),
                ),
              ],
            ),
            const SizedBox(height: 12),
            ModeSelectorWidget(
              items: const [
                ModeSelectorItem(
                  id: 'low_latency',
                  name: 'Low Latency',
                  desc: 'Games/Video',
                ),
                ModeSelectorItem(
                  id: 'balanced',
                  name: 'Balanced',
                  desc: 'Daily',
                ),
                ModeSelectorItem(
                  id: 'high_quality',
                  name: 'High Quality',
                  desc: 'Music',
                ),
              ],
              selectedId: 'balanced',
              enabled: modeEnabled,
              onSelected: (_) {},
            ),
            const SizedBox(height: 12),
            if (state == ConsoleUiState.error)
              FilledButton.tonal(
                onPressed: () {},
                child: const Text('Retry Connection'),
              ),
            if (state == ConsoleUiState.error) const SizedBox(height: 8),
            const DangerActionButton(
              label: 'Stop Playback',
              enabled: true,
              onPressed: null,
            ),
            if (state == ConsoleUiState.error) const SizedBox(height: 8),
            if (state == ConsoleUiState.error)
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: const Color.fromRGBO(239, 68, 68, 0.12),
                  borderRadius: AudioConsoleRadius.button,
                  border: Border.all(
                    color: const Color.fromRGBO(239, 68, 68, 0.3),
                  ),
                ),
                child: const Text('ws error'),
              ),
          ],
        ),
      ),
    ),
  );
  await tester.pump(const Duration(milliseconds: 250));
}
