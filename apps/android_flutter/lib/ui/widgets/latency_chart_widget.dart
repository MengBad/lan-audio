import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../audio_console_theme.dart';

/// A real-time area chart showing current latency with a dashed baseline
/// reference line. Refreshes every 500ms with a rolling 60-second window.
///
/// Scheme A optimizations:
/// - Baseline rendered as a dashed horizontal line (not area fill)
/// - Only current curve uses gradient area fill
/// - Real-time value displayed in top-right corner
/// - Y-axis shows only top/bottom labels to save horizontal space
/// - Smooth Catmull-Rom interpolation for the current curve
class LatencyChartWidget extends StatefulWidget {
  const LatencyChartWidget({
    super.key,
    required this.currentLatencyMs,
    required this.baselineLatencyMs,
    required this.isZh,
    this.refreshInterval = const Duration(milliseconds: 500),
    this.windowSeconds = 60,
    this.height = 100,
  });

  /// Live current latency snapshot (in ms).
  final ValueGetter<double?> currentLatencyMs;

  /// Baseline latency snapshot (in ms).
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
  late final List<double> _current;
  double _lastBaseline = 0;
  double _lastCurrent = 0;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _maxSamples = (widget.windowSeconds * 1000) ~/
        widget.refreshInterval.inMilliseconds.clamp(1, 5000);
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
      if (base != null && base.isFinite) _lastBaseline = base;
      final v = cur ?? (_current.isNotEmpty ? _current.last : 0.0);
      _lastCurrent = v.isFinite ? v : 0.0;
      _current.add(_lastCurrent);
      while (_current.length > _maxSamples) {
        _current.removeAt(0);
      }
    });
  }

  String tr(String zh, String en) => widget.isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    final currentText = _lastCurrent > 0
        ? '${_lastCurrent.round()}ms'
        : '--';

    return Container(
      height: widget.height,
      decoration: BoxDecoration(
        color: AudioConsoleColors.bg2,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: AudioConsoleColors.border),
      ),
      padding: const EdgeInsets.fromLTRB(6, 8, 10, 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    tr('延迟', 'Latency'),
                    style: AudioConsoleType.caption(
                      color: AudioConsoleColors.text2,
                    ),
                  ),
                  const SizedBox(width: 10),
                  _LegendDot(
                    color: AudioConsoleColors.teal,
                    label: tr('当前', 'Now'),
                  ),
                  const SizedBox(width: 10),
                  _LegendDot(
                    color: const Color(0xFF60A5FA),
                    label: tr('基线', 'Base'),
                    dashed: true,
                  ),
                ],
              ),
              Text(
                currentText,
                style: TextStyle(
                  fontFamily: AudioConsoleType.monoFamily,
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                  color: AudioConsoleColors.teal,
                ),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Expanded(
            child: CustomPaint(
              painter: _LatencyAreaPainter(
                current: List<double>.unmodifiable(_current),
                baseline: _lastBaseline,
                maxSamples: _maxSamples,
              ),
              size: Size.infinite,
            ),
          ),
        ],
      ),
    );
  }
}

class _LegendDot extends StatelessWidget {
  const _LegendDot({
    required this.color,
    required this.label,
    this.dashed = false,
  });

  final Color color;
  final String label;
  final bool dashed;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (dashed)
          CustomPaint(
            size: const Size(10, 2),
            painter: _DashLinePainter(color: color),
          )
        else
          Container(
            width: 7,
            height: 7,
            decoration: BoxDecoration(
              color: color,
              shape: BoxShape.circle,
              boxShadow: [
                BoxShadow(
                  color: color.withValues(alpha: 0.5),
                  blurRadius: 3,
                ),
              ],
            ),
          ),
        const SizedBox(width: 4),
        Text(
          label,
          style: TextStyle(
            fontFamily: AudioConsoleType.monoFamily,
            fontSize: 10,
            color: AudioConsoleColors.text3,
          ),
        ),
      ],
    );
  }
}

class _DashLinePainter extends CustomPainter {
  _DashLinePainter({required this.color});
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.5
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(Offset(0, size.height / 2), Offset(3, size.height / 2), paint);
    canvas.drawLine(Offset(5.5, size.height / 2), Offset(size.width, size.height / 2), paint);
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

class _LatencyAreaPainter extends CustomPainter {
  _LatencyAreaPainter({
    required this.current,
    required this.baseline,
    required this.maxSamples,
  });

  final List<double> current;
  final double baseline;
  final int maxSamples;

  static const Color currentColor = AudioConsoleColors.teal;
  static const Color baselineColor = Color(0xFF60A5FA);
  static const Color gridColor = Color.fromRGBO(255, 255, 255, 0.05);
  static const Color axisLabelColor = AudioConsoleColors.text3;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;

    // Reserve minimal left strip for Y-axis labels (top + bottom only).
    const double yLabelWidth = 26;
    final plotRect = Rect.fromLTWH(
      yLabelWidth,
      2,
      size.width - yLabelWidth - 2,
      size.height - 4,
    );

    // Compute Y range.
    double maxVal = 100;
    double mx = baseline;
    for (final v in current) {
      if (v > mx) mx = v;
    }
    maxVal = (mx * 1.2).clamp(50.0, 1000.0);

    _drawGrid(canvas, plotRect, maxVal);
    _drawBaseline(canvas, plotRect, maxVal);
    if (current.isNotEmpty) {
      _drawCurrentSeries(canvas, plotRect, maxVal);
    }
  }

