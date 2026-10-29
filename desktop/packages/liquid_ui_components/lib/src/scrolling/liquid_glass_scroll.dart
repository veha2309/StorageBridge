import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

/// Keeps optical outlines stable at scroll limits without stretching their
/// screen-space shader independently of the widget's layout bounds.
///
/// Use on MaterialApp.scrollBehavior or a local ScrollConfiguration. This keeps
/// platform scroll physics and input devices, but removes the edge decoration.
class LiquidGlassScrollBehavior extends MaterialScrollBehavior {
  const LiquidGlassScrollBehavior();

  @override
  Widget buildOverscrollIndicator(
    BuildContext context,
    Widget child,
    ScrollableDetails details,
  ) => child;
}

/// Invalidates the optical subtree BEFORE paint when any enclosing scrollable
/// moves. The renderer's scene-time transform tracker otherwise corrects a
/// retained lens one frame late. No per-frame ticker or widget rebuild is used.
class ScrollAwareGlass extends StatefulWidget {
  const ScrollAwareGlass({super.key, required this.child});

  final Widget child;

  @override
  State<ScrollAwareGlass> createState() => _ScrollAwareGlassState();
}

class _ScrollAwareGlassState extends State<ScrollAwareGlass> {
  List<ScrollPosition> _positions = const [];

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Register inherited scroll dependencies as well as finding nested axes.
    Scrollable.maybeOf(context);
    Scrollable.maybeOf(context, axis: Axis.horizontal);
    Scrollable.maybeOf(context, axis: Axis.vertical);
    final positions = <ScrollPosition>{};
    context.visitAncestorElements((element) {
      if (element is StatefulElement && element.state is ScrollableState) {
        positions.add((element.state as ScrollableState).position);
      }
      return true;
    });
    _positions = positions.toList(growable: false);
  }

  @override
  Widget build(BuildContext context) =>
      _ScrollPaintInvalidator(positions: _positions, child: widget.child);
}

class _ScrollPaintInvalidator extends SingleChildRenderObjectWidget {
  const _ScrollPaintInvalidator({
    required this.positions,
    required super.child,
  });

  final List<ScrollPosition> positions;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderScrollPaintInvalidator(positions);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderScrollPaintInvalidator renderObject,
  ) => renderObject.positions = positions;
}

class _RenderScrollPaintInvalidator extends RenderProxyBox {
  _RenderScrollPaintInvalidator(this._positions);

  List<ScrollPosition> _positions;

  set positions(List<ScrollPosition> value) {
    if (attached) _unsubscribe();
    _positions = value;
    if (attached) _subscribe();
  }

  void _subscribe() {
    for (final position in _positions) {
      position.addListener(_invalidate);
    }
  }

  void _unsubscribe() {
    for (final position in _positions) {
      position.removeListener(_invalidate);
    }
  }

  void _invalidate() {
    // ListView retains each item's layers. Marking only the parent does not
    // repaint a nested lens behind a repaint boundary, so visit its subtree.
    void repaint(RenderObject node) {
      node.markNeedsPaint();
      node.visitChildren(repaint);
    }

    repaint(this);
  }

  @override
  void attach(PipelineOwner owner) {
    super.attach(owner);
    _subscribe();
  }

  @override
  void detach() {
    _unsubscribe();
    super.detach();
  }
}
