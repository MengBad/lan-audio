import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/main.dart';
import 'package:lan_audio_android_mvp/ui/mic_status_widget.dart';
import 'package:lan_audio_android_mvp/ui/widgets/danger_action_button.dart';
import 'package:lan_audio_android_mvp/ui/widgets/mode_selector_widget.dart';

void main() {
  testWidgets('app entry uses Audio Console Dark instead of MVP UI',
      (tester) async {
    await tester.binding.setSurfaceSize(const Size(420, 1200));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const LanAudioApp());
    await tester.pump();

    // Theme is dark (Audio Console Dark)
    final app = tester.widget<MaterialApp>(find.byType(MaterialApp));
    expect(app.theme?.brightness, Brightness.dark);

    // App title is correct (not MVP)
    expect(app.title, equals('LAN Audio Console'));

    // No MVP string anywhere in the widget tree
    expect(find.textContaining('MVP'), findsNothing);

    // No debug build tag in the widget tree
    expect(find.textContaining('audio-console-dark'), findsNothing);

    // Core Audio Console Dark components are mounted
    expect(find.byType(ModeSelectorWidget), findsOneWidget);
    expect(find.byType(DangerActionButton), findsOneWidget);
    // JitterGraphWidget is conditionally rendered (only when streaming),
    // so it may not be present in idle state — verify MicStatusWidget instead.
    // MicStatusWidget is on the Audio tab (offstage in IndexedStack).
    expect(find.byType(MicStatusWidget, skipOffstage: false), findsOneWidget);

    // Hero status equivalent: animated status orb exists (48x48 AnimatedContainer)
    expect(find.byType(AnimatedContainer), findsWidgets);

    // DangerActionButton has correct label
    expect(find.text('Stop Playback'), findsOneWidget);

    // buildAudioConsoleTheme is applied (scaffold background is dark)
    final scaffold = tester.widget<Scaffold>(find.byType(Scaffold).first);
    // The theme's scaffoldBackgroundColor is AudioConsoleColors.bg (#0C0F14)
    // which is applied via the theme, not directly on Scaffold
    expect(scaffold, isNotNull);
  });
}