  void _drawGrid(Canvas canvas, Rect rect, double maxVal) {
    final paint = Paint()
      ..color = gridColor
      ..strokeWidth = 0.5;

    // Draw 3 subtle grid lines (no labels on middle ones).
    const int gridLines = 3;
    final textStyle = TextStyle(
      color: axisLabelColor,
      fontSize: 8,
      fontFamily: AudioConsoleType.monoFamily,
    );

    for (int i = 0; i <= gridLines; i++) {
      final t = i / gridLines;
      final y = rect.top + rect.height * (1 - t);
      canvas.drawLine(Offset(rect.left, y), Offset(rect.right, y), paint);

      // Only label top and bottom.
      if (i == 0 || i == gridLines) {
        final value = maxVal * t;
        final tp = TextPainter(
          text: TextSpan(text: '${value.round()}', style: textStyle),
          textDirection: TextDirection.ltr,
        )..layout();
        tp.paint(canvas, Offset(rect.left - tp.width - 3, y - tp.height / 2));
      }
    }
  }

  void _drawBaseline(Canvas canvas, Rect rect, double maxVal) {
    if (baseline <= 0) return;

    final t = (baseline / maxVal).clamp(0.0, 1.0);
    final y = rect.top + rect.height * (1 - t);

    // Dashed line.
    final paint = Paint()
      ..color = baselineColor.withValues(alpha: 0.7)
      ..strokeWidth = 1.2
      ..strokeCap = StrokeCap.round;

    const double dashWidth = 5;
    const double dashGap = 3;
    double x = rect.left;
    while (x < rect.right) {
      final end = (x + dashWidth).clamp(rect.left, rect.right);
      canvas.drawLine(Offset(x, y), Offset(end, y), paint);
      x += dashWidth + dashGap;
    }

    // Label on the right side.
    final textStyle = TextStyle(
      color: baselineColor.withValues(alpha: 0.8),
      fontSize: 9,
      fontFamily: AudioConsoleType.monoFamily,
    );
    final tp = TextPainter(
      text: TextSpan(text: '${baseline.round()}', style: textStyle),
      textDirection: TextDirection.ltr,
    )..layout();
    tp.paint(canvas, Offset(rect.right - tp.width, y - tp.height - 2));
  }

  void _drawCurrentSeries(Canvas canvas, Rect rect, double maxVal) {
    if (current.isEmpty) return;

    final n = current.length;
    final stride = rect.width / (maxSamples - 1).clamp(1, 1 << 30);
    final startX = rect.right - stride * (n - 1);

    // Build points list.
    final points = <Offset>[];
    for (int i = 0; i < n; i++) {
      final x = startX + stride * i;
      final v = current[i].clamp(0.0, maxVal);
      final t = v / maxVal.clamp(0.0001, double.infinity);
      final y = rect.top + rect.height * (1 - t);
      points.add(Offset(x, y));
    }

    // Build smooth path using Catmull-Rom spline.
    final path = _catmullRomPath(points);
    final fillPath = Path.from(path);
    fillPath.lineTo(points.last.dx, rect.bottom);
    fillPath.lineTo(points.first.dx, rect.bottom);
    fillPath.close();

    // Gradient fill.
    final fillPaint = Paint()
      ..shader = ui.Gradient.linear(
        Offset(0, rect.top),
        Offset(0, rect.bottom),
        [
          currentColor.withValues(alpha: 0.30),
          currentColor.withValues(alpha: 0.02),
        ],
      );
    canvas.save();
    canvas.clipRect(rect);
    canvas.drawPath(fillPath, fillPaint);

    // Stroke.
    final strokePaint = Paint()
      ..color = currentColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..isAntiAlias = true;
    canvas.drawPath(path, strokePaint);
    canvas.restore();

    // Draw a small dot at the latest point.
    if (points.isNotEmpty) {
      final last = points.last;
      canvas.drawCircle(
        last,
        2.5,
        Paint()..color = currentColor,
      );
      canvas.drawCircle(
        last,
        2.5,
        Paint()
          ..color = currentColor.withValues(alpha: 0.3)
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
      );
    }
  }

  /// Attempt Catmull-Rom interpolation; falls back to straight lines if < 3 points.
  Path _catmullRomPath(List<Offset> points) {
    final path = Path();
    if (points.isEmpty) return path;
    if (points.length < 3) {
      path.moveTo(points.first.dx, points.first.dy);
      for (int i = 1; i < points.length; i++) {
        path.lineTo(points[i].dx, points[i].dy);
      }
      return path;
    }

    path.moveTo(points.first.dx, points.first.dy);
    for (int i = 0; i < points.length - 1; i++) {
      final p0 = i > 0 ? points[i - 1] : points[i];
      final p1 = points[i];
      final p2 = points[i + 1];
      final p3 = i + 2 < points.length ? points[i + 2] : points[i + 1];

      // Catmull-Rom to cubic bezier conversion (tension = 0.5).
      final cp1 = Offset(
        p1.dx + (p2.dx - p0.dx) / 6,
        p1.dy + (p2.dy - p0.dy) / 6,
      );
      final cp2 = Offset(
        p2.dx - (p3.dx - p1.dx) / 6,
        p2.dy - (p3.dy - p1.dy) / 6,
      );
      path.cubicTo(cp1.dx, cp1.dy, cp2.dx, cp2.dy, p2.dx, p2.dy);
    }
    return path;
  }

  @override
  bool shouldRepaint(covariant _LatencyAreaPainter oldDelegate) {
    return oldDelegate.current != current ||
        oldDelegate.baseline != baseline;
  }
}
