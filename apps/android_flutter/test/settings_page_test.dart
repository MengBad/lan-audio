import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/main.dart' show kAppVersion;
import 'package:lan_audio_android/services/mic_capture_service.dart';
import 'package:lan_audio_android/ui/mic_status_widget.dart';
import 'package:lan_audio_android/ui/pages/settings_page.dart';
import 'package:lan_audio_android/widgets/capsule_button.dart';

void main() {
  SettingsPage buildPage({
    String? preferredCodec,
    int? customBufferMs,
    required List<String?> codecChanges,
    required List<int?> bufferChanges,
    required List<String> presetChanges,
    required List<int> guideOpens,
  }) {
    return SettingsPage(
      isZh: false,
      preferredCodec: preferredCodec,
      onPreferredCodecChanged: codecChanges.add,
      customBufferMs: customBufferMs,
      onBufferMsChanged: bufferChanges.add,
      eqEnabled: true,
      eqLowDb: 0,
      eqMidDb: 0,
      eqHighDb: 0,
      onSetEq: ({bool? enabled, int? lowDb, int? midDb, int? highDb}) {},
      onApplyEqPreset: presetChanges.add,
      micService: MicCaptureService(),
      micEnabled: false,
      serviceTargetHost: '127.0.0.1',
      reverseChannelPort: 7878,
      onToggleMic: () async {},
      loudnessNormalizationEnabled: false,
      onSetLoudnessNormalization: (_) {},
      lang: 'en',
      onLangChanged: (_) {},
      onOpenPowerSavingGuide: () => guideOpens[0]++,
      onCheckUpdate: () {},
      updateCheckRunning: false,
      autoConnectEnabled: false,
      onAutoConnectChanged: (_) {},
      debugData: const {},
    );
  }

  testWidgets('SettingsPage wires the visible completion controls',
      (tester) async {
    final codecChanges = <String?>[];
    final bufferChanges = <int?>[];
    final presetChanges = <String>[];
    final guideOpens = <int>[0];

    await tester.pumpWidget(MaterialApp(
      home: buildPage(
        customBufferMs: 80,
        codecChanges: codecChanges,
        bufferChanges: bufferChanges,
        presetChanges: presetChanges,
        guideOpens: guideOpens,
      ),
    ));

    expect(find.byType(MicStatusWidget), findsOneWidget);
    expect(find.text('Background Playback'), findsOneWidget);
    expect(find.text('Version'), findsOneWidget);
    expect(find.text(kAppVersion), findsOneWidget);

    final backgroundKey = find.byKey(const Key('settings_background_playback'));
    await tester.ensureVisible(backgroundKey);
    await tester.pump();
    await tester.tap(backgroundKey);
    await tester.pump();
    expect(guideOpens, [1]);

    for (final preset in ['flat', 'bass', 'vocal', 'bright']) {
      final presetFinder = find.byKey(Key('eq_preset_$preset'));
      await tester.ensureVisible(presetFinder);
      await tester.pump();
      await tester.tap(presetFinder);
      await tester.pump();
    }
    expect(presetChanges, ['flat', 'bass', 'vocal', 'bright']);

    final autoFinder = find.byKey(const Key('settings_codec_auto'));
    await tester.ensureVisible(autoFinder);
    await tester.pump();
    await tester.tap(autoFinder);
    await tester.pump();
    expect(codecChanges, [null]);

    final resetFinder = find.byKey(const Key('settings_buffer_reset'));
    await tester.ensureVisible(resetFinder);
    await tester.pump();
    await tester.tap(resetFinder);
    await tester.pump();
    expect(bufferChanges, [null]);
  });

  testWidgets('SettingsPage Auto codec is selected for null preference',
      (tester) async {
    await tester.pumpWidget(MaterialApp(
      home: buildPage(
        codecChanges: <String?>[],
        bufferChanges: <int?>[],
        presetChanges: <String>[],
        guideOpens: <int>[0],
      ),
    ));

    final auto = tester.widget<CapsuleButton>(
      find.byKey(const Key('settings_codec_auto')),
    );
    expect(auto.selected, isTrue);
  });
}
