part of 'liquid_glass_navigation.dart';

class _AndroidGlassBar extends StatefulWidget {
  const _AndroidGlassBar({
    required this.items,
    required this.selectedIndex,
    required this.onTap,
    required this.tintColor,
    required this.unselectedColor,
    required this.surfaceColor,
    this.onScrubChanged,
    this.glassSettings,
    this.scrollController,
    required this.shrinkOnScroll,
    required this.collapseThreshold,
    required this.expandThreshold,
  });

  final List<LiquidGlassNavigationItem> items;
  final int selectedIndex;
  final ValueChanged<int> onTap;
  final Color tintColor;
  final Color unselectedColor;
  final Color surfaceColor;
  final ValueChanged<double?>? onScrubChanged;
  final LiquidGlassSettings? glassSettings;
  final ScrollController? scrollController;
  final bool shrinkOnScroll;
  final double collapseThreshold;
  final double expandThreshold;

  @override
  State<_AndroidGlassBar> createState() => _AndroidGlassBarState();
}

class _AndroidGlassBarState extends State<_AndroidGlassBar>
    with TickerProviderStateMixin {
  late final AnimationController _position = AnimationController.unbounded(
    vsync: this,
    value: widget.selectedIndex.toDouble(),
  );
  late final AnimationController _lift = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
    reverseDuration: const Duration(milliseconds: 280),
  );
  late final _compact = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 260),
    reverseDuration: const Duration(milliseconds: 200),
  );
  double? _lastScroll;
  double? _lastViewport;
  double _scrollTravel = 0;
  bool _compactTarget = false;
  bool _allowShrink = true;
  int? _tapIndex;
  double get _compactProgress => Curves.easeOutCubic.transform(_compact.value);

  @override
  void initState() {
    super.initState();
    _compact.addStatusListener((status) {
      if (status == AnimationStatus.completed ||
          status == AnimationStatus.dismissed) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) _syncScrollMetrics();
        });
      }
    });
    widget.scrollController?.addListener(_onScroll);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _onScroll();
    });
  }

  void _syncScrollMetrics() {
    final controller = widget.scrollController;
    if (controller == null ||
        !controller.hasClients ||
        controller.positions.length != 1) {
      return;
    }
    final position = controller.position;
    if (!position.hasContentDimensions) return;
    _lastScroll = position.pixels.clamp(
      position.minScrollExtent,
      position.maxScrollExtent,
    );
    _lastViewport = position.viewportDimension;
    _scrollTravel = 0;
  }

  void _setCompact(bool value) {
    final target =
        value &&
        _allowShrink &&
        widget.shrinkOnScroll &&
        widget.scrollController != null;
    if (_pointer != null) return;
    if (_compactTarget == target &&
        (_compact.isAnimating || _compact.value == (target ? 1 : 0))) {
      return;
    }
    _compactTarget = target;
    if (!_allowShrink) {
      _compact.value = 0;
    } else {
      _compact.animateTo(
        target ? 1 : 0,
        duration: Duration(milliseconds: target ? 260 : 200),
      );
    }
  }

  void _onScroll() {
    if (!mounted || !_active) return;
    final controller = widget.scrollController;
    if (controller == null ||
        !controller.hasClients ||
        controller.positions.length != 1) {
      _lastScroll = null;
      _setCompact(false);
      return;
    }
    final position = controller.position;
    if (!widget.shrinkOnScroll ||
        !_allowShrink ||
        position.axis != Axis.vertical ||
        !position.hasContentDimensions ||
        position.maxScrollExtent <= position.minScrollExtent) {
      _lastScroll = null;
      _scrollTravel = 0;
      _setCompact(false);
      return;
    }
    final pixels = position.pixels.clamp(
      position.minScrollExtent,
      position.maxScrollExtent,
    );
    final resized =
        _lastViewport != null && _lastViewport != position.viewportDimension;
    _lastViewport = position.viewportDimension;
    final previous = _lastScroll;
    _lastScroll = pixels;
    // Layout corrections from changing bar/keyboard height are not a new
    // scroll direction, and must not feed back into another resize.
    if (resized) {
      _scrollTravel = 0;
      return;
    }
    if (_pointer != null) return;
    final reversed = position.axisDirection == AxisDirection.up;
    if ((reversed ? position.extentAfter : position.extentBefore) <= 0) {
      _scrollTravel = 0;
      _setCompact(false);
      return;
    }
    if (previous == null) return;
    final delta = (pixels - previous) * (reversed ? -1 : 1);
    if (delta == 0) return;
    if (_scrollTravel.sign != delta.sign) _scrollTravel = 0;
    _scrollTravel += delta;
    if (_scrollTravel >= widget.collapseThreshold) {
      _setCompact(true);
      _scrollTravel = 0;
    } else if (_scrollTravel <= -widget.expandThreshold) {
      _setCompact(false);
      _scrollTravel = 0;
    }
  }

  int? _pointer;
  Offset _touch = Offset.zero;
  double _width = 1;
  double _motion = 0;
  bool _dragging = false;
  bool _cancelled = false;
  bool _active = true;
  bool _reduceMotion = false;
  bool _rtl = false;
  Timer? _holdTimer;
  bool _previewing = false;

  void _beginPreview() {
    if (!_active || _reduceMotion || widget.onScrubChanged == null) return;
    _holdTimer?.cancel();
    _previewing = true;
    widget.onScrubChanged?.call(_position.value);
  }

  void _endPreview({bool deferred = false}) {
    _holdTimer?.cancel();
    if (!_previewing) return;
    _previewing = false;
    final callback = widget.onScrubChanged;
    if (deferred) {
      WidgetsBinding.instance.addPostFrameCallback((_) => callback?.call(null));
    } else {
      callback?.call(null);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _reduceMotion = MediaQuery.disableAnimationsOf(context);
    _rtl = Directionality.of(context) == TextDirection.rtl;
    _allowShrink =
        !_reduceMotion &&
        !MediaQuery.accessibleNavigationOf(context) &&
        MediaQuery.textScalerOf(context).scale(12) <= 12 * 1.4;
    if (!_allowShrink) _setCompact(false);
  }

  @override
  void deactivate() {
    _endPreview(deferred: true);
    _active = false;
    _pointer = null;
    _dragging = false;
    _position.stop();
    _lift.stop();
    _compact.stop();
    super.deactivate();
  }

  @override
  void activate() {
    super.activate();
    _active = true;
    _lift.value = 0;
    _lastScroll = null;
    _setCompact(false);
    _settle();
  }

  @override
  void didUpdateWidget(covariant _AndroidGlassBar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.scrollController != widget.scrollController) {
      oldWidget.scrollController?.removeListener(_onScroll);
      widget.scrollController?.addListener(_onScroll);
      _lastScroll = null;
      _lastViewport = null;
      _scrollTravel = 0;
      _setCompact(false);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _onScroll();
      });
    } else if (!widget.shrinkOnScroll) {
      _scrollTravel = 0;
      _setCompact(false);
    }
    if (oldWidget.items.length != widget.items.length) {
      _endPreview(deferred: true);
      _cancelled = true;
      _pointer = null;
      _dragging = false;
      _lift.value = 0;
      _position.value = widget.selectedIndex.toDouble();
    } else if (_pointer == null &&
        oldWidget.selectedIndex != widget.selectedIndex) {
      _settle();
    }
  }

  double _positionAt(Offset local) {
    final physical = (local.dx / _width).clamp(0.0, 1.0);
    final logical = _rtl ? 1 - physical : physical;
    return (logical * widget.items.length - 0.5).clamp(
      0.0,
      widget.items.length - 1.0,
    );
  }

  void _settle() {
    if (_reduceMotion) {
      _position.value = widget.selectedIndex.toDouble();
    } else {
      _position.animateWith(
        SpringSimulation(
          const SpringDescription(mass: 1, stiffness: 380, damping: 29),
          _position.value,
          widget.selectedIndex.toDouble(),
          _position.velocity,
        ),
      );
    }
  }

  void _down(PointerDownEvent event) {
    if (_pointer != null) return;
    _cancelled = false;
    _pointer = event.pointer;
    _touch = event.localPosition;
    _tapIndex = _positionAt(_touch - const Offset(5, 0)).round();
    // Freeze geometry for this gesture so touch/drag destinations cannot move
    // underneath the finger. Restore expanded navigation when it completes.
    _compact.stop();
    _motion = 0;
    _position.stop();
    if (!_reduceMotion) {
      _lift.forward();
      _holdTimer?.cancel();
      _holdTimer = Timer(const Duration(milliseconds: 180), () {
        if (mounted && _active && _pointer != null) _beginPreview();
      });
    }
  }

  void _release(PointerEvent event) {
    if (event.pointer != _pointer) return;
    _holdTimer?.cancel();
    _pointer = null;
    _lift.reverse();
    _setCompact(false);
  }

  void _cancel() {
    if (!_active) return;
    _endPreview();
    _cancelled = true;
    _pointer = null;
    _dragging = false;
    _lift.reverse();
    _setCompact(false);
    _settle();
  }

  void _commit(int index) {
    _dragging = false;
    _pointer = null;
    _lift.reverse();
    _setCompact(false);
    widget.onTap(index);
    _endPreview();
    // Restore the controlled selection if the host declines navigation.
    _settle();
  }

  @override
  void dispose() {
    _endPreview(deferred: true);
    _position.dispose();
    _lift.dispose();
    widget.scrollController?.removeListener(_onScroll);
    _compact.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final dark = Theme.of(context).brightness == Brightness.dark;
    final highContrast = MediaQuery.highContrastOf(context);
    return AnimatedBuilder(
      animation: _compact,
      builder: (context, _) {
        final compact = _compactProgress;
        return SafeArea(
          top: false,
          child: Padding(
            padding: EdgeInsets.fromLTRB(20, 10, 20, 24 - 8 * compact),
            child: LayoutBuilder(
              builder: (context, outer) {
                final compactWidth = (widget.items.length * 56.0 + 10).clamp(
                  0.0,
                  outer.maxWidth,
                );
                return Align(
                  heightFactor: 1,
                  child: SizedBox(
                    width:
                        outer.maxWidth +
                        (compactWidth - outer.maxWidth) * compact,
                    height: 62 - 14 * compact,
                    child: LayoutBuilder(
                      builder: (context, constraints) {
                        _width = constraints.maxWidth - 10;
                        return Listener(
                          onPointerDown: (event) => _down(event),
                          onPointerUp: _release,
                          onPointerCancel: (event) {
                            if (event.pointer == _pointer) _cancel();
                          },
                          child: GestureDetector(
                            behavior: HitTestBehavior.opaque,
                            excludeFromSemantics: true,
                            onTap: () {
                              if (!_cancelled) {
                                _commit(
                                  _tapIndex ??
                                      _positionAt(
                                        _touch - const Offset(5, 0),
                                      ).round(),
                                );
                              }
                            },
                            onTapCancel: () {
                              if (_pointer == null && !_dragging) _cancel();
                            },
                            onHorizontalDragStart: (details) {
                              if (_cancelled) return;
                              _dragging = true;
                              _position.value = _positionAt(
                                details.localPosition - const Offset(5, 0),
                              );
                              _beginPreview();
                            },
                            onHorizontalDragUpdate: (details) {
                              if (_cancelled) return;
                              // Ease changes in stretch rather than snapping at every pointer event.
                              final movement = (details.delta.dx / _width * 16)
                                  .clamp(-1.0, 1.0);
                              _motion = _motion * 0.65 + movement * 0.35;
                              _position.value = _positionAt(
                                details.localPosition - const Offset(5, 0),
                              );
                              if (_previewing) {
                                widget.onScrubChanged?.call(_position.value);
                              }
                            },
                            onHorizontalDragEnd: (_) {
                              if (!_cancelled) _commit(_position.value.round());
                            },
                            onHorizontalDragCancel: () {
                              if (_dragging) _cancel();
                            },
                            child: AnimatedBuilder(
                              animation: Listenable.merge([_position, _lift]),
                              builder: (context, _) =>
                                  _scene(constraints, dark, highContrast),
                            ),
                          ),
                        );
                      },
                    ),
                  ),
                );
              },
            ),
          ),
        );
      },
    );
  }

  Widget _scene(BoxConstraints constraints, bool dark, bool highContrast) {
    final position = _position.value.clamp(0.0, widget.items.length - 1.0);
    final lift = _reduceMotion
        ? 0.0
        : Curves.easeOutCubic.transform(_lift.value) *
              (widget.glassSettings?.dynamicsIntensity ?? 1);
    final stretch = _motion.abs() * lift;
    final fraction = (position + 0.5) / widget.items.length;
    final center = _rtl ? 1 - fraction : fraction;
    final scaleX = 1 + 0.22 * lift + 0.10 * stretch;
    final scaleY = 1 + 0.42 * lift - 0.035 * stretch;
    final selectorWidth = _width / widget.items.length * scaleX;
    final selectorHeight = (constraints.maxHeight - 10) * scaleY;

    return Stack(
      clipBehavior: Clip.none,
      children: [
        // Surface first: the live backdrop, not a flattened screenshot or icon texture.
        Positioned.fill(
          child: IgnorePointer(
            child: GlassShadow(
              key: const ValueKey('android-glass-surface'),
              radius: BorderRadius.circular(30),
              shadow: BoxShadow(
                color: widget.tintColor.withValues(alpha: dark ? 0.24 : 0.09),
                blurRadius: 18,
                offset: const Offset(0, 5),
              ),
              child: _surface(dark: dark, highContrast: highContrast),
            ),
          ),
        ),
        // Icons must be painted before the lens so its backdrop input contains
        // them. Painting another crisp icon above it hides the optical smear.
        Positioned.fill(
          child: IgnorePointer(
            child: ExcludeSemantics(
              child: Padding(
                padding: EdgeInsets.symmetric(
                  horizontal: 5,
                  vertical: 5 * (1 - _compactProgress),
                ),
                child: _destinations(position, lift, dark, iconsOnly: true),
              ),
            ),
          ),
        ),
        Positioned(
          left: 5 + center * _width - selectorWidth / 2,
          // Lift the expanded capsule out of the surface, with room in the outer padding.
          top: (constraints.maxHeight - selectorHeight) / 2 - 3 * lift,
          width: selectorWidth,
          height: selectorHeight,
          child: IgnorePointer(
            child: SizedBox.expand(
              key: const ValueKey('android-glass-selector'),
              child: _surface(
                dark: dark,
                highContrast: highContrast,
                selector: true,
                lift: lift,
              ),
            ),
          ),
        ),
        // Keep labels above the moving lens for crisp, undistorted text.
        Positioned.fill(
          child: Padding(
            padding: EdgeInsets.symmetric(
              horizontal: 5,
              vertical: 5 * (1 - _compactProgress),
            ),
            child: _destinations(position, lift, dark),
          ),
        ),
      ],
    );
  }

  Widget _surface({
    required bool dark,
    required bool highContrast,
    bool selector = false,
    double lift = 0,
  }) {
    final radius = selector ? 29.0 : 30.0;
    if (widget.glassSettings != null) {
      final config = widget.glassSettings!;
      return LiquidGlassSurface(
        settings: selector
            ? config.copyWith(
                variant: LiquidGlassVariant.clear,
                tintColor:
                    highContrast ||
                        config.reduceTransparency ||
                        config.rendering == LiquidGlassRendering.solid
                    ? config.tintColor ?? widget.surfaceColor
                    : widget.tintColor,
                opacity: highContrast ? 0.20 : 0.07 * (1 - lift),
                blur: 0,
                depth:
                    (config.depth +
                            0.02 +
                            0.09 * lift +
                            0.025 * _motion.abs() * lift)
                        .clamp(0.0, 1.0),
                magnification: config.magnification + 0.015 + 0.12 * lift,
                chromaticAberration:
                    config.chromaticAberration + 0.0003 + 0.0012 * lift,
                refraction: config.refraction + 0.15,
                lightDirection: config.lightDirection + _motion * lift * 25,
                lightIntensity: config.lightIntensity + 0.25 * lift,
                refractionWidth: config.refractionWidth + 4 * lift,
                // The interactive lens retains its full bevel as it compacts.
                scaleRefractionWithSize: false,
                edgeAmbientIntensity:
                    (config.edgeAmbientIntensity + 0.15 * lift).clamp(0.0, 5.0),
                edgeLightSpread: (config.edgeLightSpread + 0.1 * lift).clamp(
                  0.0,
                  1.0,
                ),
                shadowOpacity: 0,
              )
            : config,
        borderRadius: BorderRadius.circular(radius),
        child: const SizedBox.expand(),
      );
    }
    final tint = selector
        ? widget.tintColor.withValues(
            alpha: highContrast ? 0.20 : 0.07 * (1 - lift),
          )
        : widget.surfaceColor.withValues(
            alpha: highContrast ? 1 : (dark ? 0.45 : 0.30),
          );
    if (!ui.ImageFilter.isShaderFilterSupported ||
        highContrast ||
        _reduceMotion) {
      return ClipRRect(
        borderRadius: BorderRadius.circular(radius),
        child: BackdropFilter(
          filter: ui.ImageFilter.blur(
            sigmaX: selector ? 0 : 10,
            sigmaY: selector ? 0 : 10,
          ),
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: tint,
              borderRadius: BorderRadius.circular(radius),
              border: Border.all(
                color: widget.tintColor.withValues(alpha: selector ? .12 : .22),
                width: 1.2,
              ),
            ),
          ),
        ),
      );
    }
    return DecoratedBox(
      position: DecorationPosition.foreground,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(radius),
        border: Border.all(
          color: widget.tintColor.withValues(alpha: selector ? .12 : .22),
          width: 1.2,
        ),
      ),
      child: LiquidGlassSurface(
        borderRadius: BorderRadius.circular(radius),
        settings: LiquidGlassSettings(
          tintColor: tint.withValues(alpha: 1),
          opacity: tint.a,
          saturation: selector ? 1.12 : 1.2,
          blur: selector ? 0 : 8,
          borderWidth: selector ? 1.1 : 0.8,
          lightIntensity: selector ? 0.85 + lift * 0.35 : 0.65,
          lightDirection: 115 + (selector ? _motion * lift * 25 : 0),
          edgeAmbientIntensity: selector ? 0.65 : 0.35,
          edgeLightSpread: selector ? 0.65 : 0.45,
          magnification: selector ? 1.015 + 0.12 * lift : 1,
          chromaticAberration: selector ? 0.0008 + 0.0012 * lift : 0.0005,
          refraction: selector ? 1.5 : 1.35,
          refractionWidth: selector ? 8 + 4 * lift : 8,
          scaleRefractionWithSize: false,
          depth: selector
              ? 0.065 + 0.09 * lift + 0.025 * _motion.abs() * lift
              : 0.045,
          shadowOpacity: 0,
          animationDuration: Duration.zero,
        ),
        child: const SizedBox.expand(),
      ),
    );
  }

  Widget _destinations(
    double position,
    double lift,
    bool dark, {
    bool iconsOnly = false,
  }) {
    final preview = _dragging ? position.round() : widget.selectedIndex;
    final theme = Theme.of(context);
    return Row(
      children: List.generate(widget.items.length, (index) {
        final item = widget.items[index];
        final active = index == preview;
        final color = active ? widget.tintColor : widget.unselectedColor;
        final icon = iconsOnly
            ? Icon(item.icon, size: 23, color: color)
            : const SizedBox(width: 23, height: 23);
        final proximity = (1 - (position - index).abs()).clamp(0.0, 1.0);
        return Expanded(
          child: Semantics(
            label: item.badge == null
                ? item.label
                : '${item.label}, ${item.badge}',
            excludeSemantics: true,
            selected: index == widget.selectedIndex,
            button: true,
            onTap: () => _commit(index),
            child: Transform.scale(
              scale: ui.ImageFilter.isShaderFilterSupported
                  ? 1
                  : 1 + 0.09 * lift * proximity,
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  item.badge == null || !iconsOnly
                      ? icon
                      : Badge(label: Text(item.badge!), child: icon),
                  SizedBox(height: 3 * (1 - _compactProgress)),
                  if (iconsOnly)
                    SizedBox(
                      height:
                          MediaQuery.textScalerOf(
                            context,
                          ).clamp(maxScaleFactor: 1.4).scale(12) *
                          1.15 *
                          (1 - _compactProgress),
                    )
                  else
                    ClipRect(
                      child: Align(
                        heightFactor: 1 - _compactProgress,
                        child: Opacity(
                          opacity: 1 - _compactProgress,
                          child: Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 4),
                            child: Text(
                              item.label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              textAlign: TextAlign.center,
                              textScaler: MediaQuery.textScalerOf(
                                context,
                              ).clamp(maxScaleFactor: 1.4),
                              style: theme.textTheme.labelSmall?.copyWith(
                                color: color,
                                fontSize: 12,
                                height: 1.15,
                                fontWeight: active
                                    ? FontWeight.w700
                                    : FontWeight.w500,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        );
      }),
    );
  }
}
