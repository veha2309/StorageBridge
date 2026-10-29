import 'package:flutter/material.dart';

import '../surfaces/liquid_glass_surface.dart';
import '../motion/liquid_glass_dynamics.dart';

/// A controlled glass slider with a filled capsule and an optical thumb.
/// Flutter's Slider owns gestures, focus, keyboard and screen-reader actions.
class LiquidGlassSlider extends StatefulWidget {
  const LiquidGlassSlider({
    super.key,
    required this.value,
    required this.onChanged,
    this.min = 0,
    this.max = 1,
    this.divisions,
    this.label,
    this.semanticLabel,
    this.onChangeStart,
    this.onChangeEnd,
    this.settings,
    this.height = 48,
    this.activeColor,
    this.thumbIcon,
  }) : assert(min < max),
       assert(min > -double.infinity && max < double.infinity),
       assert(value >= min && value <= max),
       assert(divisions == null || divisions > 0),
       assert(height >= 48 && height < double.infinity);

  final double value;
  final double min;
  final double max;
  final int? divisions;
  final String? label;
  final String? semanticLabel;
  final ValueChanged<double>? onChanged;
  final ValueChanged<double>? onChangeStart;
  final ValueChanged<double>? onChangeEnd;
  final LiquidGlassSettings? settings;
  final double height;
  final Color? activeColor;

  /// Optional glyph inside the thumb, e.g. a speaker for volume.
  final IconData? thumbIcon;

  @override
  State<LiquidGlassSlider> createState() => _LiquidGlassSliderState();
}

class _LiquidGlassSliderState extends State<LiquidGlassSlider> {
  bool _dragging = false;
  double _dragDelta = 0;

