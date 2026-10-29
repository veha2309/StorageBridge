part of 'glass_backdrop.dart';

/// Uniform order is an internal ABI shared only with liquid_glass.frag.
/// First two floats and the input sampler are populated by Flutter's engine.
List<double>? _shaderUniformValues(
  _ShaderBackdrop lens,
  Size size,
  Matrix4 transform,
) {
  final inverse = Matrix4.tryInvert(transform);
  if (inverse == null) return null;
  final m = inverse.storage;
  final f = transform.storage;
  final dpr = lens.devicePixelRatio;
  final config = lens.settings;
  final edges = config.resolveEdgeOptics(size);
  final blend = lens.blend;
  final radius = lens.radius;
  final tint = lens.tint;
  final values = <double>[
    // Global physical pixel -> local logical pixel (affine rows).
    m[0] / dpr, m[4] / dpr, m[12],
    m[1] / dpr, m[5] / dpr, m[13],
    // Local displacement -> global physical displacement.
    f[0] * dpr, f[4] * dpr, f[1] * dpr, f[5] * dpr,
    size.width, size.height,
    radius.topLeft.x, radius.topRight.x,
    radius.bottomRight.x, radius.bottomLeft.x,
    config.refraction, edges.refractionWidth, edges.depth,
    config.magnification,
    config.rendering == LiquidGlassRendering.balanced
        ? 0
        : config.chromaticAberration,
    config.blur * dpr, config.saturation,
    tint.r, tint.g, tint.b, tint.a,
    lens.drawRim ? config.borderWidth : 0,
    lens.drawRim ? config.lightIntensity : 0,
    config.lightDirection * 3.141592653589793 / 180,
    config.edgeAmbientIntensity, config.edgeLightSpread,
    blend == null ? 0 : 1,
    blend?.first.dx ?? 0, blend?.first.dy ?? 0,
    blend?.second.dx ?? 0, blend?.second.dy ?? 0,
    blend?.radius ?? 0, blend?.smoothness ?? 0,
  ];
  return values;
}
