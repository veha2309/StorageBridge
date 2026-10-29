import 'package:flutter/material.dart';
import 'package:liquid_glass_easy/liquid_glass_easy.dart' as optical;

import '../configuration/liquid_glass_settings.dart';
import 'glass_blend_geometry.dart';
import '../scrolling/liquid_glass_scroll.dart';

/// Adapter for the device-tested renderer. Third-party types stay in this file.
class StableGlassBackdrop extends StatelessWidget {
  const StableGlassBackdrop({
    super.key,
    required this.settings,
    required this.tint,
    required this.radius,
    required this.drawRim,
    required this.child,
    this.blend,
    this.blendMembers,
    this.blendSize,
  });

  final LiquidGlassSettings settings;
  final Color tint;
  final BorderRadius radius;
  final bool drawRim;
  final Widget child;
  final GlassBlendGeometry? blend;
  final List<Widget>? blendMembers;
  final Size? blendSize;

  optical.LiquidGlassStyle _style(Size size, {double? cornerRadius}) {
    final edges = settings.resolveEdgeOptics(size);
    return optical.LiquidGlassStyle(
      shape: optical.LiquidGlassShape.continuousRoundedRectangle(
        cornerRadius: cornerRadius ?? (drawRim ? radius.topLeft.x : 0),
        borderWidth: drawRim ? settings.borderWidth : 0,
        lightIntensity: drawRim ? settings.lightIntensity : 0,
        lightDirection: settings.lightDirection % 360,
        borderType: optical.OpticalBorder(
          ambientIntensity: settings.edgeAmbientIntensity,
          lightSpread: settings.edgeLightSpread,
          borderSaturation: settings.saturation.clamp(0.0, 3.0),
        ),
      ),
      appearance: optical.LiquidGlassAppearance(
        color: tint,
        saturation: settings.saturation,
        blur: optical.LiquidGlassBlur(
          sigmaX: settings.blur,
          sigmaY: settings.blur,
        ),
      ),
      refraction: optical.LiquidGlassRefraction(
        magnification: settings.magnification,
        chromaticAberration: settings.rendering == LiquidGlassRendering.balanced
            ? 0
            : settings.chromaticAberration,
        refractionType: optical.OpticalRefraction(
          refraction: settings.refraction,
          refractionWidth: edges.refractionWidth,
          depth: edges.depth,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) =>
      ScrollAwareGlass(child: _buildGlass(context));

  Widget _buildGlass(BuildContext context) {
    final geometry = blend;
    final members = blendMembers;
    if (geometry != null && members != null && blendSize != null) {
      final style = _style(
        Size.square(geometry.radius * 2),
        cornerRadius: geometry.radius,
      );
      return optical.LiquidGlassBlender(
        style: style,
        smoothness: geometry.smoothness,
        useEngineBlur: false,
        child: SizedBox.fromSize(
          size: blendSize,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              for (var i = 0; i < 2; i++)
                Positioned(
                  left:
                      (i == 0 ? geometry.first.dx : geometry.second.dx) -
                      geometry.radius,
                  top:
                      (i == 0 ? geometry.first.dy : geometry.second.dy) -
                      geometry.radius,
                  width: geometry.radius * 2,
                  height: geometry.radius * 2,
                  child: optical.LiquidGlassLens(
                    style: style,
                    child: Center(child: members[i]),
                  ),
                ),
            ],
          ),
        ),
      );
    }
    return LayoutBuilder(
      builder: (context, constraints) => optical.LiquidGlassLens(
        style: _style(constraints.biggest),
        child: child,
      ),
    );
  }
}

/// Internal multi-member adapter. Each pass respects the stable shader limit.
class StableBlendPass extends StatelessWidget {
  const StableBlendPass({
    super.key,
    required this.size,
    required this.bounds,
    required this.radii,
    required this.children,
    required this.settings,
    required this.smoothness,
  });
  final Size size;
  final List<Rect> bounds;
  final List<double> radii;
  final List<Widget> children;
  final LiquidGlassSettings settings;
  final double smoothness;
  @override
  Widget build(BuildContext context) =>
      ScrollAwareGlass(child: _buildGlass(context));

  Widget _buildGlass(BuildContext context) {
    final tint = (settings.tintColor ?? Theme.of(context).colorScheme.surface)
        .withValues(alpha: settings.opacity);
    final adapter = StableGlassBackdrop(
      settings: settings,
      tint: tint,
      radius: BorderRadius.zero,
      drawRim: true,
      child: const SizedBox(),
    );
    final lenses = [
      for (var i = 0; i < children.length; i++)
        Positioned.fromRect(
          rect: bounds[i],
          child: optical.LiquidGlassLens(
            style: adapter._style(bounds[i].size, cornerRadius: radii[i]),
            child: children[i],
          ),
        ),
    ];
    final stack = SizedBox.fromSize(
      size: size,
      child: Stack(children: lenses),
    );
    if (children.length < 2) return stack;
    return optical.LiquidGlassBlender(
      style: adapter._style(bounds.first.size, cornerRadius: radii.first),
      smoothness: smoothness,
      useEngineBlur: false,
      child: stack,
    );
  }
}
