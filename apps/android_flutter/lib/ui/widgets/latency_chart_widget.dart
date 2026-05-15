import 'dart:async';

import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

/// A real-time area chart that compares the baseline (pre-optimization) latency
/// against the current measured latency. Refreshes every 500ms with a rolling
/// 60-second window.
///
/// Design language matches the Audio Console Dark theme:
/// - Background: AudioConsoleColors.bg (#0c0f14)
/// - Baseline curve: blue (#3b82f6) with downward gradient fill
/// - Current curve: teal (#00d4aa) with downward gradient fill
/// - Grid lines: subtle white at 6% alpha
class LatencyChartWidget extends StatefulWidget {
  const LatencyChartWidget({
    super.key,
    required this.currentLatencyMs,
    required this.baselineLatencyMs,
    required this.isZh,
    this.refreshInterval = const Duration(milliseconds: 500),
    this.windowSeconds = 60,
    this.height = 120,
  });

  /// Live current latency snapshot (in ms). The widget samples this every
  /// [refreshInterval] and pushes into its internal ring buffer.
  final ValueGetter<double?> currentLatencyMs;

  /// Baseline latency snapshot (in ms). Returning null means "use the previous
  /// sample" so a static reference line can be drawn by repeatedly returning
  /// the same value.
  final ValueGetter<double?> baselineLatencyMs;

  final bool isZh;
  final Duration refreshInterval;
  final int windowSeconds;
  final double height;

  @override
  State<LatencyChartWidget> createState() => _LatencyChartWidgetState();
}

class _LatencyChartWidgetState extends State<LatencyChartWidget> {
  late final int _maxSamples;
  late final List<double> _baseline;
  late final List<double> _current;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _maxSamples = (widget.windowSeconds * 1000) ~/
        widget.refreshInterval.inMilliseconds.clamp(1, 5000);
    _baseline = <double>[];
    _current = <double>[];
    _timer = Timer.periodic(widget.refreshInterval, (_) => _sample());
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  void _sample() {
    if (!mounted) return;
    final cur = widget.currentLatencyMs();
    final base = widget.baselineLatencyMs();
    setState(() {
      _push(_current, cur);
      _push(_baseline, base);
    });
  }

  void _push(List<double> list, double? value) {
    final v = value ?? (list.isNotEmpty ? list.last : 0.0);
    list.add(v.isFinite ? v : 0.0);
    while (list.length > _maxSamples) {
      list.removeAt(0);
    }
  }

