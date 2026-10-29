import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/physics.dart';
import '../surfaces/liquid_glass_surface.dart';
import '../surfaces/glass_shadow_decoration.dart';

part 'liquid_glass_navigation_bar.dart';

/// Compatibility alias for apps migrating from the navigation-only package.
@Deprecated('Use LiquidGlassNavigationBar instead.')
typedef LiquidRetinaNavigationBar = LiquidGlassNavigationBar;

/// Compatibility alias for apps migrating from the navigation-only package.
@Deprecated('Use LiquidGlassNavigationItem instead.')
typedef LiquidRetinaNavigationItem = LiquidGlassNavigationItem;

/// A destination in a [LiquidGlassNavigationBar].
@immutable
class LiquidGlassNavigationItem {
  /// Creates an icon and label, with an optional badge.
  const LiquidGlassNavigationItem({
    required this.label,
    required this.icon,
    this.badge,
  });

  /// Accessible destination label displayed below the icon.
  final String label;

  /// Icon from the host app's declared icon fonts.
  final IconData icon;

  /// Optional badge text.
  final String? badge;
}

/// A controlled liquid glass navigation capsule designed for Android.
///
/// The host owns routing and [selectedIndex]. Use `Scaffold(extendBody: true)`
/// to let the lens refract the content behind it. Supports two to five tabs.
class LiquidGlassNavigationBar extends StatelessWidget {
  /// Creates a navigation capsule using the surrounding Material theme.
  LiquidGlassNavigationBar({
    super.key,
    required List<LiquidGlassNavigationItem> items,
    required this.selectedIndex,
    required this.onTap,
    this.onScrubChanged,
    this.tintColor,
    this.unselectedColor,
    this.surfaceColor,
    this.glassSettings,
    this.scrollController,
    this.shrinkOnScroll = true,
    this.collapseThreshold = 32,
    this.expandThreshold = 8,
  }) : items = List.unmodifiable(items),
       assert(collapseThreshold > 0 && collapseThreshold < double.infinity),
       assert(expandThreshold > 0 && expandThreshold < double.infinity) {
    if (items.length < 2 || items.length > 5) {
      throw ArgumentError.value(items.length, 'items.length', 'Use 2–5 tabs.');
    }
    if (selectedIndex < 0 || selectedIndex >= items.length) {
      throw RangeError.index(selectedIndex, items, 'selectedIndex');
    }
  }

  /// Layout height excluding the device's bottom safe area.
  static const double height = 96;

  /// Compact layout height excluding the bottom safe area. Destinations retain
  /// at least 48 logical pixels of height and keep accessible labels.
  static const double compactHeight = 74;

  /// Destinations in logical reading order.
  final List<LiquidGlassNavigationItem> items;

  /// Destination selected by the host.
  final int selectedIndex;

  /// Requests selection; the host may accept or reject the destination.
  final ValueChanged<int> onTap;

  /// Fractional logical tab position after a hold and during dragging.
  /// `null` ends or cancels preview. This callback never changes routes.
  final ValueChanged<double?>? onScrubChanged;

  /// Active color, defaulting to [ColorScheme.primary].
  final Color? tintColor;

  /// Inactive color, defaulting to [ColorScheme.onSurfaceVariant].
  final Color? unselectedColor;

  /// Surface tint, defaulting to [ColorScheme.surface].
  final Color? surfaceColor;

  /// Optional optical configuration, also inherited from [LiquidGlassTheme].
  /// Omitting both preserves the original navigation appearance.
  final LiquidGlassSettings? glassSettings;

  /// The single vertical viewport that drives compact/expanded navigation.
  /// Omit to retain the fixed expanded bar. The host owns this controller.
  final ScrollController? scrollController;
  final bool shrinkOnScroll;
  final double collapseThreshold;
  final double expandThreshold;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return _AndroidGlassBar(
      items: items,
      selectedIndex: selectedIndex,
      onTap: onTap,
      onScrubChanged: onScrubChanged,
      tintColor: tintColor ?? colors.primary,
      unselectedColor: unselectedColor ?? colors.onSurfaceVariant,
      surfaceColor: surfaceColor ?? colors.surface,
      glassSettings: glassSettings ?? LiquidGlassTheme.maybeOf(context),
      scrollController: scrollController,
      shrinkOnScroll: shrinkOnScroll,
      collapseThreshold: collapseThreshold,
      expandThreshold: expandThreshold,
    );
  }
}
