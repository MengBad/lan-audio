import 'dart:async';
import 'package:flutter/material.dart';
import '../services/mic_capture_service.dart';

class MicStatusWidget extends StatefulWidget {
  final MicCaptureService service;
  final String? host;
  final int reversePort;
  final bool enabled;
  final Future<void> Function() onToggle;

  const MicStatusWidget({
    super.key,
    required this.service,
    this.host,
    this.reversePort = 7878,
    required this.enabled,
    required this.onToggle,
  });

  @override
  State<MicStatusWidget> createState() => _MicStatusWidgetState();
}

class _MicStatusWidgetState extends State<MicStatusWidget>
    with SingleTickerProviderStateMixin {
  Timer? _levelTimer;
  double _amplitude = 0.0;
  late AnimationController _pulseController;

  @override
  void initState() {
    super.initState();
    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 800),
    );
    if (widget.service.status == MicStatus.streaming) {
      _startLevelPolling();
      _pulseController.repeat(reverse: true);
    }
  }

  @override
  void didUpdateWidget(MicStatusWidget old) {
    super.didUpdateWidget(old);
    if (widget.service.status == MicStatus.streaming && _levelTimer == null) {
      _startLevelPolling();
      _pulseController.repeat(reverse: true);
    } else if (widget.service.status != MicStatus.streaming &&
        _levelTimer != null) {
      _stopLevelPolling();
      _pulseController.stop();
      _pulseController.reset();
    }
  }

  void _startLevelPolling() {
    _levelTimer = Timer.periodic(const Duration(milliseconds: 100), (_) async {
      if (!mounted) return;
      await widget.service.getLevel();
      if (!mounted) return;
      setState(() {
        _amplitude =
            ((widget.service.rmsDb + 96.0).clamp(0.0, 96.0) / 96.0);
      });
    });
  }

  void _stopLevelPolling() {
    _levelTimer?.cancel();
    _levelTimer = null;
  }

  @override
  void dispose() {
    _stopLevelPolling();
    _pulseController.dispose();
    super.dispose();
  }

  Color _statusColor() {
    switch (widget.service.status) {
      case MicStatus.streaming:
        return const Color(0xFFFF4444);
      case MicStatus.connecting:
        return const Color(0xFFFFB300);
      case MicStatus.error:
        return const Color(0xFFFF4444);
      case MicStatus.off:
        return Colors.grey;
    }
  }

  @override
  Widget build(BuildContext context) {
    final isStreaming = widget.service.status == MicStatus.streaming;
    final statusColor = _statusColor();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Divider(),
        Row(
          children: [
            AnimatedBuilder(
              animation: _pulseController,
              builder: (context, child) {
                final scale =
                    isStreaming ? 1.0 + _pulseController.value * 0.3 : 1.0;
                return Transform.scale(
                  scale: scale,
                  child: Container(
                    width: 10,
                    height: 10,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: statusColor,
                    ),
                  ),
                );
              },
            ),
            const SizedBox(width: 6),
            Text(
              'Mic',
              style: TextStyle(
                fontWeight: FontWeight.w600,
                fontSize: 14,
                color: Theme.of(context).colorScheme.onSurface,
              ),
            ),
            if (isStreaming) ...[
              const SizedBox(width: 4),
              Text(
                '${widget.service.peakDb.toStringAsFixed(0)} dB',
                style: TextStyle(
                  fontSize: 11,
                  fontFamily: 'monospace',
                  color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.6),
                ),
              ),
            ],
            const Spacer(),
            SizedBox(
              height: 28,
              child: Switch(
                value: widget.enabled,
                onChanged: (_) async {
                  await widget.onToggle();
                },
                activeThumbColor: const Color(0xFFFF4444),
              ),
            ),
          ],
        ),
        if (isStreaming) ...[
          const SizedBox(height: 6),
          ClipRRect(
            borderRadius: BorderRadius.circular(3),
            child: SizedBox(
              height: 4,
              child: Row(
                children: List.generate(12, (i) {
                  final threshold = (i + 1) / 12.0;
                  final active = _amplitude >= threshold;
                  return Expanded(
                    child: Container(
                      margin: const EdgeInsets.symmetric(horizontal: 1),
                      decoration: BoxDecoration(
                        color: active
                            ? const Color(0xFF00D4AA)
                            : const Color(0xFF1A2035).withValues(alpha: 0.3),
                        borderRadius: BorderRadius.circular(1),
                      ),
                    ),
                  );
                }),
              ),
            ),
          ),
          const SizedBox(height: 2),
          Text(
            'Streaming to PC',
            style: TextStyle(
              color: const Color(0xFF00D4AA).withValues(alpha: 0.8),
              fontSize: 10,
            ),
          ),
        ],
        if (widget.service.status == MicStatus.connecting)
          const Padding(
            padding: EdgeInsets.only(top: 2),
            child: Text(
              'Connecting...',
              style: TextStyle(color: Color(0xFFFFB300), fontSize: 10),
            ),
          ),
        if (widget.service.status == MicStatus.error)
          Padding(
            padding: const EdgeInsets.only(top: 2),
            child: Text(
              'Error: ${widget.service.errorMessage}',
              style: const TextStyle(color: Color(0xFFFF4444), fontSize: 10),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
          ),
      ],
    );
  }
}
