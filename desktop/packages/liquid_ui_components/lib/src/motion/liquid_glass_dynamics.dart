import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/physics.dart';

/// Paint-only liquid response for an existing control or glass surface.
///
/// A Listener observes input without competing with the child's gestures.
/// The child keeps its layout, focus, semantics and native action handling.
/// [pressed] and [dragOffset] can instead drive a visual-only thumb or keyboard
/// press. Idle controls have no active ticker. Reduced motion disables effects.
class LiquidGlassDynamics extends StatefulWidget {
  const LiquidGlassDynamics({
    super.key,
    required this.child,
    this.enabled = true,
    this.intensity = 1,
    this.pressed,
    this.dragOffset = Offset.zero,
    this.trackDrag = false,
    this.displacement = 2.5,
    this.borderRadius = const BorderRadius.all(Radius.circular(28)),
  }) : assert(intensity >= 0 && intensity <= 1),
       assert(displacement >= 0 && displacement <= 8);

  final Widget child;
  final bool enabled;
  final double intensity;
  final bool? pressed;

  /// Normalized physical drag direction, clamped to [-1, 1] per axis.
  final Offset dragOffset;

  /// Keep reacting to pointer movement beyond touch slop. Leave off for buttons
  /// inside scroll views so scrolling cancels their decorative press response.
  final bool trackDrag;

  /// Maximum translation in logical pixels; zero keeps a slider thumb locked
  /// to its value while retaining directional deformation.
  final double displacement;
  final BorderRadius borderRadius;

  @override
  State<LiquidGlassDynamics> createState() => _LiquidGlassDynamicsState();
}

