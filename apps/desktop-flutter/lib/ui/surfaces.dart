// Surfaces by depth, as the Tauri shell drew them (FL-DS-002; its tokens.css materials): recessed
// wells, raised keys with a gradient, a lit edge and a contact shade, overlay sheets. Edges are painted directly, not stacked box-shadows:
// on the owner's 125 % display the Tauri shell lost 6–10 fps to box-shadows alone.

import 'package:flutter/widgets.dart';

import 'theme.dart';
import 'tokens.dart';

/// −1: a recessed well (fields, the command field, the console's prompt): the well tone, a 1 px
/// border, an inner shadow along the top and a lit edge under it (Tauri's `--recessed-*`).
class Well extends StatelessWidget {
  const Well({
    super.key,
    required this.child,
    this.radius = Metrics.radiusControl,
    this.padding,
    this.border,
  });
  final Widget child;
  final double radius;
  final EdgeInsetsGeometry? padding;

  /// The border (defaults to the groove); fields pass their input line or the focus colour.
  final Color? border;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return CustomPaint(
      painter: _WellPainter(
        fill: c.recessed,
        border: border ?? c.groove,
        inner: c.wellShadow,
        edge: c.litEdge,
        radius: radius,
      ),
      child: padding == null ? child : Padding(padding: padding!, child: child),
    );
  }
}

class _WellPainter extends CustomPainter {
  _WellPainter({
    required this.fill,
    required this.border,
    required this.inner,
    required this.edge,
    required this.radius,
  });
  final Color fill, border, inner, edge;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final rr = RRect.fromRectAndRadius(rect, Radius.circular(radius));
    // The lit edge below the well (0 1px 0 var(--edge)).
    canvas.drawRRect(rr.shift(const Offset(0, 1)), Paint()..color = edge);
    canvas.drawRRect(rr, Paint()..color = fill);
    // The inner shadow along the top (inset 0 1px 2px).
    canvas.save();
    canvas.clipRRect(rr);
    canvas.drawRect(
      Rect.fromLTWH(0, 0, size.width, 3),
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [inner, inner.withValues(alpha: 0)],
        ).createShader(Rect.fromLTWH(0, 0, size.width, 3)),
    );
    canvas.restore();
    canvas.drawRRect(
      rr.deflate(0.5),
      Paint()
        ..style = PaintingStyle.stroke
        ..color = border,
    );
  }

  @override
  bool shouldRepaint(_WellPainter o) =>
      o.fill != fill ||
      o.border != border ||
      o.inner != inner ||
      o.edge != edge ||
      o.radius != radius;
}

/// +1: a raised key (buttons, keycaps): the raised gradient, a 1 px border, a lit edge inside
/// the top and a 1 px contact shade below (Tauri's `--raised-*`).
class Raised extends StatelessWidget {
  const Raised({
    super.key,
    required this.child,
    this.color,
    this.top,
    this.border,
    this.radius = Metrics.radiusControl,
    this.topOnly = false,
    this.lit = true,
  });
  final Widget child;

  /// The fill (the gradient's lower stop); [top] its upper stop.
  final Color? color;
  final Color? top;
  final Color? border;
  final double radius;

  /// Only the top corners rounded, no border below (tabs).
  final bool topOnly;

  /// The lit edge inside the top (off for the pressed state).
  final bool lit;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return CustomPaint(
      painter: RaisedPainter(
        top: top ?? (color == null ? c.raisedTop : color!),
        bottom: color ?? c.raised,
        border: border ?? c.groove,
        lit: lit ? c.litEdge : null,
        shade: c.contactShade,
        radius: radius,
        topOnly: topOnly,
      ),
      child: child,
    );
  }
}

/// Paints a raised key.
class RaisedPainter extends CustomPainter {
  RaisedPainter({
    required this.top,
    required this.bottom,
    required this.border,
    required this.lit,
    required this.shade,
    required this.radius,
    this.topOnly = false,
  });
  final Color top, bottom, border, shade;
  final Color? lit;
  final double radius;
  final bool topOnly;

  @override
  void paint(Canvas canvas, Size size) {
    final r = Radius.circular(radius);
    final rect = Offset.zero & size;
    final rr = topOnly
        ? RRect.fromRectAndCorners(rect, topLeft: r, topRight: r)
        : RRect.fromRectAndRadius(rect, r);
    if (!topOnly) canvas.drawRRect(rr.shift(const Offset(0, 1)), Paint()..color = shade);
    canvas.drawRRect(
      rr,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [top, bottom],
        ).createShader(rect),
    );
    if (lit != null) {
      canvas.save();
      canvas.clipRRect(rr.deflate(1));
      canvas.drawRect(Rect.fromLTWH(0, 1, size.width, 1), Paint()..color = lit!);
      canvas.restore();
    }
    canvas.drawRRect(
      rr.deflate(0.5),
      Paint()
        ..style = PaintingStyle.stroke
        ..color = border,
    );
  }

  @override
  bool shouldRepaint(RaisedPainter o) =>
      o.top != top ||
      o.bottom != bottom ||
      o.border != border ||
      o.lit != lit ||
      o.shade != shade ||
      o.radius != radius ||
      o.topOnly != topOnly;
}

/// +2: a sheet (menus, dialogs, the palette, tooltips): the overlay tone, a 1 px border, a lit
/// edge inside the top and Tauri's two-layer overlay shadow.
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
        border: Border.all(color: c.groove),
        boxShadow: [
          BoxShadow(color: c.sheetShadow, blurRadius: 32, offset: const Offset(0, 12)),
          BoxShadow(
            color: c.sheetShadow.withValues(alpha: c.sheetShadow.a * 0.64),
            blurRadius: 6,
            offset: const Offset(0, 2),
          ),
        ],
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
    canvas.drawLine(
      Offset(radius, 0.5),
      Offset(size.width - radius, 0.5),
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

/// Paints one sheet (its blurred shadow included) for the first few frames after start, nearly
/// transparent, so the renderer prepares the blur before the first menu or palette opens.
/// Measured: without it the first palette's frame took 33.7 ms to raster; later ones 2–5 ms.
class SheetWarmup extends StatefulWidget {
  const SheetWarmup({super.key});

  @override
  State<SheetWarmup> createState() => _SheetWarmupState();
}

class _SheetWarmupState extends State<SheetWarmup> {
  int _frames = 0;

  @override
  void initState() {
    super.initState();
    _tick();
  }

  void _tick() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (++_frames >= 3) {
        setState(() {});
      } else {
        setState(() {});
        _tick();
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    if (_frames >= 3) return const SizedBox.shrink();
    return const IgnorePointer(
      child: ExcludeSemantics(
        child: Opacity(
          opacity: 0.01,
          child: SizedBox(width: 48, height: 32, child: Sheet(child: SizedBox.expand())),
        ),
      ),
    );
  }
}
