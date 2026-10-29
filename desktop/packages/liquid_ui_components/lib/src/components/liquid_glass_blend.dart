import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import '../rendering/glass_backdrop.dart';
import '../rendering/stable_glass_backdrop.dart';
import '../surfaces/liquid_glass_surface.dart';
import '../surfaces/glass_shadow_decoration.dart';

part 'blend_geometry.dart';
part 'blend_items.dart';
part 'blend_items_outline.dart';

/// Two circular controls that fuse into one refractive liquid surface.
///
/// [separation] is the distance between lens centers, in logical pixels.
/// Animate it with a spring for continuous fusion and separation. On Impeller,
/// the merged field bends the live backdrop around both lobes and their neck.
/// Other engines use a matching frosted silhouette. Children keep their own
/// actions and semantics; glass is decorative and does not handle gestures.
class LiquidGlassBlend extends StatelessWidget {
  /// Builds any number of positioned actions, controls or panels.
  /// All items share a liquid contour; up to six use per-member optical fusion.
  factory LiquidGlassBlend.builder({
    Key? key,
    required int itemCount,
    required LiquidGlassBlendItem Function(BuildContext, int) itemBuilder,
    required Size size,
    double smoothness,
    LiquidGlassSettings? settings,
  }) = _ItemGlassBlend;

  /// Uses a list of independently draggable members in a bounded arena.
  factory LiquidGlassBlend.items({
    Key? key,
    required List<LiquidGlassBlendItem> items,
    required Size size,
    double smoothness = 40,
    LiquidGlassSettings? settings,
  }) {
    final snapshot = List<LiquidGlassBlendItem>.unmodifiable(items);
    return LiquidGlassBlend.builder(
      key: key,
      size: size,
      itemCount: snapshot.length,
      smoothness: smoothness,
      settings: settings,
      itemBuilder: (_, index) => snapshot[index],
    );
  }

  const LiquidGlassBlend({
    super.key,
    required this.first,
    required this.second,
    required this.separation,
    this.radius = 48,
    this.smoothness = 40,
    this.viscosity = 0.5,
    this.settings,
  }) : playgroundSize = null,
       _firstCenter = null,
       _secondCenter = null,
       _canvasSize = null,
       assert(radius > 0 && radius < double.infinity),
       assert(viscosity >= 0 && viscosity <= 1),
       assert(separation >= 0 && separation < double.infinity),
       assert(smoothness > 0 && smoothness < double.infinity);

  /// A bounded play area where users drag either lens in two dimensions.
  /// Positions are owned internally and retained when the parent rebuilds.
  LiquidGlassBlend.draggable({
    super.key,
    required this.first,
    required this.second,
    this.radius = 48,
    this.smoothness = 40,
    this.viscosity = 0.5,
    this.settings,
    Size size = const Size(320, 240),
  }) : playgroundSize = size,
       _firstCenter = null,
       _secondCenter = null,
       _canvasSize = null,
       separation = 0,
       assert(size.width > 0 && size.width < double.infinity),
       assert(size.height > 0 && size.height < double.infinity),
       assert(radius > 0 && radius < double.infinity),
       assert(viscosity >= 0 && viscosity <= 1),
       assert(smoothness > 0 && smoothness < double.infinity);

  final Widget first;
  final Widget second;
  final double separation;
  final double radius;

  /// Maximum fusion softness at contact. Its influence fades to zero once
  /// the edge gap reaches min(16 logical pixels, 35% of the radius).
  final double smoothness;

  /// Artistic optical thickness, from flat glass (0) to a deeper liquid lens
  /// (1). Scales refraction, magnification and depth; 0.5 preserves [settings].
  /// Does not change opacity, blur, diameter or the distance at which drops fuse.
  /// This is a visual control, rather than a physical viscosity measurement.
  final double viscosity;
  final LiquidGlassSettings? settings;
  final Size? playgroundSize;
  final Offset? _firstCenter;
  final Offset? _secondCenter;
  final Size? _canvasSize;

