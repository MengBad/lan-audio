import 'package:flutter/material.dart';

class AudioConsoleColors {
  static const Color bg = Color(0xFF0C0F14);
  static const Color bg2 = Color(0xFF131720);
  static const Color bg3 = Color(0xFF1A1F2C);
  static const Color surface = Color(0xFF1E2433);
  static const Color surface2 = Color(0xFF252B3B);
  static const Color teal = Color(0xFF00D4AA);
  static const Color tealDim = Color.fromRGBO(0, 212, 170, 0.12);
  static const Color tealGlow = Color.fromRGBO(0, 212, 170, 0.25);
  static const Color amber = Color(0xFFF59E0B);
  static const Color error = Color(0xFFEF4444);
  static const Color text = Color(0xFFF0F2F7);
  static const Color text2 = Color(0xFF8892A4);
  static const Color text3 = Color(0xFF4A5568);
  static const Color border = Color.fromRGBO(255, 255, 255, 0.07);
  static const Color borderStrong = Color.fromRGBO(255, 255, 255, 0.12);
}

class AudioConsoleSpacing {
  static const double xxs = 4;
  static const double xs = 8;
  static const double sm = 10;
  static const double md = 12;
  static const double lg = 16;
  static const double xl = 20;
}

class AudioConsoleRadius {
  static const BorderRadius chip = BorderRadius.all(Radius.circular(6));
  static const BorderRadius button = BorderRadius.all(Radius.circular(10));
  static const BorderRadius card = BorderRadius.all(Radius.circular(14));
  static const BorderRadius pill = BorderRadius.all(Radius.circular(20));
}

class AudioConsoleMotion {
  static const Duration stateTransition = Duration(milliseconds: 200);
  static const Duration metricFade = Duration(milliseconds: 150);
  static const Duration orbPulse = Duration(milliseconds: 2000);
}

/// Section-card box shadow used by the Audio Console Dark cards.
/// Soft, low-spread drop shadow that matches the surface color tone.
class AudioConsoleShadow {
  static const List<BoxShadow> section = <BoxShadow>[
    BoxShadow(
      color: Color.fromRGBO(0, 0, 0, 0.25),
      blurRadius: 12,
      offset: Offset(0, 4),
    ),
  ];
}

class AudioConsoleType {
  static const String sansFamily = 'DM Sans';
  static const String monoFamily = 'IBM Plex Mono';

  static TextStyle title() => const TextStyle(
        fontFamily: sansFamily,
        fontSize: 20,
        fontWeight: FontWeight.w500,
        color: AudioConsoleColors.text,
      );

  static TextStyle monoValue({Color color = AudioConsoleColors.teal}) =>
      TextStyle(
        fontFamily: monoFamily,
        fontSize: 18,
        fontWeight: FontWeight.w500,
        color: color,
      );

  static TextStyle monoMeta({Color color = AudioConsoleColors.text2}) =>
      TextStyle(
        fontFamily: monoFamily,
        fontSize: 12,
        color: color,
      );

  static TextStyle caption({Color color = AudioConsoleColors.text3}) =>
      TextStyle(
        fontFamily: sansFamily,
        fontSize: 10,
        color: color,
        letterSpacing: 0.8,
        fontWeight: FontWeight.w500,
      );

  static TextStyle body({Color color = AudioConsoleColors.text}) => TextStyle(
        fontFamily: sansFamily,
        fontSize: 14,
        color: color,
      );

  static TextStyle statusChip({required Color color}) => TextStyle(
        fontFamily: sansFamily,
        fontSize: 12,
        color: color,
        fontWeight: FontWeight.w700,
      );

  static TextStyle buttonLabel({Color color = AudioConsoleColors.text}) =>
      TextStyle(
        fontFamily: sansFamily,
        fontSize: 14,
        color: color,
        fontWeight: FontWeight.w500,
      );

  static TextStyle debugConsole(
          {Color color = const Color(0xFF8FF7D6),
          FontWeight fontWeight = FontWeight.w400}) =>
      TextStyle(
        fontFamily: monoFamily,
        fontSize: 12,
        color: color,
        fontWeight: fontWeight,
      );
}

ThemeData buildAudioConsoleTheme() {
  const scheme = ColorScheme.dark(
    primary: AudioConsoleColors.teal,
    secondary: AudioConsoleColors.amber,
    error: AudioConsoleColors.error,
    surface: AudioConsoleColors.surface,
  );
  return ThemeData(
    brightness: Brightness.dark,
    colorScheme: scheme,
    scaffoldBackgroundColor: AudioConsoleColors.bg,
    cardColor: AudioConsoleColors.surface,
    useMaterial3: true,
    fontFamily: AudioConsoleType.sansFamily,
    appBarTheme: const AppBarTheme(
      backgroundColor: AudioConsoleColors.bg,
      foregroundColor: AudioConsoleColors.text,
      elevation: 0,
      centerTitle: false,
    ),
    dividerColor: AudioConsoleColors.border,
  );
}
