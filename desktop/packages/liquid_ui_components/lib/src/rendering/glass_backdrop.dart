import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/foundation.dart' show listEquals;

import '../configuration/liquid_glass_settings.dart';
import 'stable_glass_backdrop.dart';
import 'glass_blend_geometry.dart';

export 'glass_blend_geometry.dart';

part 'glass_backdrop_layer.dart';
part 'glass_shader_uniforms.dart';

/// Package-owned live backdrop. Foreground children never enter the shader.
/// Unsupported backends and shader loading failures retain a tinted blur.
class GlassBackdrop extends StatefulWidget {
  const GlassBackdrop({
    super.key,
    required this.settings,
    required this.tint,
    required this.child,
    this.borderRadius = BorderRadius.zero,
    this.drawRim = true,
    this.blend,
    this.blendMembers,
    this.blendSize,
  });

  final LiquidGlassSettings settings;
  final Color tint;
  final Widget child;
  final BorderRadius borderRadius;
  final bool drawRim;
  final GlassBlendGeometry? blend;
  final List<Widget>? blendMembers;
  final Size? blendSize;

  @override
  State<GlassBackdrop> createState() => _GlassBackdropState();
}

class _GlassBackdropState extends State<GlassBackdrop> {
  // Programs are immutable and shared. Mutable shader instances are per lens.
  static Future<ui.FragmentProgram>? _program;
  ui.FragmentShader? _shader;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _ensureNative();
  }

  void _ensureNative() {
    if (widget.settings.renderer == LiquidGlassRenderer.experimentalNative &&
        ui.ImageFilter.isShaderFilterSupported &&
        _shader == null &&
        !_loading) {
      _load();
    }
  }

  @override
  void didUpdateWidget(GlassBackdrop oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.settings.renderer != widget.settings.renderer) {
      _ensureNative();
    }
  }

  Future<void> _load() async {
    _loading = true;
    try {
      final program = await (_program ??= ui.FragmentProgram.fromAsset(
        'packages/liquid_ui_components/shaders/liquid_glass.frag',
      ));
      if (mounted) setState(() => _shader = program.fragmentShader());
    } catch (error, stack) {
      // A failed load must not leave the lens transparent or break its child.
      _program = null;
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: error,
          stack: stack,
          library: 'liquid_ui_components',
          context: ErrorDescription('loading the native glass shader'),
        ),
      );
    } finally {
      _loading = false;
    }
  }

  @override
  void dispose() {
    _shader?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (widget.settings.renderer == LiquidGlassRenderer.stable) {
      return StableGlassBackdrop(
        settings: widget.settings,
        tint: widget.tint,
        radius: widget.borderRadius,
        drawRim: widget.drawRim,
        blend: widget.blend,
        blendMembers: widget.blendMembers,
        blendSize: widget.blendSize,
        child: widget.child,
      );
    }
    final shader = _shader;
    if (shader == null) {
      return BackdropFilter(
        filter: ui.ImageFilter.blur(
          sigmaX: widget.settings.blur,
          sigmaY: widget.settings.blur,
        ),
        child: ColoredBox(color: widget.tint, child: widget.child),
      );
    }
    return _ShaderBackdrop(
      shader: shader,
      settings: widget.settings,
      tint: widget.tint,
      radius: widget.borderRadius,
      drawRim: widget.drawRim,
      blend: widget.blend,
      devicePixelRatio: View.of(context).devicePixelRatio,
      child: widget.child,
    );
  }
}

class _ShaderBackdrop extends SingleChildRenderObjectWidget {
  const _ShaderBackdrop({
    required this.shader,
    required this.settings,
    required this.tint,
    required this.radius,
    required this.drawRim,
    required this.blend,
    required this.devicePixelRatio,
    required super.child,
  });

  final ui.FragmentShader shader;
  final LiquidGlassSettings settings;
  final Color tint;
  final BorderRadius radius;
  final bool drawRim;
  final GlassBlendGeometry? blend;
  final double devicePixelRatio;

  @override
  _RenderShaderBackdrop createRenderObject(BuildContext context) =>
      _RenderShaderBackdrop(this);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderShaderBackdrop renderObject,
  ) {
    renderObject.configuration = this;
  }
}

class _RenderShaderBackdrop extends RenderProxyBox {
  _RenderShaderBackdrop(this._configuration);
  _ShaderBackdrop _configuration;
  final _filter = LayerHandle<_GlassBackdropLayer>();
  List<double>? _lastUniforms;

  set configuration(_ShaderBackdrop value) {
    if (_configuration.shader != value.shader) _lastUniforms = null;
    _configuration = value;
    markNeedsPaint();
  }

  @override
  bool get alwaysNeedsCompositing => true;

  @override
  void paint(PaintingContext context, Offset offset) {
    if (size.isEmpty) return;
    final effect = _filter.layer ??= _GlassBackdropLayer();
    effect.prepare = () {
      final values = _shaderUniformValues(
        _configuration,
        size,
        getTransformTo(null),
      );
      if (values != null && !listEquals(values, _lastUniforms)) {
        for (var i = 0; i < values.length; i++) {
          _configuration.shader.setFloat(i + 2, values[i]);
        }
        _lastUniforms = values;
        effect.filter = ui.ImageFilter.shader(_configuration.shader);
      }
    };
    context.pushLayer(effect, super.paint, offset);
  }

  @override
  void dispose() {
    _filter.layer = null;
    super.dispose();
  }
}
