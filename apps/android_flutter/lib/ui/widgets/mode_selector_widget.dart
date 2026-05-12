import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

class ModeSelectorItem {
  const ModeSelectorItem({
    required this.id,
    required this.name,
    required this.desc,
  });

  final String id;
  final String name;
  final String desc;
}

class ModeSelectorWidget extends StatelessWidget {
  const ModeSelectorWidget({
    required this.items,
    required this.selectedId,
    required this.enabled,
    required this.onSelected,
    super.key,
  });

  final List<ModeSelectorItem> items;
  final String selectedId;
  final bool enabled;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: items
          .map(
            (item) => Expanded(
              child: Padding(
                padding: EdgeInsets.only(
                  right: item == items.last ? 0 : AudioConsoleSpacing.xs,
                ),
                child: _ModeButton(
                  item: item,
                  active: item.id == selectedId,
                  enabled: enabled,
                  key: Key('mode_button_${item.id}'),
                  onTap: () => onSelected(item.id),
                ),
              ),
            ),
          )
          .toList(),
    );
  }
}

class _ModeButton extends StatelessWidget {
  const _ModeButton({
    super.key,
    required this.item,
    required this.active,
    required this.enabled,
    required this.onTap,
  });

  final ModeSelectorItem item;
  final bool active;
  final bool enabled;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final bg = active ? AudioConsoleColors.tealDim : AudioConsoleColors.surface;
    final border = active
        ? const Color.fromRGBO(0, 212, 170, 0.4)
        : AudioConsoleColors.border;
    final textColor =
        active ? AudioConsoleColors.teal : AudioConsoleColors.text;
    final opacity = enabled ? 1.0 : 0.5;
    return Opacity(
      key: Key('mode_button_opacity_${item.id}'),
      opacity: opacity,
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          borderRadius: AudioConsoleRadius.button,
          onTap: enabled ? onTap : null,
          child: AnimatedContainer(
            key: Key('mode_button_container_${item.id}'),
            duration: AudioConsoleMotion.stateTransition,
            padding: const EdgeInsets.symmetric(
              horizontal: AudioConsoleSpacing.xs,
              vertical: AudioConsoleSpacing.xs,
            ),
            decoration: BoxDecoration(
              color: bg,
              borderRadius: AudioConsoleRadius.button,
              border: Border.all(color: border),
            ),
            child: Column(
              children: [
                Text(
                  item.name,
                  style: AudioConsoleType.caption(color: textColor).copyWith(
                    fontSize: 11,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  item.desc,
                  style: AudioConsoleType.caption(),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
