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
  testWidgets(
      'idle: hero exists, orb static semantics present, no streaming label',
      (tester) async {
    await tester.pumpWidget(_buildHarness(state: ConsoleUiState.idle));

    expect(find.byKey(const Key('hero_status')), findsOneWidget);
    expect(find.byKey(const Key('hero_orb_static')), findsOneWidget);
    expect(find.text('Streaming'), findsNothing);
  });

  testWidgets(
      'connecting: connecting semantics visible and mode selector disabled',
      (tester) async {
    await tester.pumpWidget(_buildHarness(
      state: ConsoleUiState.connecting,
      modeEnabled: false,
    ));

    expect(find.text('Connecting'), findsOneWidget);
    final opacity = tester
        .widget<Opacity>(find.byKey(const Key('mode_button_opacity_balanced')));
    expect(opacity.opacity, 0.5);
  });

  testWidgets('streaming: metrics row/server card/danger action are visible',
      (tester) async {
    await tester.pumpWidget(_buildHarness(state: ConsoleUiState.streaming));

    expect(find.byKey(const Key('server_card')), findsOneWidget);
    expect(find.byKey(const Key('metric_chip_buffer_ms')), findsOneWidget);
    expect(find.byKey(const Key('metric_chip_rx_fps')), findsOneWidget);
    expect(find.byKey(const Key('metric_chip_underrun')), findsOneWidget);
    expect(find.byKey(const Key('danger_action')), findsOneWidget);
    expect(find.byKey(const Key('hero_orb_pulsing')), findsOneWidget);
  });

  testWidgets('error: error semantics and recover entry are visible',
      (tester) async {
    await tester.pumpWidget(_buildHarness(state: ConsoleUiState.error));

    expect(find.text('Error'), findsOneWidget);
    expect(find.byKey(const Key('error_message')), findsOneWidget);
    expect(find.byKey(const Key('retry_action')), findsOneWidget);
  });

  testWidgets('mode selector: active and inactive states are visually distinct',
      (tester) async {
    await tester.pumpWidget(_buildHarness(
      state: ConsoleUiState.streaming,
      selectedModeId: 'balanced',
    ));

    final active = tester.widget<AnimatedContainer>(
      find.byKey(const Key('mode_button_container_balanced')),
    );
    final inactive = tester.widget<AnimatedContainer>(
      find.byKey(const Key('mode_button_container_low_latency')),
    );
    final activeDecoration = active.decoration! as BoxDecoration;
    final inactiveDecoration = inactive.decoration! as BoxDecoration;
    expect(activeDecoration.color, isNot(equals(inactiveDecoration.color)));
  });
}

Widget _buildHarness({
  required ConsoleUiState state,
  bool modeEnabled = true,
  String selectedModeId = 'balanced',
}) {
  final status = ConsoleStatusMapper.viewData(state);
  return MaterialApp(
    theme: buildAudioConsoleTheme(),
    home: Scaffold(
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          HeroStatusWidget(
            status: status,
            meta: 'balanced · opus · v2',
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
                  child: MetricChipWidget(label: 'buffer ms', value: '128')),
              SizedBox(width: 8),
              Expanded(child: MetricChipWidget(label: 'rx fps', value: '50.1')),
              SizedBox(width: 8),
              Expanded(child: MetricChipWidget(label: 'underrun', value: '0')),
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
            selectedId: selectedModeId,
            enabled: modeEnabled,
            onSelected: (_) {},
          ),
          const SizedBox(height: 12),
          if (state == ConsoleUiState.error)
            FilledButton.tonal(
              key: const Key('retry_action'),
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
              key: const Key('error_message'),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: const Color.fromRGBO(239, 68, 68, 0.12),
                borderRadius: AudioConsoleRadius.button,
                border:
                    Border.all(color: const Color.fromRGBO(239, 68, 68, 0.3)),
              ),
              child: const Text('ws error'),
            ),
        ],
      ),
    ),
  );
}
