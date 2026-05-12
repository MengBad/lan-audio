import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/main.dart';
import 'package:lan_audio_android_mvp/ui/jitter_graph_widget.dart';
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

    final app = tester.widget<MaterialApp>(find.byType(MaterialApp));
    expect(app.theme?.brightness, Brightness.dark);
    expect(find.textContaining('MVP'), findsNothing);
    expect(find.byType(ModeSelectorWidget), findsOneWidget);
    expect(find.byType(DangerActionButton), findsOneWidget);
    expect(find.byType(JitterGraphWidget), findsOneWidget);
    expect(find.byType(MicStatusWidget), findsOneWidget);
  });
}