class _LiquidGlassDynamicsState extends State<LiquidGlassDynamics>
    with TickerProviderStateMixin {
  late final _press = AnimationController.unbounded(vsync: this);
  late final _x = AnimationController.unbounded(vsync: this);
  late final _y = AnimationController.unbounded(vsync: this);
  late final _motion = Listenable.merge([_press, _x, _y]);
  int? _pointer;
  Offset _origin = Offset.zero;
  Alignment _light = Alignment.center;
  bool _reduceMotion = false;
  bool _reactivated = false;
  bool get _enabled => widget.enabled && widget.intensity > 0 && !_reduceMotion;

  void _spring(AnimationController controller, double target) {
    if (controller.value == target && !controller.isAnimating) return;
    controller.animateWith(
      SpringSimulation(
        // A softer return preserves momentum instead of stopping abruptly.
        SpringDescription(
          mass: 1,
          stiffness: target == 0 ? 260 : 420,
          damping: target == 0 ? 21 : 29,
        ),
        controller.value,
        target,
        controller.velocity,
        snapToEnd: true,
        tolerance: const Tolerance(distance: 0.001, velocity: 0.01),
      ),
    );
  }

  void _reset() {
    _pointer = null;
    _press.value = 0;
    _x.value = 0;
    _y.value = 0;
  }

  void _externalPress() {
    if (!_enabled) {
      _reset();
    } else if (widget.pressed != null) {
      _spring(_press, widget.pressed! ? 1 : 0);
      _spring(_x, widget.pressed! ? widget.dragOffset.dx.clamp(-1.0, 1.0) : 0);
      _spring(_y, widget.pressed! ? widget.dragOffset.dy.clamp(-1.0, 1.0) : 0);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _reduceMotion = MediaQuery.disableAnimationsOf(context);
    _externalPress();
  }

  @override
  void didUpdateWidget(LiquidGlassDynamics oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.enabled != widget.enabled ||
        oldWidget.intensity != widget.intensity ||
        oldWidget.pressed != widget.pressed ||
        oldWidget.dragOffset != widget.dragOffset) {
      _externalPress();
    }
  }

  void _follow(Offset position) {
    final box = context.findRenderObject()! as RenderBox;
    final width = box.size.width;
    final height = box.size.height;
    _light = Alignment(
      width == 0 ? 0 : (position.dx / width * 2 - 1).clamp(-1.0, 1.0),
      height == 0 ? 0 : (position.dy / height * 2 - 1).clamp(-1.0, 1.0),
    );
    final travel = position - _origin;
    _spring(_x, (_light.x * 0.35 + travel.dx / 50).clamp(-1.0, 1.0));
    _spring(_y, (_light.y * 0.35 + travel.dy / 50).clamp(-1.0, 1.0));
  }

  void _down(PointerDownEvent event) {
    if (!_enabled || _pointer != null) return;
    _pointer = event.pointer;
    _origin = event.localPosition;
    _follow(event.localPosition);
    _spring(_press, 1);
  }

  void _move(PointerMoveEvent event) {
    if (event.pointer != _pointer) return;
    if (!widget.trackDrag &&
        (event.localPosition - _origin).distance > kTouchSlop) {
      _release(event);
      return;
    }
    _follow(event.localPosition);
  }

  void _release(PointerEvent event) {
    if (event.pointer != _pointer) return;
    _pointer = null;
    _spring(_press, 0);
    _spring(_x, 0);
    _spring(_y, 0);
  }

  @override
  void deactivate() {
    _pointer = null;
    _press.stop();
    _x.stop();
    _y.stop();
    super.deactivate();
  }

  @override
  void activate() {
    super.activate();
    _reactivated = true;
  }

  @override
  void dispose() {
    _press.dispose();
    _x.dispose();
    _y.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_reactivated) {
      _reactivated = false;
      _reset();
      _externalPress();
    }
    return Listener(
      onPointerDown: _down,
      onPointerMove: _move,
      onPointerUp: _release,
      onPointerCancel: _release,
      child: AnimatedBuilder(
        animation: _motion,
        // Keep the glass lens and control out of the per-frame build path.
        child: RepaintBoundary(child: widget.child),
        builder: (context, child) {
          final strength = _enabled ? widget.intensity : 0.0;
          final press = _press.value.clamp(-0.15, 1.15) * strength;
          final dx = _x.value * strength;
          final dy = _y.value * strength;
          final vx = (_x.velocity * strength / 24).clamp(-1.0, 1.0);
          final vy = (_y.velocity * strength / 24).clamp(-1.0, 1.0);
          final stretch =
              (dx.abs() - dy.abs()) * 0.045 + (vx.abs() - vy.abs()) * 0.025;
          final scaleX = 1 + press * 0.045 + stretch;
          return Transform.translate(
            offset: Offset(dx * widget.displacement, dy * widget.displacement),
            child: Transform(
              key: const ValueKey('liquid-dynamics-transform'),
              alignment: Alignment.center,
              transform: Matrix4.diagonal3Values(scaleX, 1 / scaleX, 1),
              child: Stack(
                clipBehavior: Clip.none,
                children: [
                  child!,
                  Positioned.fill(
                    child: IgnorePointer(
                      child: ExcludeSemantics(
                        child: ClipRRect(
                          borderRadius: widget.borderRadius,
                          child: CustomPaint(
                            painter: _LiquidTouchLight(
                              light: _light,
                              amount: press.clamp(0.0, 1.0),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

class _LiquidTouchLight extends CustomPainter {
  const _LiquidTouchLight({required this.light, required this.amount});
  final Alignment light;
  final double amount;

  @override
  void paint(Canvas canvas, Size size) {
    if (amount <= 0 || size.isEmpty) return;
    final rect = Offset.zero & size;
    canvas.drawRect(
      rect,
      Paint()
        ..shader = RadialGradient(
          center: light,
          radius: 0.85,
          colors: [
            Colors.white.withValues(alpha: 0.20 * amount),
            Colors.transparent,
          ],
        ).createShader(rect),
    );
  }

  @override
  bool shouldRepaint(_LiquidTouchLight oldDelegate) =>
      light != oldDelegate.light || amount != oldDelegate.amount;
}
