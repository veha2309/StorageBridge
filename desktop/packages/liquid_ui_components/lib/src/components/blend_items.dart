part of 'liquid_glass_blend.dart';

/// A positioned glass member. Its content can be any interactive widget.
@immutable
class LiquidGlassBlendItem {
  const LiquidGlassBlendItem({
    this.key,
    required this.bounds,
    required this.child,
    this.borderRadius = 28,
  }) : assert(borderRadius >= 0);

  /// Stable identity used to retain a position when the list is reordered.
  final LocalKey? key;
  final Rect bounds;
  final Widget child;
  final double borderRadius;
}

// ignore_for_file: sort_child_properties_last

class _ItemGlassBlend extends LiquidGlassBlend {
  _ItemGlassBlend({
    super.key,
    required this.itemCount,
    required this.itemBuilder,
    required this.size,
    super.smoothness = 40,
    super.settings,
  }) : super(first: const SizedBox(), second: const SizedBox(), separation: 0) {
    if (itemCount < 0) throw ArgumentError.value(itemCount, 'itemCount');
    if (!size.isFinite || size.isEmpty) throw ArgumentError.value(size, 'size');
  }
  final int itemCount;
  final LiquidGlassBlendItem Function(BuildContext, int) itemBuilder;
  final Size size;

  @override
  Widget build(BuildContext context) => _MovableBlendItems(blend: this);
}

class _MovableBlendItems extends StatefulWidget {
  const _MovableBlendItems({required this.blend});
  final _ItemGlassBlend blend;
  @override
  State<_MovableBlendItems> createState() => _MovableBlendItemsState();
}

class _MovableBlendItemsState extends State<_MovableBlendItems> {
  final _positions = <Object, Offset>{};
  Size get size => widget.blend.size;
  LiquidGlassSettings? get settings => widget.blend.settings;
  double get smoothness => widget.blend.smoothness;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final available = Size(
        math.min(size.width, constraints.maxWidth),
        math.min(size.height, constraints.maxHeight),
      );
      return SizedBox.fromSize(
        size: available,
        child: ClipRect(child: _buildItems(context, available)),
      );
    },
  );

  Widget _buildItems(BuildContext context, Size size) {
    final source = List.generate(
      widget.blend.itemCount,
      (i) => widget.blend.itemBuilder(context, i),
    );
    final identities = <Object>{};
    for (var i = 0; i < source.length; i++) {
      if (!identities.add(source[i].key ?? i)) {
        throw ArgumentError('Blend item keys must be unique.');
      }
    }
    _positions.removeWhere((key, _) => !identities.contains(key));
    final items = <LiquidGlassBlendItem>[];
    for (var i = 0; i < source.length; i++) {
      final item = source[i];
      final identity = item.key ?? i;
      Offset bound(Offset p) => Offset(
        p.dx.clamp(0.0, math.max(0.0, size.width - item.bounds.width)),
        p.dy.clamp(0.0, math.max(0.0, size.height - item.bounds.height)),
      );
      final position = bound(_positions[identity] ?? item.bounds.topLeft);
      void move(Offset delta) => setState(
        () => _positions[identity] = bound(
          (_positions[identity] ?? position) + delta,
        ),
      );
      items.add(
        LiquidGlassBlendItem(
          key: item.key,
          bounds: position & item.bounds.size,
          borderRadius: item.borderRadius,
          child: Semantics(
            label: 'Movable glass ${i + 1}',
            customSemanticsActions: {
              CustomSemanticsAction(label: 'Move left'): () =>
                  move(const Offset(-20, 0)),
              CustomSemanticsAction(label: 'Move right'): () =>
                  move(const Offset(20, 0)),
              CustomSemanticsAction(label: 'Move up'): () =>
                  move(const Offset(0, -20)),
              CustomSemanticsAction(label: 'Move down'): () =>
                  move(const Offset(0, 20)),
            },
            child: GestureDetector(
              key: ValueKey('blend-item-drag-$identity'),
              behavior: HitTestBehavior.opaque,
              onPanUpdate: (details) => move(details.delta),
              child: item.child,
            ),
          ),
        ),
      );
    }
    for (final item in items) {
      if (!item.bounds.isFinite ||
          item.bounds.isEmpty ||
          !item.borderRadius.isFinite ||
          item.borderRadius < 0) {
        throw ArgumentError(
          'Blend members require finite nonempty bounds and radius.',
        );
      }
    }
    final config = settings ?? LiquidGlassTheme.of(context);
    final optical =
        !config.reduceTransparency &&
        !MediaQuery.highContrastOf(context) &&
        ui.ImageFilter.isShaderFilterSupported &&
        (config.rendering == LiquidGlassRendering.automatic ||
            config.rendering == LiquidGlassRendering.balanced);
    if (optical && items.length <= 6) {
      return SizedBox.fromSize(
        size: size,
        child: Stack(
          children: [
            for (var start = 0; start < items.length; start += 6)
              StableBlendPass(
                size: size,
                bounds: items.skip(start).take(6).map((e) => e.bounds).toList(),
                radii: items
                    .skip(start)
                    .take(6)
                    .map((e) => e.borderRadius)
                    .toList(),
                children: items
                    .skip(start)
                    .take(6)
                    .map((e) => e.child)
                    .toList(),
                settings: config,
                smoothness: smoothness,
              ),
          ],
        ),
      );
    }
    if (items.isEmpty) return SizedBox.fromSize(size: size);
    return LiquidGlassSurface(
      settings: config,
      shape: _BlendItemsBorder(items, smoothness: smoothness),
      child: SizedBox.fromSize(
        size: size,
        child: Stack(
          children: [
            for (final item in items)
              Positioned.fromRect(rect: item.bounds, child: item.child),
          ],
        ),
      ),
    );
  }
}

/// The portable fallback shares one blur and tint over the member union.
class _BlendItemsBorder extends OutlinedBorder {
  _BlendItemsBorder(this.items, {this.smoothness = 40, super.side});
  final double smoothness;
  final _paths = <Rect, Path>{};
  final List<LiquidGlassBlendItem> items;
  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(side.width);
  @override
  _BlendItemsBorder copyWith({BorderSide? side}) =>
      _BlendItemsBorder(items, smoothness: smoothness, side: side ?? this.side);
  @override
  ShapeBorder scale(double t) => _BlendItemsBorder(
    [
      for (final item in items)
        LiquidGlassBlendItem(
          bounds: Rect.fromLTRB(
            item.bounds.left * t,
            item.bounds.top * t,
            item.bounds.right * t,
            item.bounds.bottom * t,
          ),
          borderRadius: item.borderRadius * t,
          child: item.child,
        ),
    ],
    smoothness: smoothness * t,
    side: side.scale(t),
  );
  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) => _paths
      .putIfAbsent(rect, () => _blendItemsOutline(items, rect, smoothness));

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      getOuterPath(rect);
  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    if (side.style != BorderStyle.none) {
      canvas.drawPath(getOuterPath(rect), side.toPaint());
    }
  }
}