  LiquidGlassBlend._positioned({
    required this.first,
    required this.second,
    required Offset firstCenter,
    required Offset secondCenter,
    required Size size,
    required this.radius,
    required this.smoothness,
    required this.viscosity,
    this.settings,
  }) : _firstCenter = firstCenter,
       _secondCenter = secondCenter,
       _canvasSize = size,
       separation = (secondCenter - firstCenter).distance,
       playgroundSize = null;

  @override
  Widget build(BuildContext context) {
    if (playgroundSize != null) return _DraggableBlend(blend: this);
    final config = settings ?? LiquidGlassTheme.of(context);
    final gap = math.max(0.0, separation - radius * 2);
    final proximity = (1 - gap / math.min(16.0, radius * 0.35)).clamp(0.0, 1.0);
    final blendWidth = math.max(
      0.001,
      smoothness * proximity * proximity * (3 - 2 * proximity),
    );
    final theme = Theme.of(context);
    final solid =
        config.reduceTransparency ||
        MediaQuery.highContrastOf(context) ||
        config.rendering == LiquidGlassRendering.solid;
    final opticalEnabled =
        !solid &&
        (config.rendering == LiquidGlassRendering.automatic ||
            config.rendering == LiquidGlassRendering.balanced) &&
        ui.ImageFilter.isShaderFilterSupported;
    final tint = (config.tintColor ?? theme.colorScheme.surface).withValues(
      alpha: config.opacity,
    );
    final foreground = config.foregroundColor ?? theme.colorScheme.onSurface;
    final edges = config.resolveEdgeOptics(Size.square(radius * 2));
    final lensStrength = viscosity * 2;
    final members = SizedBox(
      width: _canvasSize?.width ?? separation + radius * 2,
      height: _canvasSize?.height ?? radius * 2,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Positioned(
            left: (_firstCenter?.dx ?? radius) - radius,
            top: (_firstCenter?.dy ?? radius) - radius,
            width: radius * 2,
            height: radius * 2,
            child: Center(child: first),
          ),
          Positioned(
            left: (_secondCenter?.dx ?? separation + radius) - radius,
            top: (_secondCenter?.dy ?? radius) - radius,
            width: radius * 2,
            height: radius * 2,
            child: Center(child: second),
          ),
        ],
      ),
    );
    final outline = _firstCenter == null
        ? _BlendedLenses(
            distance: separation,
            radius: radius,
            smoothness: blendWidth,
          )
        : _PositionedBlendBorder(
            first: _firstCenter,
            second: _secondCenter!,
            radius: radius,
            smoothness: blendWidth,
          );
    final surface = opticalEnabled
        ? GlassShadow(
            shape: outline,
            shadow: BoxShadow(
              color: Colors.black.withValues(alpha: config.shadowOpacity),
              blurRadius: config.shadowBlur,
              offset: const Offset(0, 5),
            ),
            child: ClipPath(
              clipper: ShapeBorderClipper(shape: outline),
              child: GlassBackdrop(
                settings: config.copyWith(
                  refraction: 1 + (config.refraction - 1) * lensStrength,
                  depth: (edges.depth * lensStrength).clamp(0.0, 1.0),
                  magnification: math.max(
                    0.01,
                    1 + (config.magnification - 1) * lensStrength,
                  ),
                  refractionWidth: edges.refractionWidth,
                  scaleRefractionWithSize: false,
                ),
                tint: tint,
                blendMembers: [first, second],
                blendSize: Size(
                  _canvasSize?.width ?? separation + radius * 2,
                  _canvasSize?.height ?? radius * 2,
                ),
                blend: GlassBlendGeometry(
                  first: _firstCenter ?? Offset(radius, radius),
                  second: _secondCenter ?? Offset(separation + radius, radius),
                  radius: radius,
                  smoothness: blendWidth,
                ),
                child: members,
              ),
            ),
          )
        : LiquidGlassSurface(
            settings: config.copyWith(animationDuration: Duration.zero),
            shape: outline,
            child: members,
          );
    return IconTheme.merge(
      data: IconThemeData(color: foreground),
      child: DefaultTextStyle.merge(
        style: TextStyle(color: foreground),
        child: surface,
      ),
    );
  }
}

