import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/physics.dart';

import '../surfaces/liquid_glass_surface.dart';

part 'morph_geometry.dart';

/// The spatial character of a glass transition.
enum LiquidGlassMorphMotion { droplet, jelly, merge }

/// A single glass lens that expands from a compact control into a panel.
///
/// The host owns [expanded] and supplies actions in both children. Place in an
/// Align or Stack with loose constraints to allow the surface to change size.
/// By default this widget participates in layout. Set [overlay] to expand over
/// surrounding content while retaining only the compact layout footprint.
/// Children retain their state, but only the settled, visible
/// target child accepts input, focus and accessibility traversal once revealed.
class LiquidGlassMorph extends StatefulWidget {
  LiquidGlassMorph({
    super.key,
    required this.expanded,
    required this.collapsedSize,
    required this.expandedSize,
    required this.collapsedChild,
    required this.expandedChild,
    this.onExpandedChanged,
    this.overlay = false,
    this.dragHoldDuration = const Duration(milliseconds: 120),
    this.motion = LiquidGlassMorphMotion.droplet,
    this.sharedChild,
    this.collapsedSharedAlignment = AlignmentDirectional.center,
    this.expandedSharedAlignment = AlignmentDirectional.topStart,
    this.collapsedSharedPadding = EdgeInsets.zero,
    this.expandedSharedPadding = const EdgeInsets.all(16),
    this.collapsedBorderRadius = const BorderRadius.all(Radius.circular(28)),
    this.expandedBorderRadius = const BorderRadius.all(Radius.circular(32)),
    this.alignment = AlignmentDirectional.topStart,
    this.settings,
    this.expandedSettings,
    this.duration = const Duration(milliseconds: 380),
    this.curve = Curves.linear,
  }) : assert(collapsedSize.width > 0 && collapsedSize.width < double.infinity),
       assert(
         collapsedSize.height > 0 && collapsedSize.height < double.infinity,
       ),
       assert(expandedSize.width > 0 && expandedSize.width < double.infinity),
       assert(expandedSize.height > 0 && expandedSize.height < double.infinity),
       assert(!duration.isNegative),
       assert(!dragHoldDuration.isNegative);

  final bool expanded;
  final Size collapsedSize;
  final Size expandedSize;
  final Widget collapsedChild;
  final Widget expandedChild;

  /// Paint the lens in the nearest Overlay, anchored to the compact control.
  /// Expansion does not push siblings or get clipped by a parent scroll view.
  /// Only the glass intercepts input; outside taps pass through. The host still
  /// controls dismissal through [expanded]. Requires an ancestor Overlay.
  final bool overlay;

  /// Brief hold before a vertical pull owns the gesture. Ordinary swipes can
  /// still scroll before this deadline. Defaults to 120 ms.
  final Duration dragHoldDuration;
  final LiquidGlassMorphMotion motion;

  /// Enables hold-and-slide interaction on the compact control and panel
  /// header. Drag down to open, up to close, then release to settle. The host
  /// must update [expanded] in response. Panel scrolling stays independent.
  final ValueChanged<bool>? onExpandedChanged;

  /// Content rendered once above both states, such as a title that travels
  /// from the button to the panel header without fading or being duplicated.
  /// Reserve its space in both children. Pointer events pass through to the
  /// state-specific controls beneath it; its semantics remain available.
  final Widget? sharedChild;
  final AlignmentGeometry collapsedSharedAlignment;
  final AlignmentGeometry expandedSharedAlignment;
  final EdgeInsetsGeometry collapsedSharedPadding;
  final EdgeInsetsGeometry expandedSharedPadding;
  final BorderRadius collapsedBorderRadius;
  final BorderRadius expandedBorderRadius;
  final AlignmentGeometry alignment;

  /// Compact material, defaulting to the surrounding [LiquidGlassTheme].
  final LiquidGlassSettings? settings;

  /// Expanded material. Defaults to slightly deeper glass with the same blur.
  ///
  /// Optical scalar values interpolate. Rendering mode, variant and
  /// accessibility preferences use the target configuration immediately.
  final LiquidGlassSettings? expandedSettings;

