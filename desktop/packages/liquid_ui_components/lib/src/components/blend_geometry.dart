part of 'liquid_glass_blend.dart';

class _PositionedBlendBorder extends OutlinedBorder {
  const _PositionedBlendBorder({
    required this.first,
    required this.second,
    required this.radius,
    required this.smoothness,
    super.side,
  });
  final Offset first;
  final Offset second;
  final double radius;
  final double smoothness;

  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(side.width);
  @override
  _PositionedBlendBorder copyWith({BorderSide? side}) => _PositionedBlendBorder(
    first: first,
    second: second,
    radius: radius,
    smoothness: smoothness,
    side: side ?? this.side,
  );
  @override
  ShapeBorder scale(double t) => _PositionedBlendBorder(
    first: first * t,
    second: second * t,
    radius: radius * t,
    smoothness: smoothness * t,
    side: side.scale(t),
  );
  Path _path(Rect rect, double inset) {
    final delta = second - first;
    final r = math.max(0.0, radius - inset);
    final local = _BlendedLenses(
      distance: delta.distance,
      radius: r,
      smoothness: smoothness,
    ).getOuterPath(Rect.fromLTWH(0, 0, delta.distance + r * 2, r * 2));
    final transform =
        Matrix4.translationValues(rect.left + first.dx, rect.top + first.dy, 0)
          ..rotateZ(math.atan2(delta.dy, delta.dx))
          ..translateByDouble(-r, -r, 0, 1);
    return local.transform(transform.storage);
  }

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      _path(rect, 0);
  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      _path(rect, side.width);
  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    if (side.style != BorderStyle.none && side.width > 0) {
      canvas.drawPath(_path(rect, side.width / 2), side.toPaint());
    }
  }
}

class _BlendedLenses extends OutlinedBorder {
  _BlendedLenses({
    required this.distance,
    required this.radius,
    required this.smoothness,
    super.side,
  });
  final double distance;
  final double radius;
  final double smoothness;
  final _paths = <Rect, Path>{};

  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(side.width);

  @override
  _BlendedLenses copyWith({BorderSide? side}) => _BlendedLenses(
    distance: distance,
    radius: radius,
    smoothness: smoothness,
    side: side ?? this.side,
  );

  @override
  ShapeBorder scale(double t) => _BlendedLenses(
    distance: distance * t,
    radius: radius * t,
    smoothness: smoothness * t,
    side: side.scale(t),
  );

  Path _outline(Rect rect) =>
      _paths.putIfAbsent(rect, () => _buildOutline(rect));

  Path _buildOutline(Rect rect) {
    final r = math.min(radius, rect.height / 2);
    if (r <= 0 || rect.isEmpty) return Path();
    final left = rect.left + r;
    final right = rect.right - r;
    final cy = rect.center.dy;
    final smoothing = smoothness;
    // A smooth union defines one continuous liquid field. Unlike a separate
    // bridge patch, its neck cannot cross itself or leave an isolated island.
    double field(double x, double y) {
      final a = math.sqrt((x - left) * (x - left) + y * y) - r;
      final b = math.sqrt((x - right) * (x - right) + y * y) - r;
      final h = math.max(smoothing - (a - b).abs(), 0) / smoothing;
      return math.min(a, b) - h * h * smoothing * 0.25;
    }

    final path = Path();
    final top = <Offset>[];
    final bottom = <Offset>[];
    void finish() {
      if (top.length > 1) {
        path.moveTo(top.first.dx, top.first.dy);
        for (final point in top.skip(1)) {
          path.lineTo(point.dx, point.dy);
        }
        for (final point in bottom.reversed) {
          path.lineTo(point.dx, point.dy);
        }
        path.close();
      }
      top.clear();
      bottom.clear();
    }

    // Solve the upper and lower field boundaries; symmetry makes the vertical
    // root monotonic. Separate runs naturally become two drops after rupture.
    const samples = 192;
    for (var i = 0; i <= samples; i++) {
      final x = rect.left + rect.width * i / samples;
      if (field(x, 0) > 0) {
        finish();
        continue;
      }
      var low = 0.0;
      var high = r * 1.3;
      for (var j = 0; j < 16; j++) {
        final mid = (low + high) / 2;
        if (field(x, mid) < 0) {
          low = mid;
        } else {
          high = mid;
        }
      }
      final y = (low + high) / 2;
      top.add(Offset(x, cy - y));
      bottom.add(Offset(x, cy + y));
    }
    finish();
    return path;
  }

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      _outline(rect);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      _outline(rect.deflate(side.width));

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    if (side.style != BorderStyle.none && side.width > 0) {
      canvas.drawPath(_outline(rect.deflate(side.width / 2)), side.toPaint());
    }
  }
}
