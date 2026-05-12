import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

class ServerCardWidget extends StatelessWidget {
  const ServerCardWidget({
    required this.title,
    required this.badge,
    required this.address,
    this.onTap,
    this.hint,
    super.key,
  });

  final String title;
  final String badge;
  final String address;
  final VoidCallback? onTap;
  final String? hint;

  @override
  Widget build(BuildContext context) {
    return Material(
      key: const Key('server_card'),
      color: Colors.transparent,
      child: InkWell(
        borderRadius: AudioConsoleRadius.card,
        onTap: onTap,
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.symmetric(
            horizontal: AudioConsoleSpacing.lg,
            vertical: AudioConsoleSpacing.md,
          ),
          decoration: BoxDecoration(
            color: AudioConsoleColors.surface,
            borderRadius: AudioConsoleRadius.card,
            border: Border.all(color: AudioConsoleColors.borderStrong),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Text(title,
                      style: AudioConsoleType.caption(
                          color: AudioConsoleColors.text2)),
                  const Spacer(),
                  Container(
                    key: const Key('server_badge'),
                    padding:
                        const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                    decoration: BoxDecoration(
                      color: AudioConsoleColors.tealDim,
                      borderRadius: AudioConsoleRadius.pill,
                      border: Border.all(
                        color: const Color.fromRGBO(0, 212, 170, 0.3),
                      ),
                    ),
                    child: Text(
                      badge,
                      style: AudioConsoleType.caption(
                          color: AudioConsoleColors.teal),
                    ),
                  ),
                  const SizedBox(width: 8),
                  const Icon(Icons.expand_more,
                      size: 18, color: AudioConsoleColors.text2),
                ],
              ),
              const SizedBox(height: AudioConsoleSpacing.xs),
              Text(
                address,
                key: const Key('server_address'),
                style:
                    AudioConsoleType.monoMeta(color: AudioConsoleColors.teal),
              ),
              if (hint != null) const SizedBox(height: 4),
              if (hint != null)
                Text(
                  hint!,
                  key: const Key('server_card_hint'),
                  style:
                      AudioConsoleType.caption(color: AudioConsoleColors.text2),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
