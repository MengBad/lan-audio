import 'package:flutter/material.dart';

import '../audio_console_status.dart';
import '../audio_console_theme.dart';

class HeroStatusWidget extends StatefulWidget {
  const HeroStatusWidget({
    required this.status,
    required this.meta,
    required this.isZh,
    super.key,
  });

  final ConsoleStatusViewData status;
  final String meta;
  final bool isZh;

  @override
  State<HeroStatusWidget> createState() => _HeroStatusWidgetState();
}

class _HeroStatusWidgetState extends State<HeroStatusWidget>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: AudioConsoleMotion.orbPulse,
  );

  @override
  void initState() {
    super.initState();
    _syncAnimation();
  }

  @override
  void didUpdateWidget(covariant HeroStatusWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.status.state != widget.status.state) {
      _syncAnimation();
    }
  }

  void _syncAnimation() {
    if (widget.status.state == ConsoleUiState.streaming) {
      _controller.repeat();
    } else {
      _controller.stop();
      _controller.value = 0;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  IconData _iconFor(ConsoleUiState state) {
    switch (state) {
      case ConsoleUiState.streaming:
        return Icons.graphic_eq_rounded;
      case ConsoleUiState.connecting:
        return Icons.wifi_tethering_rounded;
      case ConsoleUiState.buffering:
        return Icons.sync_rounded;
      case ConsoleUiState.error:
        return Icons.error_outline_rounded;
      case ConsoleUiState.idle:
        return Icons.sensors_rounded;
    }
  }

  @override
  Widget build(BuildContext context) {
    final isPulsing = widget.status.state == ConsoleUiState.streaming;
    return Column(
      key: const Key('hero_status'),
      children: [
        AnimatedBuilder(
          animation: _controller,
          builder: (context, child) {
            final shadowSpread = widget.status.state == ConsoleUiState.streaming
                ? (16 * _controller.value).toDouble()
                : 0.0;
            final shadowOpacity =
                widget.status.state == ConsoleUiState.streaming
                    ? ((1 - _controller.value) * 0.35).toDouble()
                    : 0.0;
            return Semantics(
              label: isPulsing ? 'orb-pulsing' : 'orb-static',
              child: AnimatedContainer(
                key: Key('hero_orb_${isPulsing ? 'pulsing' : 'static'}'),
                duration: AudioConsoleMotion.stateTransition,
                width: 104,
                height: 104,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: widget.status.background,
                  border: Border.all(
                    color: widget.status.border,
                    width: 1.5,
                  ),
                  boxShadow: shadowSpread <= 0
                      ? const []
                      : [
                          BoxShadow(
                            color: AudioConsoleColors.teal.withValues(
                              alpha: shadowOpacity,
                            ),
                            spreadRadius: shadowSpread,
                            blurRadius: 0,
                          ),
                        ],
                ),
                child: Icon(
                  _iconFor(widget.status.state),
                  size: 42,
                  color: widget.status.accent,
                ),
              ),
            );
          },
        ),
        const SizedBox(height: AudioConsoleSpacing.lg),
        AnimatedDefaultTextStyle(
          duration: AudioConsoleMotion.stateTransition,
          style: AudioConsoleType.title(),
          child: Text(
            widget.isZh ? widget.status.titleZh : widget.status.titleEn,
            key: const Key('hero_title'),
          ),
        ),
        const SizedBox(height: AudioConsoleSpacing.xxs),
        Text(
          widget.meta,
          key: const Key('hero_meta'),
          style: AudioConsoleType.monoMeta(),
          textAlign: TextAlign.center,
        ),
      ],
    );
  }
}
