import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/main.dart';
import 'package:lan_audio_android/ui/pages/connect_page.dart';
import 'package:lan_audio_android/ui/audio_console_status.dart';
import 'package:lan_audio_android/ui/audio_console_theme.dart';
import 'package:lan_audio_android/ui/pages/play_page.dart';
import 'package:lan_audio_android/ui/widgets/mode_selector_widget.dart';
import 'package:lan_audio_android/ui/pages/more_page.dart';
import 'package:lan_audio_android/ui/pages/settings_page.dart';
import 'package:lan_audio_android/ui/widgets/latency_chart_widget.dart';
import 'package:lan_audio_android/ui/widgets/danger_action_button.dart';

void main() {
  testWidgets('app entry uses Audio Console Dark instead of MVP UI',
      (tester) async {
    await tester.binding.setSurfaceSize(const Size(480, 1200));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const LanAudioApp());
    await tester.pump();

    // Theme is dark (Audio Console Dark)
    final app = tester.widget<MaterialApp>(find.byType(MaterialApp));
    expect(app.theme?.brightness, Brightness.dark);

    // App title is correct (not MVP)
    expect(app.title, equals('LAN Audio'));

    // No MVP string anywhere in the widget tree
    expect(find.textContaining('MVP'), findsNothing);

    // No debug build tag in the widget tree
    expect(find.textContaining('audio-console-dark'), findsNothing);

    // --- 3-tab widget tree: PlayPage, ConnectPage, SettingsPage ---
    // MoreServerData (defined in more_page.dart) is the server contract used by
    // ConnectPage; MorePage itself is intentionally no longer mounted.
    // IndexedStack wraps non-active children in Offstage, so we use
    // skipOffstage: false to find them.
    expect(find.byType(PlayPage), findsOneWidget);
    expect(
      find.byType(ConnectPage, skipOffstage: false),
      findsOneWidget,
    );
    expect(
      find.byType(SettingsPage, skipOffstage: false),
      findsOneWidget,
    );

    // 3-tab navigation icons present: play_circle_outline, link, settings
    expect(find.byIcon(Icons.play_circle_outline), findsOneWidget);
    expect(find.byIcon(Icons.link), findsOneWidget);
    expect(find.byIcon(Icons.settings), findsOneWidget);

    // Core Audio Console Dark components are mounted in PlayPage
    expect(find.byType(ModeSelectorWidget), findsOneWidget);
    expect(find.byType(DangerActionButton), findsOneWidget);

    // Status orb: AnimatedContainer exists (100px orb in PlayPage)
    expect(find.byType(AnimatedContainer), findsWidgets);

    // DangerActionButton has correct label
    expect(find.text('Stop Playback'), findsOneWidget);

    // MoreServerData is the shared server contract consumed by ConnectPage.
    expect(
      const MoreServerData(
        serverId: 's',
        serverName: 'n',
        host: 'h',
        wsPort: 39991,
        udpPort: 39992,
        latencyMs: null,
      ).serverId,
      's',
    );

    // buildAudioConsoleTheme() is applied (scaffold background is dark)
    final scaffold = tester.widget<Scaffold>(find.byType(Scaffold).first);
    expect(scaffold, isNotNull);
    // The theme's scaffoldBackgroundColor comes from buildAudioConsoleTheme()
    // which sets it to AudioConsoleColors.bg (#0C0F14) — a dark color
    final scaffoldBgColor =
        Theme.of(tester.element(find.byType(Scaffold).first))
            .scaffoldBackgroundColor;
    // Verify it's a dark background (luminance < 0.1 means very dark)
    expect(scaffoldBgColor.computeLuminance(), lessThan(0.1));
  });

  testWidgets('connected PlayPage mounts the live latency chart', (tester) async {
    await tester.binding.setSurfaceSize(const Size(480, 1200));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(MaterialApp(
      theme: buildAudioConsoleTheme(),
      home: Scaffold(
        body: PlayPage(
          isZh: false,
          consoleState: ConsoleUiState.streaming,
          statusChipLabel: 'Connected',
          statusText: 'Streaming',
          isConnecting: false,
          wsConnected: true,
          playbackStopped: false,
          metricBufferText: '60',
          metricUnderrunText: '0',
          tcpRoundTripMs: 4,
          modeItems: const [
            ModeSelectorItem(id: 'balanced', name: 'Balanced', desc: 'Daily'),
          ],
          currentModeId: 'balanced',
          modeSelectorEnabled: true,
          onModeSelected: (_) {},
          onStopPlayback: () {},
          onRetryConnection: () {},
          onConnectPressed: () {},
          onDisconnectPressed: () {},
          serverName: 'test-server',
          currentLatencyMs: () => 64.0,
          baselineLatencyMs: () => 185.0,
          effectiveCodec: 'opus',
          sampleRate: 48000,
          channels: 2,
        ),
      ),
    ));
    await tester.pump();

    expect(find.byType(LatencyChartWidget), findsOneWidget);
    expect(find.byType(ModeSelectorWidget), findsOneWidget);
    expect(find.byType(DangerActionButton), findsOneWidget);

    // Passed-in status label/text are surfaced in the compact status area.
    expect(find.text('Connected'), findsOneWidget);
    expect(find.text('Streaming'), findsOneWidget);
    expect(find.text('Now Playing'), findsOneWidget);
  });
}
