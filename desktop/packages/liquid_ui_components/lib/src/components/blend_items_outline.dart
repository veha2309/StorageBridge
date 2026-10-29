part of 'liquid_glass_blend.dart';

/// One smooth-union contour for every item, independent of shader pass limits.
/// A bounded sampling grid keeps drag work bounded as the arena grows.
Path _blendItemsOutline(
  List<LiquidGlassBlendItem> items,
  Rect arena,
  double softness,
) {
  if (items.isEmpty) return Path();
  var region = items.first.bounds;
  for (final item in items.skip(1)) {
    region = region.expandToInclude(item.bounds);
  }
  region = region.inflate(softness / 2 + 2);
  final step = math.max(2.0, region.longestSide / 96);
  final nx = (region.width / step).ceil();
  final ny = (region.height / step).ceil();
  double field(Offset point) {
    var distance = double.infinity;
    for (final item in items) {
      final radius = math.min(item.borderRadius, item.bounds.shortestSide / 2);
      final p = point - item.bounds.center;
      final x = p.dx.abs() - item.bounds.width / 2 + radius;
      final y = p.dy.abs() - item.bounds.height / 2 + radius;
      final d =
          math.sqrt(math.pow(math.max(x, 0), 2) + math.pow(math.max(y, 0), 2)) +
          math.min(math.max(x, y), 0) -
          radius;
      final k = math.max(
        0.001,
        math.min(softness, item.bounds.shortestSide / 2),
      );
      final h = math.max(k - (distance - d).abs(), 0) / k;
      distance = math.min(distance, d) - h * h * k * 0.25;
    }
    return distance;
  }

  final values = List.generate(
    ny + 1,
    (y) => List.generate(
      nx + 1,
      (x) => field(region.topLeft + Offset(x * step, y * step)),
    ),
  );
  final points = <int, Offset>{};
  final links = <int, List<int>>{};
  const cases = <List<int>>[
    [],
    [3, 0],
    [0, 1],
    [3, 1],
    [1, 2],
    [3, 0, 1, 2],
    [0, 2],
    [3, 2],
    [2, 3],
    [0, 2],
    [0, 1, 2, 3],
    [1, 2],
    [1, 3],
    [0, 1],
    [0, 3],
    [],
  ];
  for (var y = 0; y < ny; y++) {
    for (var x = 0; x < nx; x++) {
      final v = [
        values[y][x],
        values[y][x + 1],
        values[y + 1][x + 1],
        values[y + 1][x],
      ];
      final code =
          (v[0] < 0 ? 1 : 0) |
          (v[1] < 0 ? 2 : 0) |
          (v[2] < 0 ? 4 : 0) |
          (v[3] < 0 ? 8 : 0);
      final edges = cases[code];
      final ids = [
        (y * (nx + 1) + x) * 2,
        (y * (nx + 1) + x + 1) * 2 + 1,
        ((y + 1) * (nx + 1) + x) * 2,
        (y * (nx + 1) + x) * 2 + 1,
      ];
      final corners = [
        Offset(x * step, y * step),
        Offset((x + 1) * step, y * step),
        Offset((x + 1) * step, (y + 1) * step),
        Offset(x * step, (y + 1) * step),
      ];
      for (final edge in edges) {
        final next = (edge + 1) % 4;
        points.putIfAbsent(
          ids[edge],
          () =>
              region.topLeft +
              Offset.lerp(
                corners[edge],
                corners[next],
                (v[edge] / (v[edge] - v[next])).clamp(0, 1),
              )!,
        );
      }
      for (var i = 0; i < edges.length; i += 2) {
        final a = ids[edges[i]], b = ids[edges[i + 1]];
        (links[a] ??= []).add(b);
        (links[b] ??= []).add(a);
      }
    }
  }
  final path = Path();
  final visited = <int>{};
  for (final start in links.keys) {
    if (visited.contains(start)) continue;
    var current = start;
    int? previous;
    path.moveTo(points[start]!.dx + arena.left, points[start]!.dy + arena.top);
    while (visited.add(current)) {
      final neighbors = links[current]!;
      final next = neighbors.firstWhere(
        (id) => id != previous,
        orElse: () => start,
      );
      final p = points[next]!;
      path.lineTo(p.dx + arena.left, p.dy + arena.top);
      previous = current;
      current = next;
    }
    path.close();
  }
  return path;
}
