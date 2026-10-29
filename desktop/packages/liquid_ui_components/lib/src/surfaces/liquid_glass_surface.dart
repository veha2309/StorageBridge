import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../configuration/liquid_glass_settings.dart';
import '../rendering/glass_backdrop.dart';
import 'glass_shadow_decoration.dart';

export '../configuration/liquid_glass_settings.dart';

part 'glass_rim_painter.dart';

/// A reusable glass backdrop with crisp content painted above the optical lens.
///
/// Place over real content in a Stack. Use for floating controls, not whole pages.
/// No fixed size is required; the child determines the surface's dimensions.
class LiquidGlassSurface extends StatelessWidget {
  // Reuse blur kernels for common presets rather than allocating one for
  // every animated geometry frame. Bound the cache for custom animated blur.
  static final _blurFilters = <double, ui.ImageFilter>{};
  static ui.ImageFilter _blurFilter(double sigma) {
    final cached = _blurFilters[sigma];
    if (cached != null) return cached;
    if (_blurFilters.length >= 16) _blurFilters.remove(_blurFilters.keys.first);
    return _blurFilters[sigma] = ui.ImageFilter.blur(
      sigmaX: sigma,
      sigmaY: sigma,
    );
  }

  const LiquidGlassSurface({
    super.key,
    required this.child,
    this.settings,
    this.borderRadius = const BorderRadius.all(Radius.circular(28)),
    this.shape,
    this.padding = EdgeInsets.zero,
  });

  final Widget child;
  final LiquidGlassSettings? settings;
  final BorderRadius borderRadius;

  /// Optional outline shared by the tint, clip, rim and shadow. Custom outlines
  /// use a rectangular optical sampling layer; the rim follows this shape.
  /// The optical renderer's refraction field remains rectangular.
  final OutlinedBorder? shape;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    final config = settings ?? LiquidGlassTheme.of(context);
    assert(!config.animationDuration.isNegative);
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final solid =
        config.reduceTransparency ||
        MediaQuery.highContrastOf(context) ||
        config.rendering == LiquidGlassRendering.solid;
    final opticalEnabled =
        !solid &&
        !MediaQuery.disableAnimationsOf(context) &&
        (config.rendering == LiquidGlassRendering.automatic ||
            config.rendering == LiquidGlassRendering.balanced) &&
        ui.ImageFilter.isShaderFilterSupported;
    final tint = (config.tintColor ?? theme.colorScheme.surface).withValues(
      alpha: solid
          ? 1
          : (config.opacity +
                    (dark && config.variant == LiquidGlassVariant.regular
                        ? 0.15
                        : 0))
                .clamp(0.0, 1.0),
    );
    final foreground = solid
        ? ThemeData.estimateBrightnessForColor(tint) == Brightness.dark
              ? Colors.white
              : Colors.black
        : config.foregroundColor ?? theme.colorScheme.onSurface;
    final side = BorderSide(
      color: foreground.withValues(alpha: solid ? 0.4 : 0.20),
      width: config.borderWidth,
    );
    final outline = shape?.copyWith(side: solid ? side : BorderSide.none);
    final Decoration decoration = outline != null
        ? ShapeDecoration(color: opticalEnabled ? null : tint, shape: outline)
        : BoxDecoration(
            color: opticalEnabled ? null : tint,
            borderRadius: borderRadius,
            border: solid ? Border.fromBorderSide(side) : null,
          );
    Widget backdrop;
    if (opticalEnabled) {
      backdrop = GlassBackdrop(
        settings: config,
        tint: tint,
        borderRadius: borderRadius,
        drawRim: outline == null,
        child: const SizedBox.expand(),
      );
    } else {
      backdrop = const SizedBox.expand();
    }
    backdrop = DecoratedBox(
      decoration: decoration,
      position: opticalEnabled
          ? DecorationPosition.foreground
          : DecorationPosition.background,
      child: backdrop,
    );
    if (!solid && !opticalEnabled && config.blur > 0) {
      backdrop = BackdropFilter.grouped(
        filter: _blurFilter(config.blur),
        child: backdrop,
      );
    }
    final shadow = BoxShadow(
      color: Colors.black.withValues(alpha: config.shadowOpacity),
      blurRadius: config.shadowBlur,
      offset: const Offset(0, 5),
    );
    final content = Stack(
      children: [
        Positioned.fill(
          child: IgnorePointer(child: ExcludeSemantics(child: backdrop)),
        ),
        if (!solid && (!opticalEnabled || outline != null))
          Positioned.fill(
            child: IgnorePointer(
              child: ExcludeSemantics(
                child: CustomPaint(
                  painter: _GlassRimPainter(
                    radius: borderRadius,
                    shape: outline,
                    width: config.borderWidth,
                    direction: config.lightDirection,
                    intensity: config.lightIntensity,
                    ambient: config.edgeAmbientIntensity,
                    spread: config.edgeLightSpread,
                  ),
                ),
              ),
            ),
          ),
        BackdropGroup(
          child: IconTheme.merge(
            data: IconThemeData(color: foreground),
            child: DefaultTextStyle.merge(
              style: TextStyle(color: foreground),
              child: Padding(padding: padding, child: child),
            ),
          ),
        ),
      ],
    );
    return GlassShadow(
      duration: MediaQuery.disableAnimationsOf(context)
          ? Duration.zero
          : config.animationDuration,
      radius: borderRadius,
      shape: outline,
      shadow: shadow,
      child: outline != null
          ? ClipPath(
              clipper: ShapeBorderClipper(shape: outline),
              child: content,
            )
          : ClipRRect(borderRadius: borderRadius, child: content),
    );
  }
}
