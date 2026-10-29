part of 'liquid_glass_morph.dart';

class _MorphSharedLayout extends SingleChildLayoutDelegate {
  const _MorphSharedLayout({
    required this.progress,
    required this.small,
    required this.large,
    required this.startAlignment,
    required this.endAlignment,
    required this.startPadding,
    required this.endPadding,
  });

  final double progress;
  final Size small;
  final Size large;
  final Alignment startAlignment;
  final Alignment endAlignment;
  final EdgeInsets startPadding;
  final EdgeInsets endPadding;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      BoxConstraints.loose(
        Size(
          math.max(
            0.0,
            math.min(
              small.width - startPadding.horizontal,
              large.width - endPadding.horizontal,
            ),
          ),
          math.max(
            0.0,
            math.min(
              small.height - startPadding.vertical,
              large.height - endPadding.vertical,
            ),
          ),
        ),
      );

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final start = startAlignment
        .inscribe(childSize, startPadding.deflateRect(Offset.zero & small))
        .topLeft;
    final end = endAlignment
        .inscribe(childSize, endPadding.deflateRect(Offset.zero & large))
        .topLeft;
    return Offset.lerp(start, end, progress)!;
  }

  @override
  bool shouldRelayout(_MorphSharedLayout oldDelegate) =>
      progress != oldDelegate.progress ||
      small != oldDelegate.small ||
      large != oldDelegate.large ||
      startAlignment != oldDelegate.startAlignment ||
      endAlignment != oldDelegate.endAlignment ||
      startPadding != oldDelegate.startPadding ||
      endPadding != oldDelegate.endPadding;
}

/// A continuous perimeter with a travelling waist and rounded leading lobe
/// as the source capsule spreads into the destination panel. All surface
/// layers use the same path; the children themselves remain undistorted.
class _FluidGlassBorder extends OutlinedBorder {
  _FluidGlassBorder({
    required this.borderRadius,
    required this.fullness,
    required this.direction,
    required this.motion,
    super.side,
    Map<Rect, Path>? paths,
  }) : _paths = paths ?? {};

  final BorderRadius borderRadius;
  final double fullness;
  final Alignment direction;
  final LiquidGlassMorphMotion motion;
  final Map<Rect, Path> _paths;
  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(side.width);

  @override
  _FluidGlassBorder copyWith({BorderSide? side}) => _FluidGlassBorder(
    borderRadius: borderRadius,
    fullness: fullness,
    direction: direction,
    motion: motion,
    side: side ?? this.side,
    paths: _paths,
  );

  @override
  ShapeBorder scale(double t) => _FluidGlassBorder(
    borderRadius: borderRadius * t,
    fullness: fullness,
    direction: direction,
    motion: motion,
    side: side.scale(t),
  );

  Path _path(Rect rect) => _paths.putIfAbsent(rect, () => _buildPath(rect));

  Path _buildPath(Rect rect) {
    if (rect.isEmpty) return Path();
    final rounded = borderRadius.toRRect(rect).scaleRadii();
    if (fullness < 0.00001) return Path()..addRRect(rounded);
    // Construct the perimeter from exact rounded-rectangle arcs. Uniform
    // angular samples plus cubic tangents overshoot on wide panels, producing
    // bulges even when the spring is almost settled. Corner-local deformation
    // remains convex, inside layout bounds, and converges continuously.
    final swell =
        rect.shortestSide *
        fullness *
        switch (motion) {
          LiquidGlassMorphMotion.droplet => 0.85,
          LiquidGlassMorphMotion.jelly => 0.22,
          LiquidGlassMorphMotion.merge => 0.9,
        };
    Radius corner(Radius radius, double weight) =>
        Radius.elliptical(radius.x + swell * weight, radius.y + swell * weight);
    final leadingRight = (1 - direction.x) / 2;
    final leadingBottom = (1 - direction.y) / 2;
    if (motion == LiquidGlassMorphMotion.jelly) {
      // Jelly keeps a broad, soft body; its character comes from squash and
      // the underdamped spring rather than a connecting neck.
      return Path()..addRRect(
        RRect.fromRectAndCorners(
          rect,
          topLeft: corner(rounded.tlRadius, 1),
          topRight: corner(rounded.trRadius, 1),
          bottomLeft: corner(rounded.blRadius, 1),
          bottomRight: corner(rounded.brRadius, 1),
        ).scaleRadii(),
      );
    }
    final contour = RRect.fromRectAndCorners(
      rect,
      topLeft: corner(rounded.tlRadius, 0.08),
      topRight: corner(rounded.trRadius, 0.15 + leadingRight * 0.75),
      bottomLeft: corner(rounded.blRadius, 0.15 + leadingBottom * 0.75),
      bottomRight: corner(
        rounded.brRadius,
        0.25 + (leadingRight + leadingBottom) * 0.65,
      ),
    ).scaleRadii();
    if (motion == LiquidGlassMorphMotion.droplet) {
      // One lobe: retain a small source corner while the far end balloons.
      // No waist here: a droplet must not read as two merging bubbles.
      return Path()..addRRect(contour);
    }
    // Give the body a moving waist, rather than four straight edges with
    // changing radii. Cubic controls stay inside the layout bounds and meet
    // the corner arcs tangentially; no angular resampling or overshoot.
    final neck = rect.shortestSide * fullness * 0.42;
    final path = Path();
    void edge(Offset start, Offset end, Offset inward) {
      final span = end - start;
      final middle = start + span * 0.5 + inward;
      path.cubicTo(
        start.dx + span.dx * .2,
        start.dy + span.dy * .2,
        middle.dx - span.dx * .16,
        middle.dy - span.dy * .16,
        middle.dx,
        middle.dy,
      );
      path.cubicTo(
        middle.dx + span.dx * .16,
        middle.dy + span.dy * .16,
        end.dx - span.dx * .2,
        end.dy - span.dy * .2,
        end.dx,
        end.dy,
      );
    }

    final l = contour.left,
        r = contour.right,
        top = contour.top,
        bottom = contour.bottom;
    final tl = contour.tlRadius, tr = contour.trRadius;
    final bl = contour.blRadius, br = contour.brRadius;
    path.moveTo(l + tl.x, top);
    edge(Offset(l + tl.x, top), Offset(r - tr.x, top), Offset(0, neck * .85));
    path.arcToPoint(Offset(r, top + tr.y), radius: tr);
    edge(
      Offset(r, top + tr.y),
      Offset(r, bottom - br.y),
      Offset(-neck * .35, 0),
    );
    path.arcToPoint(Offset(r - br.x, bottom), radius: br);
    edge(Offset(r - br.x, bottom), Offset(l + bl.x, bottom), Offset(0, -neck));
    path.arcToPoint(Offset(l, bottom - bl.y), radius: bl);
    edge(
      Offset(l, bottom - bl.y),
      Offset(l, top + tl.y),
      Offset(neck * .15, 0),
    );
    path.arcToPoint(Offset(l + tl.x, top), radius: tl);
    return path..close();
  }

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) => _path(rect);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      _path(rect.deflate(side.width));

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    if (side.style == BorderStyle.none || side.width == 0) return;
    final path = _path(rect.deflate(side.width / 2));
    canvas.drawPath(path, side.toPaint());
  }
}
