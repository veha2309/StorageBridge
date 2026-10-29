import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../surfaces/liquid_glass_surface.dart';

/// Soft fades suit floating controls; hard edges suit pinned toolbar content.
enum LiquidGlassScrollEdgeStyle { soft, hard }

/// Paints adaptive edge separation above a vertical scrolling viewport.
///
/// Place the scroll view here and floating controls above it in the enclosing
/// Stack or Scaffold. The edge paint never intercepts input or semantics. It
/// tracks the nearest scroll view's metrics, including reverse scrolling and
/// content resizing, and ignores nested or horizontal scrolling notifications.
/// No extra blur, shader mask, animation ticker or scroll-time build is needed.
class LiquidGlassScrollEdge extends StatefulWidget {
  const LiquidGlassScrollEdge({
    super.key,
    required this.child,
    this.topExtent = 48,
    this.bottomExtent = 96,
    this.transitionDistance = 32,
    this.opacity = 0.72,
    this.color,
    this.style = LiquidGlassScrollEdgeStyle.soft,
  }) : assert(topExtent >= 0 && topExtent < double.infinity),
       assert(bottomExtent >= 0 && bottomExtent < double.infinity),
       assert(transitionDistance > 0 && transitionDistance < double.infinity),
       assert(opacity >= 0 && opacity <= 1);

  final Widget child;
  final double topExtent;
  final double bottomExtent;

  /// Scroll distance over which an edge becomes fully visible. Edges disappear
  /// at their corresponding content boundary and when the content fits.
  final double transitionDistance;
  final double opacity;
  final Color? color;
  final LiquidGlassScrollEdgeStyle style;

  @override
  State<LiquidGlassScrollEdge> createState() => _LiquidGlassScrollEdgeState();
}

class _LiquidGlassScrollEdgeState extends State<LiquidGlassScrollEdge> {
  final _edges = ValueNotifier((top: 0.0, bottom: 0.0));
  ScrollMetrics? _metrics;

  void _update(ScrollMetrics metrics) {
    if (metrics.axis != Axis.vertical) return;
    _metrics = metrics;
    final reversed = metrics.axisDirection == AxisDirection.up;
    double strength(double distance) =>
        (distance / widget.transitionDistance).clamp(0.0, 1.0);
    _edges.value = (
      top: strength(reversed ? metrics.extentAfter : metrics.extentBefore),
      bottom: strength(reversed ? metrics.extentBefore : metrics.extentAfter),
    );
  }

  @override
  void didUpdateWidget(LiquidGlassScrollEdge oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.child.runtimeType != widget.child.runtimeType ||
        oldWidget.child.key != widget.child.key) {
      // A replaced scrollable will publish its own initial metrics. Avoid
      // carrying an old list's edge state into a different viewport.
      _metrics = null;
      _edges.value = (top: 0.0, bottom: 0.0);
    } else if (oldWidget.transitionDistance != widget.transitionDistance &&
        _metrics != null) {
      _update(_metrics!);
    }
  }

  @override
  void dispose() {
    _edges.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accessible =
        MediaQuery.highContrastOf(context) ||
        LiquidGlassTheme.of(context).reduceTransparency;
    return Stack(
      children: [
        NotificationListener<ScrollMetricsNotification>(
          onNotification: (notification) {
            if (notification.depth == 0) _update(notification.metrics);
            return false;
          },
          child: NotificationListener<ScrollNotification>(
            onNotification: (notification) {
              if (notification.depth == 0) _update(notification.metrics);
              return false;
            },
            child: widget.child,
          ),
        ),
        Positioned.fill(
          child: IgnorePointer(
            child: ExcludeSemantics(
              child: RepaintBoundary(
                child: CustomPaint(
                  painter: _ScrollEdgePainter(
                    edges: _edges,
                    topExtent: widget.topExtent,
                    bottomExtent: widget.bottomExtent,
                    opacity: widget.opacity == 0
                        ? 0
                        : (accessible ? 1 : widget.opacity),
                    color:
                        widget.color ?? Theme.of(context).colorScheme.surface,
                    style: widget.style,
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _ScrollEdgePainter extends CustomPainter {
  _ScrollEdgePainter({
    required this.edges,
    required this.topExtent,
    required this.bottomExtent,
    required this.opacity,
    required this.color,
    required this.style,
  }) : super(repaint: edges);

  final ValueNotifier<({double top, double bottom})> edges;
  final double topExtent;
  final double bottomExtent;
  final double opacity;
  final Color color;
  final LiquidGlassScrollEdgeStyle style;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || opacity == 0) return;
    void edge(double extent, double strength, bool top) {
      final height = math.min(extent, size.height / 2);
      if (height == 0 || strength == 0) return;
      final rect = Rect.fromLTWH(
        0,
        top ? 0 : size.height - height,
        size.width,
        height,
      );
      final tint = color.withValues(alpha: color.a * opacity * strength);
      final paint = Paint();
      if (style == LiquidGlassScrollEdgeStyle.hard) {
        paint.color = tint;
      } else {
        paint.shader = LinearGradient(
          begin: top ? Alignment.topCenter : Alignment.bottomCenter,
          end: top ? Alignment.bottomCenter : Alignment.topCenter,
          colors: [
            tint,
            tint.withValues(alpha: tint.a * 0.65),
            tint.withValues(alpha: 0),
          ],
          stops: const [0, 0.35, 1],
        ).createShader(rect);
      }
      canvas.drawRect(rect, paint);
    }

    edge(topExtent, edges.value.top, true);
    edge(bottomExtent, edges.value.bottom, false);
  }

  @override
  bool shouldRepaint(_ScrollEdgePainter oldDelegate) =>
      edges != oldDelegate.edges ||
      topExtent != oldDelegate.topExtent ||
      bottomExtent != oldDelegate.bottomExtent ||
      opacity != oldDelegate.opacity ||
      color != oldDelegate.color ||
      style != oldDelegate.style;
}
