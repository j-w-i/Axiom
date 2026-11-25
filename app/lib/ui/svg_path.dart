import 'package:flutter/widgets.dart';

/// SVG path data, whole grammar: absolute and relative commands, repeated arguments, packed arc
/// flags. Throws [FormatException] on anything else.
Path svgPath(String d) {
  final p = Path();
  var i = 0;
  var x = 0.0, y = 0.0, sx = 0.0, sy = 0.0, cx = 0.0, cy = 0.0;
  var cmd = '', last = '';

  void skip() {
    while (i < d.length && ' ,\n\t\r'.contains(d[i])) {
      i++;
    }
  }

  bool digit(int k) => k < d.length && d.codeUnitAt(k) >= 48 && d.codeUnitAt(k) <= 57;
  double n() {
    skip();
    final from = i;
    if (i < d.length && (d[i] == '-' || d[i] == '+')) i++;
    while (digit(i)) {
      i++;
    }
    if (i < d.length && d[i] == '.') {
      i++;
      while (digit(i)) {
        i++;
      }
    }
    if (i < d.length && (d[i] == 'e' || d[i] == 'E')) {
      i++;
      if (i < d.length && (d[i] == '-' || d[i] == '+')) i++;
      while (digit(i)) {
        i++;
      }
    }
    return double.parse(d.substring(from, i));
  }

  bool flag() {
    skip();
    final f = d[i++];
    if (f != '0' && f != '1') throw FormatException('arc flag', d, i - 1);
    return f == '1';
  }

  skip();
  while (i < d.length) {
    if (RegExp('[A-Za-z]').hasMatch(d[i])) {
      cmd = d[i++];
    } else if (cmd == 'M') {
      cmd = 'L';
    } else if (cmd == 'm') {
      cmd = 'l';
    }
    final rel = cmd == cmd.toLowerCase();
    double ax() => n() + (rel ? x : 0);
    double ay() => n() + (rel ? y : 0);
    switch (cmd.toUpperCase()) {
      case 'M':
        final (a, b) = (ax(), ay());
        p.moveTo(x = sx = a, y = sy = b);
      case 'L':
        final (a, b) = (ax(), ay());
        p.lineTo(x = a, y = b);
      case 'H':
        p.lineTo(x = ax(), y);
      case 'V':
        p.lineTo(x, y = ay());
      case 'C':
        final (a, b, c, e, f, g) = (ax(), ay(), ax(), ay(), ax(), ay());
        p.cubicTo(a, b, cx = c, cy = e, x = f, y = g);
      case 'S':
        final smooth = 'CS'.contains(last.toUpperCase()) && last.isNotEmpty;
        final (a, b) = smooth ? (2 * x - cx, 2 * y - cy) : (x, y);
        final (c, e, f, g) = (ax(), ay(), ax(), ay());
        p.cubicTo(a, b, cx = c, cy = e, x = f, y = g);
      case 'Q':
        final (a, b, f, g) = (ax(), ay(), ax(), ay());
        p.quadraticBezierTo(cx = a, cy = b, x = f, y = g);
      case 'T':
        final smooth = 'QT'.contains(last.toUpperCase()) && last.isNotEmpty;
        final (a, b) = smooth ? (2 * x - cx, 2 * y - cy) : (x, y);
        final (f, g) = (ax(), ay());
        p.quadraticBezierTo(cx = a, cy = b, x = f, y = g);
      case 'A':
        final (rx, ry, turn) = (n(), n(), n());
        final large = flag(), sweep = flag();
        final (f, g) = (ax(), ay());
        p.arcToPoint(
          Offset(x = f, y = g),
          radius: Radius.elliptical(rx.abs(), ry.abs()),
          rotation: turn,
          largeArc: large,
          clockwise: sweep,
        );
      case 'Z':
        p.close();
        (x, y) = (sx, sy);
      case _:
        throw FormatException('path command "$cmd"', d, i);
    }
    last = cmd;
    skip();
  }
  return p;
}
