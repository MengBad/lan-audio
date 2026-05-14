import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

class MetricChipWidget extends StatelessWidget {
  const MetricChipWidget({
    required this.label,
    required this.value,
    this.valueColor = AudioConsoleColors.teal,
    super.key,
  });

  final String label;
  final String value;
  final Color valueColor;

  String _labelKey() =>
      label.toLowerCase().replaceAll(RegExp(r'[^a-z0-9]+'), '_');

  @override
  Widget build(BuildContext context) {
    return Container(
      key: Key('metric_chip_${_labelKey()}'),
      padding: const EdgeInsets.symmetric(
        horizontal: AudioConsoleSpacing.sm,
        vertical: AudioConsoleSpacing.sm,
      ),
      decoration: BoxDecoration(
        color: AudioConsoleColors.surface,
        borderRadius: AudioConsoleRadius.button,
        border: Border.all(color: AudioConsoleColors.border),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            value,
            key: ValueKey<String>('${_labelKey()}_$value'),
            style: AudioConsoleType.monoValue(color: valueColor),
          ),
          const SizedBox(height: 2),
          Text(
            label.toUpperCase(),
            style: AudioConsoleType.caption(),
          ),
        ],
      ),
    );
  }
}
