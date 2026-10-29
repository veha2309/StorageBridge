import 'package:flutter/material.dart';

/// Shared exterior shadow, painted after the glass has captured its backdrop.
class GlassShadow extends StatelessWidget {
  const GlassShadow({
    super.key,
    required this.shadow,
    required this.child,
    this.radius = BorderRadius.zero,
    this.shape,
    this.duration = Duration.zero,
  });

  final BoxShadow shadow;
  final Widget child;
  final BorderRadius radius;
  final OutlinedBorder? shape;
  final Duration duration;

  @override
  Widget build(BuildContext context) {
    final decoration = _GlassShadowDecoration(
      radius: radius,
      shape: shape,
      shadow: shadow,
    );
    // Explicitly animated geometry does not need an implicit controller and
    // decoration tween rebuilt on every frame.
    if (duration == Duration.zero) {
      return DecoratedBox(
        decoration: decoration,
        position: DecorationPosition.foreground,
        child: child,
      );
    }
    return AnimatedContainer(
      duration: duration,
      // Never put the shadow beneath the backdrop filter: even the exterior blur
      // can leak back into the glass near its rim.
      foregroundDecoration: decoration,
      child: child,
    );
  }
}

/// Paint only the exterior shadow. A normal BoxShadow fills the shape too:
/// grouped backdrop captures can then sample that fill when their first visible
/// member changes during scrolling, darkening every surface sharing the capture.
class _GlassShadowDecoration extends Decoration {
  const _GlassShadowDecoration({
    required this.radius,
    required this.shape,
    required this.shadow,
  });

  final BorderRadius radius;
  final OutlinedBorder? shape;
  final BoxShadow shadow;

  @override
  Decoration? lerpFrom(Decoration? a, double t) {
    if (a is _GlassShadowDecoration) {
      return _GlassShadowDecoration(
        radius: BorderRadius.lerp(a.radius, radius, t)!,
        shape: ShapeBorder.lerp(a.shape, shape, t) as OutlinedBorder?,
        shadow: BoxShadow.lerp(a.shadow, shadow, t)!,
      );
    }
    return super.lerpFrom(a, t);
  }

  @override
  Decoration? lerpTo(Decoration? b, double t) {
    if (b is _GlassShadowDecoration) return b.lerpFrom(this, t);
    return super.lerpTo(b, t);
  }

  @override
  BoxPainter createBoxPainter([VoidCallback? onChanged]) =>
      _GlassShadowPainter(this);
}

class _GlassShadowPainter extends BoxPainter {
  _GlassShadowPainter(this.decoration);

  final _GlassShadowDecoration decoration;
  Size? _size;
  TextDirection? _direction;
  Path? _outline;
  Path? _exterior;

  @override
  void paint(Canvas canvas, Offset offset, ImageConfiguration configuration) {
    final size = configuration.size;
    final shadow = decoration.shadow;
    if (size == null || size.isEmpty || shadow.color.a == 0) return;
    if (_size != size || _direction != configuration.textDirection) {
      _size = size;
      _direction = configuration.textDirection;
      final rect = Offset.zero & size;
      _outline =
          decoration.shape?.getOuterPath(
            rect,
            textDirection: configuration.textDirection,
          ) ??
          (Path()..addRRect(decoration.radius.toRRect(rect)));
      // Exterior ring without a boolean path operation for each changing
      // contour. The nested outline cuts a hole using the even-odd fill rule.
      _exterior = Path()
        ..fillType = PathFillType.evenOdd
        ..addRect(
          rect.inflate(shadow.blurSigma * 4 + shadow.offset.distance + 1),
        )
        ..addPath(_outline!, Offset.zero);
    }
    canvas.save();
    canvas.translate(offset.dx, offset.dy);
    canvas.clipPath(_exterior!);
    canvas.drawPath(_outline!.shift(shadow.offset), shadow.toPaint());
    canvas.restore();
  }
}
