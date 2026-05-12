import 'dart:math';
import 'package:flutter/material.dart';

class JitterGraphWidget extends StatelessWidget {
  final List<int> jitterUs; // circular buffer of jitter values in microseconds, up to 120 samples
  final int p50Us;
  final int p95Us;
  final int underrunCount;

  const JitterGraphWidget({
    super.key,
    required this.jitterUs,
    required this.p50Us,
    required this.p95Us,
    required this.underrunCount,
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 280,
      height: 60,
      child: RepaintBoundary(
        child: CustomPaint(
          painter: _JitterSparklinePainter(
            jitterUs: jitterUs,
            p50Us: p50Us,
            p95Us: p95Us,
            underrunCount: underrunCount,
          ),
          size: const Size(280, 60),
        ),
      ),
    );
  }
}

class _JitterSparklinePainter extends CustomPainter {
  final List<int> jitterUs;
  final int p50Us;
  final int p95Us;
  final int underrunCount;

  _JitterSparklinePainter({
    required this.jitterUs,
    required this.p50Us,
    required this.p95Us,
    required this.underrunCount,
  });

  @override
  void paint(Canvas canvas, Size size) {
    // Background fill
    canvas.drawRect(
      Rect.fromLTWH(0, 0, size.width, size.height),
      Paint()..color = const Color(0xFF0A0E1A),
    );

    // Layout constants — chart area inset from top for readout row
    const double readoutTop = 14.0;
    const double maxJitterUs = 100000.0;
    final double chartHeight = size.height - readoutTop;

    // --- Readout row at top ---
    final String p50Ms = (p50Us / 1000).toStringAsFixed(0);
    final String p95Ms = (p95Us / 1000).toStringAsFixed(0);
    final readoutText = 'p50: ${p50Ms}ms  p95: ${p95Ms}ms  '
        '欠载: $underrunCount';

    final readoutPainter = TextPainter(
      text: TextSpan(
        text: readoutText,
        style: const TextStyle(
          fontFamily: 'IBM Plex Mono',
          fontSize: 11,
          color: Color(0x99FFFFFF), // 60 % opacity white
        ),
      ),
      textDirection: TextDirection.ltr,
    );
    readoutPainter.layout();
    readoutPainter.paint(canvas, const Offset(4, 4));

    // --- Dashed reference lines ---
    final dashPaint = Paint()
      ..color = const Color(0x33FFFFFF) // 20 % opacity white
      ..strokeWidth = 1.0;

    const List<int> refValuesUs = [30000, 60000, 100000];
    const List<String> refLabels = ['30ms', '60ms', '100ms'];

    for (int i = 0; i < refValuesUs.length; i++) {
      final double y = _yForValue(refValuesUs[i], readoutTop, chartHeight, maxJitterUs);
      _drawDashedHorizontalLine(canvas, y, size.width, dashPaint);

      // Reference line label
      final labelPainter = TextPainter(
        text: TextSpan(
          text: refLabels[i],
          style: const TextStyle(
            fontFamily: 'IBM Plex Mono',
            fontSize: 8,
            color: Color(0x66FFFFFF), // 40 % opacity white
          ),
        ),
        textDirection: TextDirection.ltr,
      );
      labelPainter.layout();
      labelPainter.paint(canvas, Offset(4, y + 9));
    }

    // --- "2.4s window" label at bottom-right ---
    final windowPainter = TextPainter(
      text: const TextSpan(
        text: '2.4s window',
        style: TextStyle(
          fontFamily: 'IBM Plex Mono',
          fontSize: 8,
          color: Color(0x66FFFFFF), // 40 % opacity white
        ),
      ),
      textDirection: TextDirection.ltr,
    );
    windowPainter.layout();
    windowPainter.paint(
      canvas,
      Offset(
        size.width - windowPainter.width - 4,
        size.height - windowPainter.height - 2,
      ),
    );

    // --- Sparkline path ---
    if (jitterUs.isEmpty) return;

    final int pointCount = jitterUs.length;
    if (pointCount < 2) return;

    final double stepX = size.width / (pointCount - 1);

    final Paint segmentPaint = Paint()
      ..strokeWidth = 1.5
      ..strokeCap = StrokeCap.round
      ..style = PaintingStyle.stroke;

    for (int i = 0; i < pointCount - 1; i++) {
      final int value = jitterUs[i].clamp(0, 120000);
      final int nextValue = jitterUs[i + 1].clamp(0, 120000);

      // Per-segment color based on the first point's jitter value
      if (value <= 30000) {
        segmentPaint.color = const Color(0xFF00D4AA); // teal
      } else if (value <= 80000) {
        segmentPaint.color = const Color(0xFFFFB300); // amber
      } else {
        segmentPaint.color = const Color(0xFFFF4444); // red
      }

      final double x1 = i * stepX;
      final double y1 = _yForValue(value, readoutTop, chartHeight, maxJitterUs);
      final double x2 = (i + 1) * stepX;
      final double y2 = _yForValue(nextValue, readoutTop, chartHeight, maxJitterUs);

      canvas.drawLine(Offset(x1, y1), Offset(x2, y2), segmentPaint);
    }
  }

  /// Map a jitter value in microseconds to a canvas y coordinate.
  /// 100 000 us (100 ms) maps to [chartTop]; 0 us maps to the bottom.
  double _yForValue(int valueUs, double chartTop, double chartHeight, double maxJitterUs) {
    final double fraction = valueUs.clamp(0, maxJitterUs.toInt()).toDouble() / maxJitterUs;
    return chartTop + (1.0 - fraction) * chartHeight;
  }

  /// Draw a dashed horizontal line from x=0 to [width] at the given y.
  void _drawDashedHorizontalLine(Canvas canvas, double y, double width, Paint paint) {
    const double dashWidth = 4.0;
    const double dashGap = 4.0;
    double x = 0.0;
    while (x < width) {
      final double endX = min(x + dashWidth, width);
      canvas.drawLine(Offset(x, y), Offset(endX, y), paint);
      x = endX + dashGap;
    }
  }

  @override
  bool shouldRepaint(covariant _JitterSparklinePainter oldDelegate) => true;
}
