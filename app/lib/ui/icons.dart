import 'package:flutter/widgets.dart';

import 'tokens.dart';

/// Axiom's own icons: 16-unit grid, 1.35 stroke, round joins, drawn from path data.
enum AxIcon {
  play('M5 3.5 L12.5 8 L5 12.5 Z'),
  pause('M5.5 4 V12 M10.5 4 V12'),
  stop('M4.5 4.5 H11.5 V11.5 H4.5 Z'),
  send('M3 8 H12 M8.5 4.5 L12 8 L8.5 11.5'),
  search('M7 2.8 A4.2 4.2 0 1 0 7 11.2 A4.2 4.2 0 1 0 7 2.8 M10.2 10.2 L13.2 13.2'),
  file('M4 2.5 H9.5 L12 5 V13.5 H4 Z M9.5 2.5 V5 H12'),
  edit('M3 13 L3.6 10.2 L10.6 3.2 L12.8 5.4 L5.8 12.4 Z'),
  terminal('M2.5 3.5 H13.5 V12.5 H2.5 Z M5 6.2 L7 8 L5 9.8 M8 10 H11'),
  branch('M5 3 V13 M5 10 C5 7.5 11 8.5 11 5 M11 5 V3'),
  plus('M8 3.5 V12.5 M3.5 8 H12.5'),
  check('M3.5 8.5 L6.5 11.5 L12.5 4.8'),
  down('M4.5 6.5 L8 10 L11.5 6.5'),
  right('M6.5 4.5 L10 8 L6.5 11.5'),
  shield('M8 2.5 L12.8 4.3 V8 C12.8 11 10.4 12.8 8 13.6 C5.6 12.8 3.2 11 3.2 8 V4.3 Z'),
  attach('M11.5 7 L7.5 11 A2.4 2.4 0 0 1 4.1 7.6 L8.6 3.1 A1.6 1.6 0 0 1 10.9 5.4 L6.6 9.7'),
  web(
    'M8 2.5 A5.5 5.5 0 1 0 8 13.5 A5.5 5.5 0 1 0 8 2.5 M2.5 8 H13.5 M8 2.5 C10.5 5 10.5 11 8 13.5 C5.5 11 5.5 5 8 2.5',
  ),
  delegate('M4.5 3 V6.5 C4.5 8.5 11.5 7.5 11.5 10 V13 M9.5 11 L11.5 13 L13.5 11'),
  close('M4.5 4.5 L11.5 11.5 M11.5 4.5 L4.5 11.5'),
  list('M3 4.5 H13 M3 8 H13 M3 11.5 H9'),
  think('M3 11 C5 11 5 5 8 5 C11 5 11 11 13 11'),
  strips('M3 3.5 V12.5 M6.4 3.5 V12.5 M9.7 3.5 V12.5 M13 3.5 V12.5'),
  lanes('M2.5 4.5 H8 M5.5 8 H11 M9 11.5 H13.5'),
  warn('M8 3 L13.5 12.8 H2.5 Z M8 6.8 V9.4 M8 11 V11.1'),
  more('M4 8 H4.1 M8 8 H8.1 M12 8 H12.1');

  const AxIcon(this.data);
  final String data;
  static final _cache = <AxIcon, Path>{};
  Path get path => _cache[this] ??= _parse(data);
}

Path _parse(String d) {
  final p = Path();
  final tk = RegExp(r'[MLHVCAZ]|-?\d*\.?\d+').allMatches(d).map((m) => m[0]!).toList();
  var i = 0, cmd = '', x = 0.0, y = 0.0;
  double n() => double.parse(tk[i++]);
  while (i < tk.length) {
    if (RegExp('[A-Z]').hasMatch(tk[i])) cmd = tk[i++];
    switch (cmd) {
      case 'M':
        p.moveTo(x = n(), y = n());
        cmd = 'L';
      case 'L':
        p.lineTo(x = n(), y = n());
      case 'H':
        p.lineTo(x = n(), y);
      case 'V':
        p.lineTo(x, y = n());
      case 'C':
        final q = [for (var k = 0; k < 6; k++) n()];
        p.cubicTo(q[0], q[1], q[2], q[3], x = q[4], y = q[5]);
      case 'A':
        final rx = n(), ry = n();
        n();
        final large = n() == 1, sweep = n() == 1;
        p.arcToPoint(
          Offset(x = n(), y = n()),
          radius: Radius.elliptical(rx, ry),
          largeArc: large,
          clockwise: sweep,
        );
      case 'Z':
        p.close();
      default:
        throw FormatException('icon path command $cmd', d);
    }
  }
  return p;
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
        ..strokeWidth = icon == AxIcon.more ? 2 : 1.35
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