  @override
  void didUpdateWidget(LiquidGlassSlider oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.onChanged == null) {
      _dragging = false;
      _dragDelta = 0;
    } else if (_dragging) {
      _dragDelta =
          ((widget.value - oldWidget.value) / (widget.max - widget.min) * 18)
              .clamp(-1.0, 1.0);
    }
  }

  @override
  Widget build(BuildContext context) {
    final settings = widget.settings ?? LiquidGlassTheme.of(context);
    final reduceMotion = MediaQuery.disableAnimationsOf(context);
    final enabled = widget.onChanged != null;
    final accent = widget.activeColor ?? Theme.of(context).colorScheme.primary;
    final highContrast = MediaQuery.highContrastOf(context);
    final duration = reduceMotion ? Duration.zero : settings.animationDuration;
    return LayoutBuilder(
      builder: (context, constraints) {
        final width = constraints.hasBoundedWidth
            ? constraints.maxWidth
            : 200.0;
        final inset = width >= 64 ? 32.0 : width / 2;
        final travel = (width - inset * 2).clamp(0.0, double.infinity);
        final rawFraction =
            (widget.value - widget.min) / (widget.max - widget.min);
        final fraction = widget.divisions == null
            ? rawFraction
            : (rawFraction * widget.divisions!).round() / widget.divisions!;
        final rtl = Directionality.of(context) == TextDirection.rtl;
        final center = inset + travel * (rtl ? 1 - fraction : fraction);
        final lift = _dragging && !reduceMotion
            ? settings.dynamicsIntensity
            : 0.0;
        final trackHeight = (widget.height * 0.58).clamp(24.0, 36.0);
        final thumbSize = (28 + lift * 12).clamp(0.0, width);
        final thumbSettings = settings.copyWith(
          variant: LiquidGlassVariant.clear,
          rendering:
              _dragging && settings.rendering == LiquidGlassRendering.frosted
              ? LiquidGlassRendering.balanced
              : settings.rendering,
          opacity: enabled ? (_dragging ? 0.045 : 0.18) : 0.10,
          blur: 0,
          depth: (settings.depth + 0.02 + 0.025 * lift).clamp(0.0, 1.0),
          // Keep the backdrop locked to the finger; deepen the bevel on press.
          magnification: 1 + 0.18 * lift,
          shadowOpacity: enabled ? 0.16 : 0.04,
          shadowBlur: 8 + 8 * lift,
        );
        return SizedBox(
          width: width,
          height: widget.height,
          child: Stack(
            children: [
              Positioned(
                left: 0,
                right: 0,
                top: (widget.height - trackHeight) / 2,
                height: trackHeight,
                child: LiquidGlassSurface(
                  key: const ValueKey('glass-slider-track'),
                  settings:
                      (settings.rendering == LiquidGlassRendering.automatic ||
                          settings.rendering == LiquidGlassRendering.balanced)
                      ? settings.copyWith(
                          rendering: LiquidGlassRendering.frosted,
                        )
                      : settings,
                  borderRadius: BorderRadius.circular(trackHeight / 2),
                  child: const SizedBox.expand(),
                ),
              ),
              Positioned(
                left: 0,
                right: 0,
                top: (widget.height - trackHeight) / 2,
                height: trackHeight,
                child: IgnorePointer(
                  child: ExcludeSemantics(
                    child: Padding(
                      padding: const EdgeInsets.all(5),
                      child: Align(
                        alignment: AlignmentDirectional.centerStart,
                        child: FractionallySizedBox(
                          widthFactor: width == 0
                              ? 0
                              : ((inset +
                                            travel * fraction +
                                            thumbSize / 2 -
                                            5) /
                                        (width - 10))
                                    .clamp(0.0, 1.0),
                          heightFactor: 1,
                          child: DecoratedBox(
                            key: const ValueKey('glass-slider-fill'),
                            decoration: BoxDecoration(
                              borderRadius: BorderRadius.circular(
                                widget.height / 2,
                              ),
                              gradient: LinearGradient(
                                begin: AlignmentDirectional.centerStart,
                                end: AlignmentDirectional.centerEnd,
                                colors: [
                                  accent.withValues(
                                    alpha: enabled
                                        ? (highContrast ? 0.55 : 0.32)
                                        : 0.08,
                                  ),
                                  accent.withValues(
                                    alpha: enabled
                                        ? (highContrast ? 0.75 : 0.62)
                                        : 0.14,
                                  ),
                                ],
                              ),
                              border: Border.all(
                                color: Colors.white.withValues(
                                  alpha: enabled ? 0.24 : 0.08,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              Positioned(
                left: center - (widget.height - 4) / 2,
                top: 2,
                width: widget.height - 4,
                height: widget.height - 4,
                child: IgnorePointer(
                  child: ExcludeSemantics(
                    child: Center(
                      child: AnimatedContainer(
                        duration: duration,
                        curve: Curves.easeOutCubic,
                        width: thumbSize,
                        height: thumbSize,
                        child: LiquidGlassDynamics(
                          enabled: enabled,
                          pressed: _dragging,
                          displacement: 0,
                          intensity: settings.dynamicsIntensity,
                          dragOffset: Offset(_dragDelta * (rtl ? -1 : 1), 0),
                          borderRadius: BorderRadius.circular(thumbSize / 2),
                          child: BackdropGroup(
                            child: LiquidGlassSurface(
                              key: const ValueKey('glass-slider-thumb'),
                              settings: thumbSettings,
                              borderRadius: BorderRadius.circular(
                                thumbSize / 2,
                              ),
                              child: Center(
                                child: widget.thumbIcon != null
                                    ? Icon(
                                        widget.thumbIcon,
                                        size: 22,
                                        color: enabled
                                            ? null
                                            : Theme.of(context).disabledColor,
                                      )
                                    : Row(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                          for (var line = 0; line < 2; line++)
                                            Container(
                                              width: 2,
                                              height: 14,
                                              margin:
                                                  const EdgeInsets.symmetric(
                                                    horizontal: 2,
                                                  ),
                                              decoration: BoxDecoration(
                                                color: Theme.of(context)
                                                    .colorScheme
                                                    .onSurface
                                                    .withValues(
                                                      alpha: enabled
                                                          ? 0.55
                                                          : 0.2,
                                                    ),
                                                borderRadius:
                                                    BorderRadius.circular(2),
                                              ),
                                            ),
                                        ],
                                      ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              Positioned.fill(
                child: Semantics(
                  label: widget.semanticLabel,
                  child: SliderTheme(
                    data: SliderTheme.of(context).copyWith(
                      trackShape: const _GlassTrackShape(),
                      trackHeight: 4,
                      activeTrackColor: Colors.transparent,
                      inactiveTrackColor: Colors.transparent,
                      disabledActiveTrackColor: Colors.transparent,
                      disabledInactiveTrackColor: Colors.transparent,
                      activeTickMarkColor: Colors.transparent,
                      inactiveTickMarkColor: Colors.transparent,
                      disabledActiveTickMarkColor: Colors.transparent,
                      disabledInactiveTickMarkColor: Colors.transparent,
                      thumbShape: const RoundSliderThumbShape(
                        enabledThumbRadius: 0,
                        disabledThumbRadius: 0,
                        elevation: 0,
                        pressedElevation: 0,
                      ),
                      overlayColor: Colors.transparent,
                      overlayShape: const RoundSliderOverlayShape(
                        overlayRadius: 29,
                      ),
                    ),
                    child: Slider(
                      overlayColor: WidgetStateProperty.resolveWith(
                        (states) => states.contains(WidgetState.focused)
                            ? accent.withValues(alpha: 0.12)
                            : Colors.transparent,
                      ),
                      value: widget.value,
                      min: widget.min,
                      max: widget.max,
                      divisions: widget.divisions,
                      label: widget.label,
                      padding: EdgeInsets.zero,
                      onChanged: widget.onChanged,
                      onChangeStart: enabled
                          ? (value) {
                              setState(() {
                                _dragging = true;
                                _dragDelta = 0;
                              });
                              widget.onChangeStart?.call(value);
                            }
                          : null,
                      onChangeEnd: enabled
                          ? (value) {
                              setState(() {
                                _dragging = false;
                                _dragDelta = 0;
                              });
                              widget.onChangeEnd?.call(value);
                            }
                          : null,
                    ),
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// One geometry for both native hit testing and the visible glass thumb.
class _GlassTrackShape extends RoundedRectSliderTrackShape {
  const _GlassTrackShape();

  @override
  Rect getPreferredRect({
    required RenderBox parentBox,
    Offset offset = Offset.zero,
    required SliderThemeData sliderTheme,
    bool isEnabled = false,
    bool isDiscrete = false,
  }) {
    final inset = parentBox.size.width >= 64 ? 32.0 : parentBox.size.width / 2;
    return Rect.fromLTWH(
      offset.dx + inset,
      offset.dy + (parentBox.size.height - 4) / 2,
      (parentBox.size.width - inset * 2).clamp(0.0, double.infinity),
      4,
    );
  }
}
