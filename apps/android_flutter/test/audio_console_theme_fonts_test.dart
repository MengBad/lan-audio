import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/ui/audio_console_theme.dart';

void main() {
  test('theme and semantic text styles use expected font families', () {
    final theme = buildAudioConsoleTheme();

    expect(theme.textTheme.bodyMedium?.fontFamily, AudioConsoleType.sansFamily);
    expect(AudioConsoleType.title().fontFamily, AudioConsoleType.sansFamily);
    expect(AudioConsoleType.body().fontFamily, AudioConsoleType.sansFamily);
    expect(AudioConsoleType.caption().fontFamily, AudioConsoleType.sansFamily);

    expect(
        AudioConsoleType.monoValue().fontFamily, AudioConsoleType.monoFamily);
    expect(AudioConsoleType.monoMeta().fontFamily, AudioConsoleType.monoFamily);
    expect(
      AudioConsoleType.debugConsole().fontFamily,
      AudioConsoleType.monoFamily,
    );
  });
}
