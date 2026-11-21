// Surfaces by depth (ADR-0020 §4): recessed wells, raised items with a lit edge and a contact
// shade, sheets with the one soft shadow. Edges are painted directly, not stacked box-shadows:
// on the owner's 125 % display the Tauri shell lost 6–10 fps to box-shadows alone.

import 'package:flutter/widgets.dart';

import 'theme.dart';
import 'tokens.dart';

/// −1: a recessed well (inputs, the console, the composer's bezel).
class Well extends StatelessWidget {
  const Well({super.key, required this.child, this.radius = Metrics.radiusControl, this.padding});
  final Widget child;
  final double radius;
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: c.recessed,
        borderRadius: BorderRadius.circular(radius),
        border: Border.all(color: c.hairline),
      ),
      child: padding == null ? child : Padding(padding: padding!, child: child),
    );
  }
}

/// +1: a raised item: its fill, a 1 px lit edge inside the top, a 1 px contact shade below.
class Raised extends StatelessWidget {
  const Raised({
    super.key,
    required this.child,
    this.color,
    this.radius = Metrics.radiusRaised,
    this.topOnly = false,
  });
  final Widget child;
  final Color? color;
  final double radius;

  /// Only the top corners rounded (tabs).
  final bool topOnly;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return CustomPaint(
      painter: _RaisedPainter(
        fill: color ?? c.raised,
        lit: c.litEdge,
        shade: c.contactShade,
        radius: radius,
        topOnly: topOnly,
      ),
      child: child,
    );
  }
}

class _RaisedPainter extends CustomPainter {
  _RaisedPainter({
    required this.fill,
    required this.lit,
    required this.shade,
    required this.radius,
    required this.topOnly,
  });
  final Color fill, lit, shade;
  final double radius;
  final bool topOnly;

  @override
  void paint(Canvas canvas, Size size) {
    final r = Radius.circular(radius);
    final rect = Offset.zero & size;
    final rrect = topOnly
        ? RRect.fromRectAndCorners(rect, topLeft: r, topRight: r)
        : RRect.fromRectAndRadius(rect, r);
    if (!topOnly) {
      canvas.drawRRect(rrect.shift(const Offset(0, 1)), Paint()..color = shade);
    }
    canvas.drawRRect(rrect, Paint()..color = fill);
    // The lit edge: a 1 px line inside the top, inset by the corner radius.
    final inset = radius * 0.6;
    canvas.drawLine(
      Offset(inset, 0.5),
      Offset(size.width - inset, 0.5),
      Paint()
        ..color = lit
        ..strokeWidth = 1,
    );
  }

  @override
  bool shouldRepaint(_RaisedPainter o) =>
      o.fill != fill ||
      o.lit != lit ||
      o.shade != shade ||
      o.radius != radius ||
      o.topOnly != topOnly;
}

/// +2: a sheet (menus, dialogs, the palette, tooltips): the sheet tone, a lit edge, a hairline
/// and the one soft shadow.
class Sheet extends StatelessWidget {
  const Sheet({super.key, required this.child, this.radius = Metrics.radiusSheet, this.padding});
  final Widget child;
  final double radius;
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: c.sheet,
        borderRadius: BorderRadius.circular(radius),
        border: Border.all(color: c.hairline),
        boxShadow: [BoxShadow(color: c.sheetShadow, blurRadius: 24, offset: const Offset(0, 8))],
      ),
      child: CustomPaint(
        foregroundPainter: _LitEdge(c.litEdge, radius),
        child: padding == null ? child : Padding(padding: padding!, child: child),
      ),
    );
  }
}

class _LitEdge extends CustomPainter {
  _LitEdge(this.color, this.radius);
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final inset = radius * 0.7;
    canvas.drawLine(
      Offset(inset, 1.5),
      Offset(size.width - inset, 1.5),
      Paint()
        ..color = color
        ..strokeWidth = 1,
    );
  }

  @override
  bool shouldRepaint(_LitEdge o) => o.color != color || o.radius != radius;
}

/// A 1 px dark groove between rows or regions.
class Groove extends StatelessWidget {
  const Groove({super.key, this.vertical = false});
  final bool vertical;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return SizedBox(
      width: vertical ? 1 : double.infinity,
      height: vertical ? double.infinity : 1,
      child: ColoredBox(color: c.groove),
    );
  }
}

/// A 1 px hairline between surfaces of the same tone.
class Hairline extends StatelessWidget {
  const Hairline({super.key, this.vertical = false});
  final bool vertical;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: vertical ? 1 : double.infinity,
      height: vertical ? double.infinity : 1,
      child: ColoredBox(color: context.colors.hairline),
    );
  }
}