  /// Perceptual spring duration; the small settling tail can last longer.
  /// Zero switches states immediately.
  final Duration duration;

  /// Optional remapping of the spring's in-range progress. Linear preserves
  /// the spring's natural velocity and is recommended for fluid motion.
  final Curve curve;

  @override
  State<LiquidGlassMorph> createState() => _LiquidGlassMorphState();
}

class _LiquidGlassMorphState extends State<LiquidGlassMorph>
    with TickerProviderStateMixin {
  late final AnimationController _controller = AnimationController.unbounded(
    vsync: this,
    duration: widget.duration,
    value: widget.expanded ? 1 : 0,
  );
  late final AnimationController _body = AnimationController.unbounded(
    vsync: this,
    value: widget.expanded ? 1 : 0,
  );
  late final Listenable _motion = Listenable.merge([_controller, _body]);
  double? _dragStart;
  double _dragProgress = 0;
  double _dragBodyProgress = 0;
  double _dragExtent = 1;
  final _overlayController = OverlayPortalController()..show();

  void _endDrag({bool cancel = false, double velocity = 0}) {
    if (_dragStart == null) return;
    _dragStart = null;
    final target = cancel ? widget.expanded : _controller.value >= 0.5;
    if (target != widget.expanded) widget.onExpandedChanged?.call(target);
    _animateTo(target ? 1 : 0, velocity: cancel ? 0 : velocity);
  }

  void _animate() {
    if (_dragStart != null) return;
    _animateTo(widget.expanded ? 1 : 0);
  }

  void _animateTo(double target, {double? velocity}) {
    if (_controller.value == target &&
        !_controller.isAnimating &&
        _body.value == target &&
        !_body.isAnimating) {
      return;
    }
    if (MediaQuery.disableAnimationsOf(context) ||
        widget.duration == Duration.zero) {
      _controller.value = target;
      _body.value = target;
    } else {
      final springVelocity = velocity ?? _controller.velocity;
      _controller.animateWith(
        SpringSimulation(
          SpringDescription.withDurationAndBounce(
            duration: widget.duration,
            bounce: widget.motion == LiquidGlassMorphMotion.jelly
                ? 0.36
                : 0.065,
          ),
          _controller.value,
          target,
          springVelocity,
          snapToEnd: true,
          tolerance: const Tolerance(distance: 0.002, velocity: 0.02),
        ),
      );
      // The leading axis pulls ahead; the heavier body follows on its own
      // spring. Each retains momentum when retargeted, so reversing does not
      // switch a curve or abruptly flip the silhouette.
      _body.animateWith(
        SpringSimulation(
          SpringDescription.withDurationAndBounce(
            duration: Duration(
              microseconds:
                  (widget.duration.inMicroseconds *
                          (widget.motion == LiquidGlassMorphMotion.jelly
                              ? 1.5
                              : 1.22))
                      .round(),
            ),
            bounce: widget.motion == LiquidGlassMorphMotion.jelly ? 0.40 : 0.09,
          ),
          _body.value,
          target,
          velocity ?? _body.velocity,
          snapToEnd: true,
          tolerance: const Tolerance(distance: 0.002, velocity: 0.02),
        ),
      );
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _animate();
  }

  @override
  void didUpdateWidget(LiquidGlassMorph oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.expanded != widget.expanded ||
        oldWidget.duration != widget.duration) {
      _animate();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _body.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final compact = widget.settings ?? LiquidGlassTheme.of(context);
    final panel =
        widget.expandedSettings ??
        compact.copyWith(
          depth: (compact.depth * 1.35).clamp(0.0, 1.0),
          shadowOpacity: (compact.shadowOpacity + 0.03).clamp(0.0, 1.0),
        );
    final alignment = widget.alignment.resolve(Directionality.of(context));
    final colors = Theme.of(context).colorScheme;
    return LayoutBuilder(
      builder: (context, constraints) {
        final small = constraints.constrain(widget.collapsedSize);
        final lensConstraints = widget.overlay
            ? constraints.copyWith(
                minWidth: 0,
                minHeight: 0,
                maxHeight: double.infinity,
              )
            : constraints;
        final large = lensConstraints.constrain(widget.expandedSize);
        final animatedLens = AnimatedBuilder(
          animation: _motion,
          builder: (context, _) {
            final progress = _controller.value;
            final t = widget.curve
                .transform(progress.clamp(0.0, 1.0))
                .clamp(0.0, 1.0);
            final flow = math.sin(math.pi * t);
            final overshoot = progress - progress.clamp(0.0, 1.0);
            final bodyProgress = _body.value;
            final bodyT = widget.curve.transform(bodyProgress.clamp(0.0, 1.0));
            final across = t + overshoot;
            final down = bodyT + bodyProgress - bodyProgress.clamp(0.0, 1.0);
            final width = (small.width + (large.width - small.width) * across)
                .clamp(1.0, double.infinity);
            final height = (small.height + (large.height - small.height) * down)
                .clamp(1.0, double.infinity);
            final size = lensConstraints.constrain(Size(width, height));
            final energy =
                (_controller.velocity.abs() / 10).clamp(0.0, 1.0) * flow;
            double mix(double a, double b) => a + (b - a) * t;
            final target = widget.expanded ? panel : compact;
            final material = target.copyWith(
              tintColor: Color.lerp(
                compact.tintColor ?? colors.surface,
                panel.tintColor ?? colors.surface,
                t,
              ),
              foregroundColor: Color.lerp(
                compact.foregroundColor ?? colors.onSurface,
                panel.foregroundColor ?? colors.onSurface,
                t,
              ),
              opacity: mix(compact.opacity, panel.opacity),
              blur: mix(compact.blur, panel.blur),
              saturation: mix(compact.saturation, panel.saturation),
              refraction: mix(compact.refraction, panel.refraction),
              depth: mix(compact.depth, panel.depth),
              magnification: mix(compact.magnification, panel.magnification),
              chromaticAberration: mix(
                compact.chromaticAberration,
                panel.chromaticAberration,
              ),
              borderWidth: mix(compact.borderWidth, panel.borderWidth),
              lightIntensity:
                  mix(compact.lightIntensity, panel.lightIntensity) +
                  energy * 0.12,
              lightDirection:
                  compact.lightDirection +
                  ((panel.lightDirection - compact.lightDirection + 540) % 360 -
                          180) *
                      t,
              refractionWidth: mix(
                compact.refractionWidth,
                panel.refractionWidth,
              ),
              edgeAmbientIntensity: mix(
                compact.edgeAmbientIntensity,
                panel.edgeAmbientIntensity,
              ),
              edgeLightSpread: mix(
                compact.edgeLightSpread,
                panel.edgeLightSpread,
              ),
              shadowBlur: mix(compact.shadowBlur, panel.shadowBlur),
              shadowOpacity: mix(compact.shadowOpacity, panel.shadowOpacity),
              // Geometry and shadow share this controller, without a second
              // implicit animation trailing behind the optical lens.
              animationDuration: Duration.zero,
            );
            Widget content(
              Widget child,
              Size size,
              double opacity,
              bool active,
              double scale,
              double offset,
            ) {
              return Positioned.fill(
                child: IgnorePointer(
                  ignoring: !active,
                  child: ExcludeFocus(
                    excluding: !active,
                    child: ExcludeSemantics(
                      excluding: !active,
                      child: Offstage(
                        offstage: opacity == 0,
                        child: TickerMode(
                          enabled: opacity > 0,
                          child: OverflowBox(
                            alignment: alignment,
                            minWidth: size.width,
                            maxWidth: size.width,
                            minHeight: size.height,
                            maxHeight: size.height,
                            child: Opacity(
                              opacity: opacity,
                              child: Transform.translate(
                                offset: Offset(0, offset),
                                child: Transform.scale(
                                  alignment: alignment,
                                  scale: scale,
                                  child: RepaintBoundary(child: child),
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              );
            }

            final lens = RepaintBoundary(
              child: LiquidGlassSurface(
                settings: material,
                shape: _FluidGlassBorder(
                  borderRadius: BorderRadius.lerp(
                    widget.collapsedBorderRadius,
                    widget.expandedBorderRadius,
                    t,
                  )!,
                  fullness:
                      (math.sin(math.pi * math.min(t, bodyT)) +
                              (t - bodyT).abs())
                          .clamp(0.0, 1.0),
                  direction: alignment,
                  motion: widget.motion,
                ),
                borderRadius:
                    BorderRadius.lerp(
                      widget.collapsedBorderRadius,
                      widget.expandedBorderRadius,
                      t,
                    )! +
                    BorderRadius.circular(energy * 8),
                child: SizedBox(
                  width: size.width,
                  height: size.height,
                  child: Stack(
                    clipBehavior: Clip.none,
                    children: [
                      content(
                        widget.collapsedChild,
                        small,
                        (1 - t / 0.65).clamp(0.0, 1.0),
                        !widget.expanded && t <= 0.15,
                        1 - 0.06 * t,
                        -4 * t,
                      ),
                      content(
                        widget.expandedChild,
                        large,
                        ((t - 0.15) / 0.85).clamp(0.0, 1.0),
                        widget.expanded && t >= 0.85,
                        0.94 + 0.06 * t,
                        10 * (1 - t),
                      ),
                      if (widget.sharedChild != null)
                        Positioned.fill(
                          child: IgnorePointer(
                            child: CustomSingleChildLayout(
                              delegate: _MorphSharedLayout(
                                progress: t,
                                small: small,
                                large: large,
                                startAlignment: widget.collapsedSharedAlignment
                                    .resolve(Directionality.of(context)),
                                endAlignment: widget.expandedSharedAlignment
                                    .resolve(Directionality.of(context)),
                                startPadding: widget.collapsedSharedPadding
                                    .resolve(Directionality.of(context)),
                                endPadding: widget.expandedSharedPadding
                                    .resolve(Directionality.of(context)),
                              ),
                              child: RepaintBoundary(child: widget.sharedChild),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            );
            if (widget.onExpandedChanged == null) return lens;
            return RawGestureDetector(
              behavior: HitTestBehavior.translucent,
              gestures: {
                LongPressGestureRecognizer:
                    GestureRecognizerFactoryWithHandlers<
                      LongPressGestureRecognizer
                    >(
                      () => LongPressGestureRecognizer(
                        duration: widget.dragHoldDuration,
                      ),
                      (recognizer) {
                        recognizer.onLongPressStart = (details) {
                          if (details.localPosition.dy > small.height) return;
                          _dragStart = details.globalPosition.dy;
                          _dragProgress = _controller.value.clamp(0.0, 1.0);
                          _dragBodyProgress = _body.value.clamp(0.0, 1.0);
                          _dragExtent = math.max(
                            80,
                            large.height - small.height,
                          );
                          _controller.stop();
                          _body.stop();
                        };
                        recognizer.onLongPressMoveUpdate = (details) {
                          final start = _dragStart;
                          if (start == null) return;
                          _controller.value =
                              (_dragProgress +
                                      (details.globalPosition.dy - start) /
                                          _dragExtent)
                                  .clamp(0.0, 1.0);
                          _body.value =
                              (_dragBodyProgress +
                                      (details.globalPosition.dy - start) /
                                          _dragExtent)
                                  .clamp(0.0, 1.0);
                        };
                        recognizer.onLongPressEnd = (details) => _endDrag(
                          velocity:
                              (details.velocity.pixelsPerSecond.dy /
                                      _dragExtent)
                                  .clamp(-8.0, 8.0),
                        );
                        recognizer.onLongPressCancel = () =>
                            _endDrag(cancel: true);
                      },
                    ),
              },
              child: lens,
            );
          },
        );
        if (!widget.overlay) return animatedLens;
        return OverlayPortal.overlayChildLayoutBuilder(
          controller: _overlayController,
          overlayChildBuilder: (context, info) {
            final origin = MatrixUtils.transformPoint(
              info.childPaintTransform,
              Offset.zero,
            );
            return Positioned(
              left: origin.dx,
              top: origin.dy,
              child: animatedLens,
            );
          },
          child: SizedBox.fromSize(size: small),
        );
      },
    );
  }
}
