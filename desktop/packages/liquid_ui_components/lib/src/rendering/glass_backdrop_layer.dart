part of 'glass_backdrop.dart';

/// Refresh uniforms during scene composition, even when an ancestor retained
/// this render object's paint. Scrolling/scale transforms cannot lag a frame.
/// This does not schedule frames: idle lenses have no ticker or polling loop.
class _GlassBackdropLayer extends ContainerLayer {
  VoidCallback? prepare;
  ui.ImageFilter? filter;

  @override
  bool get alwaysNeedsAddToScene => true;

  @override
  void addToScene(ui.SceneBuilder builder) {
    prepare?.call();
    final current = filter;
    if (current == null) {
      addChildrenToScene(builder);
      return;
    }
    engineLayer = builder.pushBackdropFilter(
      current,
      oldLayer: engineLayer as ui.BackdropFilterEngineLayer?,
    );
    addChildrenToScene(builder);
    builder.pop();
  }

  @override
  void dispose() {
    prepare = null;
    super.dispose();
  }
}
