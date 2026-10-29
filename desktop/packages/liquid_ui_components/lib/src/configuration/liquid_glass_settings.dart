import 'package:flutter/material.dart';

/// Regular glass prioritizes legibility; clear glass is for media backgrounds.
enum LiquidGlassVariant { regular, clear }

/// Selects optical rendering, a portable blur, or an opaque accessible surface.
enum LiquidGlassRendering {
  automatic,
  frosted,
  solid,

  /// Optical refraction with chromatic dispersion disabled.
  balanced,
}

/// Selects the production adapter or the opt-in v0.3 shader prototype.
enum LiquidGlassRenderer { stable, experimentalNative }

/// Shared optical settings. Geometry belongs to each component.
@immutable
class LiquidGlassSettings {
  const LiquidGlassSettings({
    this.variant = LiquidGlassVariant.regular,
    this.rendering = LiquidGlassRendering.automatic,
    this.renderer = LiquidGlassRenderer.stable,
    this.tintColor,
    this.foregroundColor,
    this.opacity = 0.30,
    this.blur = 8,
    this.saturation = 1.2,
    this.refraction = 1.35,
    this.depth = 0.045,
    this.magnification = 1,
    this.chromaticAberration = 0.0005,
    this.borderWidth = 1,
    this.lightIntensity = 0.65,
    this.lightDirection = 115,
    this.refractionWidth = 8,
    this.scaleRefractionWithSize = true,
    this.edgeAmbientIntensity = 0.35,
    this.edgeLightSpread = 0.55,
    this.shadowBlur = 18,
    this.shadowOpacity = 0.09,
    this.animationDuration = const Duration(milliseconds: 200),
    this.reduceTransparency = false,
    this.dynamicsIntensity = 1,
  }) : assert(opacity >= 0 && opacity <= 1),
       assert(blur >= 0 && blur < double.infinity),
       assert(saturation >= 0 && saturation < double.infinity),
       assert(refraction >= 1 && refraction < double.infinity),
       assert(depth >= 0 && depth <= 1),
       assert(magnification > 0 && magnification < double.infinity),
       assert(chromaticAberration >= 0 && chromaticAberration <= 1),
       assert(borderWidth >= 0 && borderWidth < double.infinity),
       assert(lightIntensity >= 0 && lightIntensity < double.infinity),
       assert(
         lightDirection > -double.infinity && lightDirection < double.infinity,
       ),
       assert(refractionWidth >= 0 && refractionWidth < double.infinity),
       assert(edgeAmbientIntensity >= 0 && edgeAmbientIntensity <= 5),
       assert(edgeLightSpread >= 0 && edgeLightSpread <= 1),
       assert(shadowBlur >= 0 && shadowBlur < double.infinity),
       assert(shadowOpacity >= 0 && shadowOpacity <= 1),
       assert(dynamicsIntensity >= 0 && dynamicsIntensity <= 1);

  /// Media preset. Supply a contrasting foreground and dim bright media behind it.
  const LiquidGlassSettings.clear({
    this.tintColor,
    this.foregroundColor = Colors.white,
    this.rendering = LiquidGlassRendering.automatic,
    this.renderer = LiquidGlassRenderer.stable,
    this.reduceTransparency = false,
    this.dynamicsIntensity = 1,
    this.lightDirection = 115,
    this.refractionWidth = 8,
    this.scaleRefractionWithSize = true,
    this.edgeAmbientIntensity = 0.35,
    this.edgeLightSpread = 0.55,
  }) : assert(dynamicsIntensity >= 0 && dynamicsIntensity <= 1),
       assert(
         lightDirection > -double.infinity && lightDirection < double.infinity,
       ),
       assert(refractionWidth >= 0 && refractionWidth < double.infinity),
       assert(edgeAmbientIntensity >= 0 && edgeAmbientIntensity <= 5),
       assert(edgeLightSpread >= 0 && edgeLightSpread <= 1),
       variant = LiquidGlassVariant.clear,
       opacity = 0.08,
       blur = 2,
       saturation = 1.1,
       refraction = 1.35,
       depth = 0.06,
       magnification = 1.015,
       chromaticAberration = 0.0008,
       borderWidth = 1,
       lightIntensity = 0.85,
       shadowBlur = 18,
       shadowOpacity = 0.12,
       animationDuration = const Duration(milliseconds: 200);

  final LiquidGlassVariant variant;
  final LiquidGlassRendering rendering;

  /// Native rendering is experimental until device visual parity is verified.
  final LiquidGlassRenderer renderer;
  final Color? tintColor;
  final Color? foregroundColor;