class _DraggableBlend extends StatefulWidget {
  const _DraggableBlend({required this.blend});
  final LiquidGlassBlend blend;

  @override
  State<_DraggableBlend> createState() => _DraggableBlendState();
}

class _DraggableBlendState extends State<_DraggableBlend> {
  final _arena = GlobalKey();
  final _positions = [const Offset(0.15, 0.42), const Offset(0.85, 0.58)];
  final _lastPointer = <int, Offset>{};

  @override
  Widget build(BuildContext context) => SizedBox(
    width: widget.blend.playgroundSize!.width,
    height: widget.blend.playgroundSize!.height,
    child: LayoutBuilder(
      builder: (context, constraints) {
        final size = constraints.biggest;
        final r = math.min(widget.blend.radius, size.shortestSide / 2);
        final travel = Size(
          math.max(0, size.width - r * 2),
          math.max(0, size.height - r * 2),
        );
        Offset center(int i) => Offset(
          r + _positions[i].dx * travel.width,
          r + _positions[i].dy * travel.height,
        );
        void move(int i, Offset delta) => setState(() {
          final p = center(i) + delta;
          _positions[i] = Offset(
            travel.width == 0
                ? 0.5
                : ((p.dx - r) / travel.width).clamp(0.0, 1.0),
            travel.height == 0
                ? 0.5
                : ((p.dy - r) / travel.height).clamp(0.0, 1.0),
          );
        });
        final a = center(0);
        final b = center(1);
        Widget handle(int i, Widget child) => Semantics(
          label: 'Movable glass ${i + 1}',
          hint: 'Drag to blend with the other glass',
          customSemanticsActions: {
            CustomSemanticsAction(label: 'Move left'): () =>
                move(i, const Offset(-20, 0)),
            CustomSemanticsAction(label: 'Move right'): () =>
                move(i, const Offset(20, 0)),
            CustomSemanticsAction(label: 'Move up'): () =>
                move(i, const Offset(0, -20)),
            CustomSemanticsAction(label: 'Move down'): () =>
                move(i, const Offset(0, 20)),
          },
          child: GestureDetector(
            key: ValueKey('blend-drag-$i'),
            behavior: HitTestBehavior.opaque,
            onPanDown: (details) => _lastPointer[i] = details.globalPosition,
            onPanStart: (details) =>
                _lastPointer.putIfAbsent(i, () => details.globalPosition),
            onPanUpdate: (details) {
              final box =
                  _arena.currentContext!.findRenderObject()! as RenderBox;
              final previous = _lastPointer[i] ?? details.globalPosition;
              move(
                i,
                box.globalToLocal(details.globalPosition) -
                    box.globalToLocal(previous),
              );
              _lastPointer[i] = details.globalPosition;
            },
            onPanEnd: (_) => _lastPointer.remove(i),
            onPanCancel: () => _lastPointer.remove(i),
            child: SizedBox.square(
              dimension: r * 2,
              child: Center(child: child),
            ),
          ),
        );
        return ClipRect(
          child: Stack(
            key: _arena,
            clipBehavior: Clip.none,
            children: [
              LiquidGlassBlend._positioned(
                firstCenter: a,
                secondCenter: b,
                size: size,
                radius: r,
                smoothness: widget.blend.smoothness,
                viscosity: widget.blend.viscosity,
                settings: widget.blend.settings,
                first: handle(0, widget.blend.first),
                second: handle(1, widget.blend.second),
              ),
            ],
          ),
        );
      },
    ),
  );
}

/// Rotate only the fallback path, never the optical member boxes. Rotating
/// a circular member's square box inflates its screen-space bounds and makes
/// the shader interpret diagonal placement as a larger droplet.
