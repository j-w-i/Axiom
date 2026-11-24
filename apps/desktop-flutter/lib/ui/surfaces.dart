// Surfaces (UI-001, ADR-0024): flat fills. Depth comes from the tone steps of the regions; the one
// raised thing on a surface is its primary action, and the one thing that floats is a sheet.
// Edges are painted directly, not stacked box-shadows: on the owner's 125 % display the Tauri
// shell lost 6–10 fps to box-shadows alone.

import 'package:flutter/widgets.dart';

import 'theme.dart';
import 'tokens.dart';

/// A well (a field, a code block): the recessed tone, with a 1 px [border] only where the box is a
/// control whose edge must be seen (fields pass the input line or the focus colour; ≥ 3:1,
/// CR-M1.6-SYSTEM-001).
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
  final Color? border;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: context.colors.recessed,
        borderRadius: BorderRadius.circular(radius),
        border: border == null ? null : Border.all(color: border!),
      ),
      child: padding == null ? child : Padding(padding: padding!, child: child),
    );
  }
}

/// A pressable's fill (a button, a chip); [lit] adds the 1 px top edge that marks the one primary
/// action of a surface.
class Raised extends StatelessWidget {
  const Raised({
    super.key,
    required this.child,
    this.color,
    this.radius = Metrics.radiusControl,
    this.lit = false,
  });
  final Widget child;
  final Color? color;
  final double radius;
  final bool lit;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final box = DecoratedBox(
      decoration: BoxDecoration(
        color: color ?? c.raised,
        borderRadius: BorderRadius.circular(radius),
      ),
      child: child,
    );
    if (!lit) return box;
    return CustomPaint(foregroundPainter: _LitEdge(c.litEdge, radius), child: box);
  }
}

/// A sheet (menus, dialogs, the palette, tooltips): the overlay tone, a 1 px border and a
/// two-layer shadow.
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
        boxShadow: [
          BoxShadow(color: c.sheetShadow, blurRadius: 32, offset: const Offset(0, 12)),
          BoxShadow(
            color: c.sheetShadow.withValues(alpha: c.sheetShadow.a * 0.64),
            blurRadius: 6,
            offset: const Offset(0, 2),
          ),
        ],
      ),
      child: padding == null ? child : Padding(padding: padding!, child: child),
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

/// The 1 px seam between regions.
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