  String tr(String zh, String en) => widget.isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: widget.height,
      decoration: BoxDecoration(
        color: AudioConsoleColors.bg2,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: AudioConsoleColors.border),
      ),
      padding: const EdgeInsets.fromLTRB(8, 10, 12, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Flexible(
                child: Text(
                  tr('延迟对比', 'Latency Comparison'),
                  style: AudioConsoleType.caption(
                    color: AudioConsoleColors.text2,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(width: 8),
              Flexible(
                child: Text(
                  tr('60s · 0.5s', '60s · 0.5s'),
                  style: AudioConsoleType.caption(
                    color: AudioConsoleColors.text3,
                  ),
                  overflow: TextOverflow.ellipsis,
                  textAlign: TextAlign.right,
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Expanded(
            child: CustomPaint(
              painter: _LatencyAreaPainter(
                baseline: List<double>.unmodifiable(_baseline),
                current: List<double>.unmodifiable(_current),
                maxSamples: _maxSamples,
              ),
              size: Size.infinite,
            ),
          ),
          const SizedBox(height: 6),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              _LegendDot(
                color: const Color(0xFF3B82F6),
                label: tr('优化前', 'Before'),
              ),
              const SizedBox(width: 16),
              _LegendDot(
                color: AudioConsoleColors.teal,
                label: tr('当前', 'Current'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _LegendDot extends StatelessWidget {
  const _LegendDot({required this.color, required this.label});

  final Color color;
  final String label;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 8,
          height: 8,
          decoration: BoxDecoration(
            color: color,
            shape: BoxShape.circle,
            boxShadow: [
              BoxShadow(
                color: color.withValues(alpha: 0.5),
                blurRadius: 4,
              ),
            ],
          ),
        ),
        const SizedBox(width: 6),
        Text(
          label,
          style: AudioConsoleType.caption(color: AudioConsoleColors.text2),
        ),
      ],
    );
  }
}

class _LatencyAreaPainter extends CustomPainter {
  _LatencyAreaPainter({
    required this.baseline,
    required this.current,
    required this.maxSamples,
  });

  final List<double> baseline;
  final List<double> current;
  final int maxSamples;

  static const Color baselineColor = Color(0xFF3B82F6);
  static const Color currentColor = AudioConsoleColors.teal;
  static const Color gridColor = Color.fromRGBO(255, 255, 255, 0.06);
  static const Color axisLabelColor = AudioConsoleColors.text3;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;

    // Reserve left strip for Y-axis labels.
    const double yLabelWidth = 32;
    final plotRect = Rect.fromLTWH(
      yLabelWidth,
      0,
      size.width - yLabelWidth,
      size.height,
    );

    // Compute Y range with auto-scale and 10% headroom.
    double minVal = 0;
    double maxVal = 100;
    if (baseline.isNotEmpty || current.isNotEmpty) {
      double mx = 0;
      for (final v in baseline) {
        if (v > mx) mx = v;
      }
      for (final v in current) {
        if (v > mx) mx = v;
      }
      maxVal = (mx * 1.15).clamp(50.0, 1000.0);
    }

    _drawGrid(canvas, plotRect, minVal, maxVal);
    if (baseline.isNotEmpty) {
      _drawSeries(canvas, plotRect, baseline, baselineColor, minVal, maxVal);
    }
    if (current.isNotEmpty) {
      _drawSeries(canvas, plotRect, current, currentColor, minVal, maxVal);
    }
  }

  void _drawGrid(Canvas canvas, Rect rect, double minVal, double maxVal) {
    final paint = Paint()
      ..color = gridColor
      ..strokeWidth = 1;

    const int gridLines = 4;
    final textStyle = TextStyle(
      color: axisLabelColor,
      fontSize: 9,
      fontFamily: AudioConsoleType.monoFamily,
    );
    for (int i = 0; i <= gridLines; i++) {
      final t = i / gridLines;
      final y = rect.top + rect.height * (1 - t);
      canvas.drawLine(
        Offset(rect.left, y),
        Offset(rect.right, y),
        paint,
      );
      final value = minVal + (maxVal - minVal) * t;
      final tp = TextPainter(
        text: TextSpan(text: '${value.round()}', style: textStyle),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(
        canvas,
        Offset(rect.left - tp.width - 4, y - tp.height / 2),
      );
    }
  }

  void _drawSeries(
    Canvas canvas,
    Rect rect,
    List<double> values,
    Color color,
    double minVal,
    double maxVal,
  ) {
    if (values.isEmpty) return;

    // Map: oldest sample is on the left, newest on the right.
    // If we have fewer samples than maxSamples, anchor the series to the right
    // so it appears to "scroll in" from the left.
    final n = values.length;
    final stride = rect.width / (maxSamples - 1).clamp(1, 1 << 30);
    final startX = rect.right - stride * (n - 1);

    final path = Path();
    final fillPath = Path();

    for (int i = 0; i < n; i++) {
      final x = startX + stride * i;
      final v = values[i].clamp(minVal, maxVal);
      final t = (v - minVal) / (maxVal - minVal).clamp(0.0001, double.infinity);
      final y = rect.top + rect.height * (1 - t);
      if (i == 0) {
        path.moveTo(x, y);
        fillPath.moveTo(x, rect.bottom);
        fillPath.lineTo(x, y);
      } else {
        path.lineTo(x, y);
        fillPath.lineTo(x, y);
      }
    }
    final lastX = startX + stride * (n - 1);
    fillPath.lineTo(lastX, rect.bottom);
    fillPath.close();

    final fillPaint = Paint()
      ..shader = LinearGradient(
        begin: Alignment.topCenter,
        end: Alignment.bottomCenter,
        colors: [
          color.withValues(alpha: 0.35),
          color.withValues(alpha: 0.02),
        ],
      ).createShader(rect);
    canvas.save();
    canvas.clipRect(rect);
    canvas.drawPath(fillPath, fillPaint);

    final strokePaint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.6
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..isAntiAlias = true;
    canvas.drawPath(path, strokePaint);
    canvas.restore();
  }

  @override
  bool shouldRepaint(covariant _LatencyAreaPainter oldDelegate) {
    return oldDelegate.baseline != baseline || oldDelegate.current != current;
  }
}
