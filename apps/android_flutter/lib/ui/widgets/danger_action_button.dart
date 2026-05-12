import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

class DangerActionButton extends StatelessWidget {
  const DangerActionButton({
    required this.label,
    required this.onPressed,
    this.enabled = true,
    super.key,
  });

  final String label;
  final VoidCallback? onPressed;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      key: const Key('danger_action'),
      width: double.infinity,
      child: OutlinedButton(
        style: OutlinedButton.styleFrom(
          backgroundColor: const Color.fromRGBO(239, 68, 68, 0.1),
          foregroundColor: AudioConsoleColors.error,
          side: const BorderSide(
            color: Color.fromRGBO(239, 68, 68, 0.3),
          ),
          shape: const RoundedRectangleBorder(
              borderRadius: AudioConsoleRadius.card),
          padding: const EdgeInsets.symmetric(vertical: AudioConsoleSpacing.md),
          textStyle: AudioConsoleType.buttonLabel(),
        ),
        onPressed: enabled ? onPressed : null,
        child: Text(label),
      ),
    );
  }
}
