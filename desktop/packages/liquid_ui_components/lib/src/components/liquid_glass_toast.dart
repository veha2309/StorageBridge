import 'package:flutter/material.dart';

import '../motion/liquid_glass_dynamics.dart';
import '../surfaces/liquid_glass_surface.dart';

/// A floating glass notification managed by Flutter's [ScaffoldMessenger].
///
/// Use [show] to capture the local glass settings, replace pending messages and
/// apply the component's motion timing. The constructor also works directly
/// with `ScaffoldMessenger.of(context).showSnackBar(...)` for native queuing.
/// Native live-region semantics, swipe dismissal, keyboard avoidance and bottom
/// navigation clearance remain owned by ScaffoldMessenger.
class LiquidGlassToast extends SnackBar {
  LiquidGlassToast({
    super.key,
    String? message,
    Widget? content,
    SnackBarAction? action,
    IconData? icon = Icons.check_circle_outline_rounded,
    LiquidGlassSettings? settings,
    bool showCloseButton = true,
    super.duration = const Duration(seconds: 3),
    EdgeInsetsGeometry? margin,
    super.width,
    SnackBarBehavior behavior = SnackBarBehavior.floating,
    bool? persist,
    double actionOverflowThreshold = 0.25,
    Color? closeIconColor,
    super.hitTestBehavior,
    super.animation,
    super.onVisible,
    DismissDirection dismissDirection = DismissDirection.down,
  }) : assert((message != null) != (content != null)),
       assert(message == null || message.isNotEmpty),
       assert(width == null || margin == null),
       assert(actionOverflowThreshold >= 0 && actionOverflowThreshold <= 1),
       assert(!duration.isNegative),
       super(
         content: _LiquidToastBody(
           message: message,
           content: content,
           action: action,
           actionOverflowThreshold: actionOverflowThreshold,
           closeIconColor: closeIconColor,
           icon: icon,
           settings: settings,
           showCloseButton: showCloseButton,
         ),
         backgroundColor: Colors.transparent,
         elevation: 0,
         padding: EdgeInsets.zero,
         margin: behavior == SnackBarBehavior.floating && width == null
             ? margin ?? const EdgeInsets.fromLTRB(16, 8, 16, 16)
             : null,
         shape: const RoundedRectangleBorder(),
         behavior: behavior,
         clipBehavior: Clip.none,
         showCloseIcon: false,
         // Keep the native action inside the shared glass surface. Mirror
         // Flutter's action persistence default without adding another action.
         persist: persist ?? action != null,
         dismissDirection: dismissDirection,
       );

  /// Show a toast without creating a separate overlay or another glass layer.
  /// By default, the newest message replaces the pending toast queue.
  static ScaffoldFeatureController<SnackBar, SnackBarClosedReason> show(
    BuildContext context, {
    String? message,
    Widget? content,
    SnackBarAction? action,
    IconData? icon = Icons.check_circle_outline_rounded,
    LiquidGlassSettings? settings,
    bool showCloseButton = true,
    Duration duration = const Duration(seconds: 3),
    bool replaceCurrent = true,
    bool? persist,
    EdgeInsetsGeometry? margin,
    double? width,
    SnackBarBehavior behavior = SnackBarBehavior.floating,
    double actionOverflowThreshold = 0.25,
    Color? closeIconColor,
    HitTestBehavior? hitTestBehavior,
    DismissDirection dismissDirection = DismissDirection.down,
    VoidCallback? onVisible,
    AnimationStyle? animationStyle,
  }) {
    final messenger = ScaffoldMessenger.of(context);
    final reduced = MediaQuery.disableAnimationsOf(context);
    if (replaceCurrent) messenger.clearSnackBars();
    return messenger.showSnackBar(
      LiquidGlassToast(
        message: message,
        content: content,
        action: action,
        persist: persist,
        margin: margin,
        width: width,
        behavior: behavior,
        actionOverflowThreshold: actionOverflowThreshold,
        closeIconColor: closeIconColor,
        hitTestBehavior: hitTestBehavior,
        dismissDirection: dismissDirection,
        onVisible: onVisible,
        icon: icon,
        settings: settings ?? LiquidGlassTheme.of(context),
        showCloseButton: showCloseButton,
        duration: duration,
      ),
      snackBarAnimationStyle: reduced
          ? AnimationStyle.noAnimation
          : animationStyle ??
                const AnimationStyle(
                  duration: Duration(milliseconds: 220),
                  reverseDuration: Duration(milliseconds: 160),
                ),
    );
  }
}

class _LiquidToastBody extends StatelessWidget {
  const _LiquidToastBody({
    required this.message,
    required this.content,
    required this.action,
    required this.actionOverflowThreshold,
    required this.closeIconColor,
    required this.icon,
    required this.settings,
    required this.showCloseButton,
  });
  final String? message;
  final Widget? content;
  final SnackBarAction? action;
  final double actionOverflowThreshold;
  final Color? closeIconColor;
  final IconData? icon;
  final LiquidGlassSettings? settings;
  final bool showCloseButton;

  @override
  Widget build(BuildContext context) {
    final config = settings ?? LiquidGlassTheme.of(context);
    final surface = LiquidGlassSurface(
      settings: config,
      borderRadius: BorderRadius.circular(28),
      padding: EdgeInsetsDirectional.fromSTEB(
        16,
        8,
        showCloseButton ? 8 : 16,
        8,
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final actionText = TextPainter(
            text: TextSpan(
              text: action?.label ?? '',
              style: Theme.of(context).textTheme.labelLarge,
            ),
            textDirection: Directionality.of(context),
            textScaler: MediaQuery.textScalerOf(context),
          )..layout();
          final overflow =
              action != null &&
              (actionText.width + 32 >
                      constraints.maxWidth * actionOverflowThreshold ||
                  MediaQuery.textScalerOf(context).scale(14) > 20);
          actionText.dispose();
          return Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  if (icon != null) ...[
                    ExcludeSemantics(child: Icon(icon, size: 22)),
                    const SizedBox(width: 12),
                  ],
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: content ?? Text(message!),
                    ),
                  ),
                  if (action != null && !overflow) action!,
                  if (showCloseButton) ...[
                    const SizedBox(width: 8),
                    LiquidGlassDynamics(
                      intensity: config.dynamicsIntensity,
                      child: IconButton(
                        tooltip: MaterialLocalizations.of(
                          context,
                        ).closeButtonTooltip,
                        icon: const Icon(Icons.close_rounded, size: 20),
                        color: closeIconColor,
                        onPressed: () =>
                            ScaffoldMessenger.of(context).hideCurrentSnackBar(
                              reason: SnackBarClosedReason.dismiss,
                            ),
                      ),
                    ),
                  ],
                ],
              ),
              if (action != null && overflow)
                Align(
                  alignment: AlignmentDirectional.centerEnd,
                  child: action!,
                ),
            ],
          );
        },
      ),
    );
    final animation = context
        .findAncestorWidgetOfExactType<SnackBar>()
        ?.animation;
    if (animation == null || MediaQuery.disableAnimationsOf(context)) {
      return surface;
    }
    // Animate the already-built surface, retaining its backdrop and content.
    return AnimatedBuilder(
      animation: animation,
      child: surface,
      builder: (context, child) {
        final t = Curves.easeOutCubic.transform(animation.value);
        return Transform.translate(
          offset: Offset(0, (1 - t) * 10),
          child: Transform(
            alignment: Alignment.bottomCenter,
            transform: Matrix4.diagonal3Values(
              0.96 + 0.04 * t,
              0.88 + 0.12 * t,
              1,
            ),
            child: child,
          ),
        );
      },
    );
  }
}
