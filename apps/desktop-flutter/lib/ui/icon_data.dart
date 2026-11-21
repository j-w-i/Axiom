// The 16 px stroke icons (lib/ui/icons.dart) and the widget that draws them.

import 'package:flutter/widgets.dart';
import 'package:path_parsing/path_parsing.dart';

import 'theme.dart';

/// One shape of an icon, in a 16 × 16 view box.
sealed class IconElement {
  const IconElement({this.fill = false});

  /// Filled with the icon colour (otherwise stroked).
  final bool fill;
}

final class IconPath extends IconElement {
  const IconPath(this.d, {super.fill});
  final String d;
}

final class IconCircle extends IconElement {
  const IconCircle(this.cx, this.cy, this.r, {super.fill});
  final double cx, cy, r;
}

final class IconRect extends IconElement {
  const IconRect(this.x, this.y, this.w, this.h, {this.rx = 0, super.fill});
  final double x, y, w, h, rx;
}

/// An icon: its shapes in a 16 × 16 view box.
final class IconData16 {
  const IconData16(this.elements);
  final List<IconElement> elements;
}

class _PathBuilder extends PathProxy {
  _PathBuilder(this.path);
  final Path path;
  @override
  void moveTo(double x, double y) => path.moveTo(x, y);
  @override
  void lineTo(double x, double y) => path.lineTo(x, y);
  @override
  void cubicTo(double x1, double y1, double x2, double y2, double x3, double y3) =>
      path.cubicTo(x1, y1, x2, y2, x3, y3);
  @override
  void close() => path.close();
}

final _cache = Expando<List<(Path, bool)>>();

List<(Path, bool)> _paths(IconData16 icon) {
  final hit = _cache[icon];
  if (hit != null) return hit;
  final out = <(Path, bool)>[];
  for (final e in icon.elements) {
    final p = Path();
    switch (e) {
      case IconPath(:final d):
        writeSvgPathDataToPath(d, _PathBuilder(p));
      case IconCircle(:final cx, :final cy, :final r):
        p.addOval(Rect.fromCircle(center: Offset(cx, cy), radius: r));
      case IconRect(:final x, :final y, :final w, :final h, :final rx):
        p.addRRect(RRect.fromRectAndRadius(Rect.fromLTWH(x, y, w, h), Radius.circular(rx)));
    }
    out.add((p, e.fill));
  }
  return _cache[icon] = out;
}

/// Draws [icon] at [size] in [color] (default: the ambient text colour). Decorative: it has no
/// semantics of its own; the control around it carries the name.
class AxiomIcon extends StatelessWidget {
  const AxiomIcon(this.icon, {super.key, this.size = 16, this.color});
  final IconData16 icon;
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final c = color ?? DefaultTextStyle.of(context).style.color ?? context.colors.ink2;
    return ExcludeSemantics(
      child: SizedBox.square(
        dimension: size,
        child: CustomPaint(painter: _IconPainter(icon, c)),
      ),
    );
  }
}

class _IconPainter extends CustomPainter {
  _IconPainter(this.icon, this.color);
  final IconData16 icon;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final scale = size.width / 16;
    canvas.save();
    canvas.scale(scale);
    final stroke = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.4
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..isAntiAlias = true;
    final fill = Paint()
      ..color = color
      ..style = PaintingStyle.fill
      ..isAntiAlias = true;
    for (final (path, filled) in _paths(icon)) {
      canvas.drawPath(path, filled ? fill : stroke);
      if (filled) canvas.drawPath(path, stroke);
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_IconPainter old) => old.icon != icon || old.color != color;
}
