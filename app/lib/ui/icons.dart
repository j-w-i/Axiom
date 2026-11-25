import 'package:flutter/widgets.dart';

import 'svg_path.dart';
import 'tokens.dart';

/// Axiom's own icons: 16-unit grid, 1.4 stroke, round corners and ends, drawn from path data.
enum AxIcon {
  play(
    'M5.2 4.3 C5.2 3.5 6 3 6.7 3.4 L12.4 7.1 C13 7.5 13 8.5 12.4 8.9 L6.7 12.6 C6 13 5.2 12.5 5.2 11.7 Z',
  ),
  pause('M5.6 4 V12 M10.4 4 V12'),
  stop(
    'M5.6 4.2 H10.4 A1.4 1.4 0 0 1 11.8 5.6 V10.4 A1.4 1.4 0 0 1 10.4 11.8 H5.6 A1.4 1.4 0 0 1 4.2 10.4 V5.6 A1.4 1.4 0 0 1 5.6 4.2 Z',
  ),
  send('M3 8 H12.6 M8.8 4.2 L12.6 8 L8.8 11.8'),
  search('M7.1 2.6 A4.5 4.5 0 1 0 7.1 11.6 A4.5 4.5 0 1 0 7.1 2.6 M10.4 10.4 L13.4 13.4'),
  file(
    'M8.8 2.2 H5.2 A1.7 1.7 0 0 0 3.5 3.9 V12.1 A1.7 1.7 0 0 0 5.2 13.8 H10.8 A1.7 1.7 0 0 0 12.5 12.1 V5.9 Z M8.8 2.2 V4.9 A1 1 0 0 0 9.8 5.9 H12.5',
  ),
  edit(
    'M9.9 3.3 A1.7 1.7 0 0 1 12.7 6.1 L6.2 12.5 C6 12.7 5.8 12.8 5.5 12.9 L3 13.4 L3.5 10.9 C3.6 10.6 3.7 10.4 3.9 10.2 Z',
  ),
  terminal(
    'M4.4 2.8 H11.6 A2 2 0 0 1 13.6 4.8 V11.2 A2 2 0 0 1 11.6 13.2 H4.4 A2 2 0 0 1 2.4 11.2 V4.8 A2 2 0 0 1 4.4 2.8 Z M5.2 6 L7.2 8 L5.2 10 M8.6 10 H10.8',
  ),
  branch(
    'M4.8 5.6 A1.7 1.7 0 1 0 4.8 2.2 A1.7 1.7 0 1 0 4.8 5.6 M4.8 13.8 A1.7 1.7 0 1 0 4.8 10.4 A1.7 1.7 0 1 0 4.8 13.8 M11.4 6.6 A1.7 1.7 0 1 0 11.4 3.2 A1.7 1.7 0 1 0 11.4 6.6 M4.8 5.6 V10.4 M11.4 6.6 C11.4 9 4.8 7.6 4.8 10.4',
  ),
  plus('M8 3.4 V12.6 M3.4 8 H12.6'),
  check('M3.6 8.4 L6.6 11.3 L12.4 4.9'),
  down('M4.6 6.4 L8 9.7 L11.4 6.4'),
  right('M6.4 4.6 L9.7 8 L6.4 11.4'),
  shield(
    'M8 2.2 C9.4 3.3 11 3.8 12.8 4 V7.8 C12.8 10.8 10.8 12.8 8 13.8 C5.2 12.8 3.2 10.8 3.2 7.8 V4 C5 3.8 6.6 3.3 8 2.2 Z',
  ),
  attach(
    'M12.9 7.4 L7.6 12.6 A3.2 3.2 0 0 1 3.1 8.1 L8.6 2.7 A2.1 2.1 0 0 1 11.6 5.7 L6.2 11 A1 1 0 0 1 4.7 9.6 L9.6 4.8',
  ),
  web(
    'M8 2.2 A5.8 5.8 0 1 0 8 13.8 A5.8 5.8 0 1 0 8 2.2 M2.2 8 H13.8 M8 2.2 C10.6 5 10.6 11 8 13.8 C5.4 11 5.4 5 8 2.2',
  ),
  delegate('M3.4 3 V6 A3 3 0 0 0 6.4 9 H12.4 M9.6 6.2 L12.4 9 L9.6 11.8'),
  close('M4.4 4.4 L11.6 11.6 M11.6 4.4 L4.4 11.6'),
  list('M2.8 4.6 H13.2 M2.8 8 H13.2 M2.8 11.4 H9'),
  think('M3 11 C5 11 5 5 8 5 C11 5 11 11 13 11'),
  strips('M3 3.5 V12.5 M6.4 3.5 V12.5 M9.7 3.5 V12.5 M13 3.5 V12.5'),
  lanes('M2.5 4.5 H8 M5.5 8 H11 M9 11.5 H13.5'),
  warn(
    'M6.9 3 A1.3 1.3 0 0 1 9.1 3 L13.6 11 A1.3 1.3 0 0 1 12.5 13 H3.5 A1.3 1.3 0 0 1 2.4 11 Z M8 6.2 V8.8 M8 10.9 V11',
  ),
  folder(
    'M2.2 4.8 A1.6 1.6 0 0 1 3.8 3.2 H5.7 C6.2 3.2 6.6 3.4 6.9 3.8 L7.5 4.6 C7.7 4.9 8 5 8.4 5 H12.2 A1.6 1.6 0 0 1 13.8 6.6 V11.2 A1.6 1.6 0 0 1 12.2 12.8 H3.8 A1.6 1.6 0 0 1 2.2 11.2 Z',
  ),
  settings(
    'M2.6 5 H7.6 M11.8 5 H13.4 M2.6 11 H4.2 M8.4 11 H13.4 M9.7 6.9 A1.9 1.9 0 1 0 9.7 3.1 A1.9 1.9 0 1 0 9.7 6.9 M6.3 12.9 A1.9 1.9 0 1 0 6.3 9.1 A1.9 1.9 0 1 0 6.3 12.9',
  ),
  sidebar(
    'M4.4 2.8 H11.6 A2 2 0 0 1 13.6 4.8 V11.2 A2 2 0 0 1 11.6 13.2 H4.4 A2 2 0 0 1 2.4 11.2 V4.8 A2 2 0 0 1 4.4 2.8 Z M6.2 2.8 V13.2',
  ),
  panel(
    'M4.4 2.8 H11.6 A2 2 0 0 1 13.6 4.8 V11.2 A2 2 0 0 1 11.6 13.2 H4.4 A2 2 0 0 1 2.4 11.2 V4.8 A2 2 0 0 1 4.4 2.8 Z M9.8 2.8 V13.2',
  ),
  up('M8 12.6 V3.6 M4.2 7.3 L8 3.5 L11.8 7.3'),
  comment(
    'M4.4 2.8 H11.6 A2 2 0 0 1 13.6 4.8 V9.2 A2 2 0 0 1 11.6 11.2 H8.2 L5.2 13.6 V11.2 H4.4 A2 2 0 0 1 2.4 9.2 V4.8 A2 2 0 0 1 4.4 2.8 Z',
  ),
  refresh('M13.2 8 A5.2 5.2 0 1 1 11.5 4.2 L13.2 5.8 M13.2 2.8 V5.8 H10.2'),
  more('M4 8 H4.1 M8 8 H8.1 M12 8 H12.1');

  const AxIcon(this.data);

  final String data;
  static final _cache = <AxIcon, Path>{};
  Path get path => _cache[this] ??= svgPath(data);
}

class AxGlyph extends StatelessWidget {
  const AxGlyph(this.icon, {super.key, this.size = 14, this.color, this.filled = false});
  final AxIcon icon;
  final double size;
  final Color? color;
  final bool filled;
  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: CustomPaint(
      size: Size.square(size),
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
    c.scale(k / 16);
    c.drawPath(
      icon.path,
      Paint()
        ..color = color
        ..style = filled ? PaintingStyle.fill : PaintingStyle.stroke
        ..strokeWidth = icon == AxIcon.more ? 2 : 1.4
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round,
    );
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