  /// Tint opacity in light mode; regular glass adds 0.15 in dark mode.
  final double opacity;
  final double blur;
  final double saturation;
  final double refraction;
  final double depth;
  final double magnification;
  final double chromaticAberration;
  final double borderWidth;
  final double lightIntensity;

  /// Direction of the rim light in degrees: 0 right, 90 top.
  final double lightDirection;

  /// Reference refraction band width in logical pixels, before size scaling.
  final double refractionWidth;
  final bool scaleRefractionWithSize;
  final double edgeAmbientIntensity;
  final double edgeLightSpread;
  final double shadowBlur;
  final double shadowOpacity;
  final Duration animationDuration;

  /// Host bridge for a platform's Reduce Transparency preference.
  final bool reduceTransparency;

  /// Strength of decorative press/drag dynamics. Zero disables these effects.
  /// Reduced motion disables them regardless of this setting.
  final double dynamicsIntensity;

  /// Returns a complete settings object with selected values replaced.
  /// Null color arguments retain the existing color.
  LiquidGlassSettings copyWith({
    LiquidGlassVariant? variant,
    LiquidGlassRendering? rendering,
    LiquidGlassRenderer? renderer,
    Color? tintColor,
    Color? foregroundColor,
    double? opacity,
    double? blur,
    double? saturation,
    double? refraction,
    double? depth,
    double? magnification,
    double? chromaticAberration,
    double? borderWidth,
    double? lightIntensity,
    double? lightDirection,
    double? refractionWidth,
    bool? scaleRefractionWithSize,
    double? edgeAmbientIntensity,
    double? edgeLightSpread,
    double? shadowBlur,
    double? shadowOpacity,
    Duration? animationDuration,
    bool? reduceTransparency,
    double? dynamicsIntensity,
  }) => LiquidGlassSettings(
    variant: variant ?? this.variant,
    rendering: rendering ?? this.rendering,
    renderer: renderer ?? this.renderer,
    tintColor: tintColor ?? this.tintColor,
    foregroundColor: foregroundColor ?? this.foregroundColor,
    opacity: opacity ?? this.opacity,
    blur: blur ?? this.blur,
    saturation: saturation ?? this.saturation,
    refraction: refraction ?? this.refraction,
    depth: depth ?? this.depth,
    magnification: magnification ?? this.magnification,
    chromaticAberration: chromaticAberration ?? this.chromaticAberration,
    borderWidth: borderWidth ?? this.borderWidth,
    lightIntensity: lightIntensity ?? this.lightIntensity,
    lightDirection: lightDirection ?? this.lightDirection,
    refractionWidth: refractionWidth ?? this.refractionWidth,
    scaleRefractionWithSize:
        scaleRefractionWithSize ?? this.scaleRefractionWithSize,
    edgeAmbientIntensity: edgeAmbientIntensity ?? this.edgeAmbientIntensity,
    edgeLightSpread: edgeLightSpread ?? this.edgeLightSpread,
    shadowBlur: shadowBlur ?? this.shadowBlur,
    shadowOpacity: shadowOpacity ?? this.shadowOpacity,
    animationDuration: animationDuration ?? this.animationDuration,
    reduceTransparency: reduceTransparency ?? this.reduceTransparency,
    dynamicsIntensity: dynamicsIntensity ?? this.dynamicsIntensity,
  );

  /// Resolve the optical bevel for a surface's actual layout size. Compact
  /// controls get a shallower/narrower rim; larger surfaces get more depth.
  /// The band never occupies more than 22% of the shortest side.
  ({double refractionWidth, double depth}) resolveEdgeOptics(Size size) {
    assert(size.isFinite && size.width >= 0 && size.height >= 0);
    if (size.isEmpty || refractionWidth == 0) {
      return (refractionWidth: 0, depth: 0);
    }
    final factor = scaleRefractionWithSize
        ? (size.shortestSide / 64).clamp(0.55, 1.35)
        : 1.0;
    return (
      refractionWidth: (refractionWidth * factor).clamp(
        0.0,
        size.shortestSide * 0.22,
      ),
      depth: (depth * factor).clamp(0.0, 1.0),
    );
  }
}

/// Provides settings to glass controls without replacing the Material theme.
class LiquidGlassTheme extends InheritedWidget {
  const LiquidGlassTheme({
    super.key,
    required this.settings,
    required super.child,
  });

  final LiquidGlassSettings settings;

  static LiquidGlassSettings? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<LiquidGlassTheme>()?.settings;

  static LiquidGlassSettings of(BuildContext context) =>
      maybeOf(context) ?? const LiquidGlassSettings();

  @override
  bool updateShouldNotify(LiquidGlassTheme oldWidget) =>
      settings != oldWidget.settings;
}
