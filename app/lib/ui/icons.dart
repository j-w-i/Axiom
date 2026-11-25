import 'package:flutter/widgets.dart';

import 'icon_data.dart';
import 'svg_path.dart';
import 'tokens.dart';

/// Icons, drawn for the size they are shown at: 16, and 12 for chevrons and checks. Each has an outline
/// and a full shape, for what is chosen. They are Fluent's (`THIRD_PARTY.md`).
enum AxIcon {
  play,
  pause,
  stop,
  up,
  search,
  file,
  edit,
  terminal,
  branch,
  plus,
  check,
  down,
  right,
  shield,
  attach,
  web,
  delegate,
  close,
  list,
  think,
  warn,
  folder,
  account,
  sidebar,
  panel,
  comment,
  chat,
  refresh,
  more;

  (double, String, String) get _data => iconData[name]!;

  /// The size it is drawn for, and shown at.
  double get size => _data.$1;

  static final _cache = <(AxIcon, bool), Path>{};
  Path path({required bool filled}) =>
      _cache[(this, filled)] ??= svgPath(filled ? _data.$3 : _data.$2);
}

class AxGlyph extends StatelessWidget {
  const AxGlyph(this.icon, {super.key, this.size, this.color, this.filled = false});
  final AxIcon icon;

  /// If it is not the size it is drawn for.
  final double? size;
  final Color? color;
  final bool filled;
  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: CustomPaint(
      // It belongs to the words beside it, and is as large as they are.
      size: Size.square(MediaQuery.textScalerOf(context).scale(size ?? icon.size)),
      painter: _GlyphPainter(icon, color ?? Ax.of(context).colors.ink3, filled),
    ),
  );
}

class _GlyphPainter extends CustomPainter {
  const _GlyphPainter(this.icon, this.color, this.filled);
  final AxIcon icon;
  final Color color;
  final bool filled;
  @override
  void paint(Canvas c, Size s) {
    // Square and centred, whatever box it is given.
    final k = s.shortestSide;
    c.translate((s.width - k) / 2, (s.height - k) / 2);
    c.scale(k / icon.size);
    c.drawPath(icon.path(filled: filled), Paint()..color = color);
  }

  @override
  bool shouldRepaint(_GlyphPainter o) => o.icon != icon || o.color != color || o.filled != filled;
}

/// Axiom's mark: two facets of one solid, the right one in shade.
class AxMark extends StatelessWidget {
  const AxMark({super.key, this.size = 18});
  final double size;
  @override
  Widget build(BuildContext context) => Semantics(
    label: 'Axiom',
    image: true,
    child: CustomPaint(size: Size.square(size), painter: _MarkPainter(Ax.of(context).colors)),
  );
}

class _MarkPainter extends CustomPainter {
  const _MarkPainter(this.c);
  final AxColors c;
  @override
  void paint(Canvas v, Size s) {
    v.scale(s.width / 20);
    Path tri(List<double> q) => Path()
      ..moveTo(q[0], q[1])
      ..lineTo(q[2], q[3])
      ..lineTo(q[4], q[5])
      ..lineTo(q[6], q[7])
      ..close();
    v.drawPath(tri([10, 2, 2, 18, 6.6, 18, 10, 11]), Paint()..color = c.ink2);
    v.drawPath(tri([10, 2, 18, 18, 13.4, 18, 10, 11]), Paint()..color = c.live);
  }

  @override
  bool shouldRepaint(_MarkPainter o) => o.c != c;
}
