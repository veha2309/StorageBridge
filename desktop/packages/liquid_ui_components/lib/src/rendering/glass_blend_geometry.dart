import 'package:flutter/material.dart';

/// Internal description of the same smooth union used by the blend clip.
@immutable
class GlassBlendGeometry {
  const GlassBlendGeometry({
    required this.first,
    required this.second,
    required this.radius,
    required this.smoothness,
  });

  final Offset first;
  final Offset second;
  final double radius;
  final double smoothness;
}
