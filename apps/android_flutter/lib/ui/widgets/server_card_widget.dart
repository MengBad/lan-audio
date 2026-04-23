import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

class ServerCardWidget extends StatelessWidget {
  const ServerCardWidget({
    required this.title,
    required this.badge,
    required this.address,
    super.key,
  });

  final String title;
  final String badge;
  final String address;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const Key('server_card'),
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
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                decoration: BoxDecoration(
                  color: AudioConsoleColors.tealDim,
                  borderRadius: AudioConsoleRadius.pill,
                  border: Border.all(
                    color: const Color.fromRGBO(0, 212, 170, 0.3),
                  ),
                ),
                child: Text(
                  badge,
                  style:
                      AudioConsoleType.caption(color: AudioConsoleColors.teal),
                ),
              ),
            ],
          ),
          const SizedBox(height: AudioConsoleSpacing.xs),
          Text(
            address,
            key: const Key('server_address'),
            style: AudioConsoleType.monoMeta(color: AudioConsoleColors.teal),
          ),
        ],
      ),
    );
  }
}
