part of 'liquid_glass_surface.dart';

class _GlassRimPainter extends CustomPainter {
  const _GlassRimPainter({
    required this.radius,
    required this.shape,
    required this.width,
    required this.direction,
    required this.intensity,
    required this.ambient,
    required this.spread,
  });
  final BorderRadius radius;
  final OutlinedBorder? shape;
  final double width;
  final double direction;
  final double intensity;
  final double ambient;
  final double spread;

  Path _path(Rect rect) =>
      shape?.getOuterPath(rect) ?? (Path()..addRRect(radius.toRRect(rect)));

  @override
  void paint(Canvas canvas, Size size) {
    if (width <= 0 || size.isEmpty) return;
    final rim = width.clamp(0.0, size.shortestSide / 4);
    final rect = Offset.zero & size;
    final radians = (direction % 360) * math.pi / 180;
    final light = Alignment(math.cos(radians), -math.sin(radians));
    final highlight = (intensity * 0.6 + ambient * 0.08).clamp(0.0, 0.9);
    final gradient = LinearGradient(
      begin: light,
      end: -light,
      colors: [
        Colors.white.withValues(alpha: highlight),
        Colors.white.withValues(alpha: (ambient * 0.12).clamp(0.0, 0.5)),
        Colors.black.withValues(
          alpha: (0.04 + ambient * 0.04).clamp(0.0, 0.24),
        ),
        Colors.white.withValues(alpha: highlight * 0.35),
      ],
      stops: [0, 0.18 + spread * 0.38, 0.76, 1],
    );
    // Broad reflected light gives the portable blur material a glass face,
    // rather than only a bright wire outline. Keep the centre transparent.
    canvas.drawPath(
      _path(rect),
      Paint()
        ..shader = LinearGradient(
          begin: light,
          end: -light,
          colors: [
            Colors.white.withValues(
              alpha: (intensity * 0.075).clamp(0.0, 0.12),
            ),
            Colors.white.withValues(alpha: 0.008),
            Colors.transparent,
            Colors.white.withValues(alpha: (ambient * 0.035).clamp(0.0, 0.04)),
          ],
          stops: const [0, 0.22, 0.68, 1],
        ).createShader(rect),
    );
    canvas.drawPath(
      _path(rect.deflate(rim / 2)),
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = rim
        ..shader = gradient.createShader(rect),
    );
    // A wider, translucent bevel gives the edge visible thickness. The sharp
    // outer rim alone reads as an outlined card on non-optical backends.
    final bevel = math.min(6.0, size.shortestSide * 0.08);
    if (bevel > rim * 2) {
      canvas.drawPath(
        _path(rect.deflate(bevel / 2)),
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = bevel
          ..shader = LinearGradient(
            begin: light,
            end: -light,
            colors: [
              Colors.white.withValues(
                alpha: (intensity * 0.10).clamp(0.0, 0.16),
              ),
              Colors.transparent,
              Colors.black.withValues(alpha: (ambient * 0.12).clamp(0.0, 0.12)),
              Colors.white.withValues(
                alpha: (intensity * 0.035).clamp(0.0, 0.06),
              ),
            ],
            stops: const [0, 0.35, 0.72, 1],
          ).createShader(rect),
      );
    }
    // A quieter inner rim defines thickness without another blur or saveLayer.
    if (size.shortestSide > rim * 4) {
      canvas.drawPath(
        _path(rect.deflate(rim * 1.6)),
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = rim * 0.45
          ..color = Colors.white.withValues(
            alpha: (ambient * 0.12).clamp(0.0, 0.3),
          ),
      );
    }
  }

  @override
  bool shouldRepaint(_GlassRimPainter oldDelegate) =>
      radius != oldDelegate.radius ||
      shape != oldDelegate.shape ||
      width != oldDelegate.width ||
      direction != oldDelegate.direction ||
      intensity != oldDelegate.intensity ||
      ambient != oldDelegate.ambient ||
      spread != oldDelegate.spread;
}
