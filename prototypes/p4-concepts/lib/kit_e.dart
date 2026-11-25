import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flutter/widgets.dart';

// ---------- palette ----------
const abyss = Color(0xFF06090E);
const ink1 = Color(0xFFE6E9EF), ink2 = Color(0xFFB0B7C3), ink3 = Color(0xFF868F9E), ink4 = Color(0xFF626B7A);
const steel = Color(0xFF8490A6), steelDim = Color(0xFF3B4352);
const volt = Color(0xFF3F68E0), voltDeep = Color(0xFF2A49A8);
const amber = Color(0xFFD2A052), red = Color(0xFFD9655E), green = Color(0xFF5FA383);
Color a(Color c, double o) => c.withValues(alpha: o);

// ---------- text ----------
TextStyle ts(double s, [int w = 400, Color c = ink1, double wd = 100, double ls = 0, double h = 1.4]) => TextStyle(fontFamily: 'Archivo', fontSize: s, height: h, color: c, letterSpacing: ls == 0 ? (s < 13 ? .15 : .1) : ls, fontVariations: [FontVariation('wght', w.toDouble()), FontVariation('wdth', wd)], fontFeatures: const [FontFeature.tabularFigures()]);
TextStyle ms(double s, [int w = 400, Color c = ink1, double h = 1.5]) => TextStyle(fontFamily: 'Spline', fontSize: s, height: h, color: c, fontVariations: [FontVariation('wght', w.toDouble())]);
Widget t(String x, [double s = 12, int w = 400, Color c = ink2, double wd = 100]) => Text(x, style: ts(s, w, c, wd), maxLines: 1, overflow: TextOverflow.ellipsis, softWrap: false);
Widget m(String x, [double s = 11.5, Color c = ink2, int w = 400]) => Text(x, style: ms(s, w, c), maxLines: 1, overflow: TextOverflow.ellipsis, softWrap: false);
/// An engraved caption: condensed, capitals, a dark line of shadow above.
Widget cap(String x, [Color c = ink3, double s = 9.5]) => Text(x.toUpperCase(), maxLines: 1, softWrap: false, style: ts(s, 620, c, 84, .55).copyWith());

// ---------- materials ----------
class Mat {
  const Mat({required this.top, required this.bottom, this.r = 4, this.lit = 0, this.edge = const Color(0x00000000), this.drop = const [], this.inset = 0, this.lip = 0, this.grain = 0});
  final Color top, bottom, edge; final double r, lit, inset, lip, grain; final List<BoxShadow> drop;
  Mat copy({Color? top, Color? bottom, Color? edge, double? r, double? lit, double? inset, double? lip, List<BoxShadow>? drop}) => Mat(top: top ?? this.top, bottom: bottom ?? this.bottom, edge: edge ?? this.edge, r: r ?? this.r, lit: lit ?? this.lit, inset: inset ?? this.inset, lip: lip ?? this.lip, drop: drop ?? this.drop);
}
const chassis = Mat(top: Color(0xFF1A2029), bottom: Color(0xFF12161D), r: 0, lit: .05);
const plate = Mat(top: Color(0xFF252D38), bottom: Color(0xFF1B212A), r: 5, lit: .10, edge: Color(0xFF080B10), drop: [BoxShadow(color: Color(0x66000000), blurRadius: 5, offset: Offset(0, 2))]);
const plateSel = Mat(top: Color(0xFF2E3B4D), bottom: Color(0xFF212B39), r: 5, lit: .16, edge: Color(0xFF080B10), drop: [BoxShadow(color: Color(0x80000000), blurRadius: 10, offset: Offset(0, 4))]);
const bay = Mat(top: Color(0xFF0A0F17), bottom: Color(0xFF0D131D), r: 5, inset: .6, lip: .07, edge: Color(0xFF04070B));
const key = Mat(top: Color(0xFF364150), bottom: Color(0xFF28313E), r: 4, lit: .14, edge: Color(0xFF090D13), drop: [BoxShadow(color: Color(0x59000000), blurRadius: 2, offset: Offset(0, 1.5))]);
const keyDown = Mat(top: Color(0xFF141A23), bottom: Color(0xFF1A212C), r: 4, inset: .7, lip: .06, edge: Color(0xFF06090E));
const keyVolt = Mat(top: Color(0xFF3C60D0), bottom: Color(0xFF3153BE), r: 4, lit: .14, edge: Color(0xFF1A2E6E), drop: [BoxShadow(color: Color(0x66000000), blurRadius: 3, offset: Offset(0, 1.5))]);
const keyAmber = Mat(top: Color(0xFF6E5732), bottom: Color(0xFF64502F), r: 4, lit: .12, edge: Color(0xFF2E2412), drop: [BoxShadow(color: Color(0x66000000), blurRadius: 3, offset: Offset(0, 1.5))]);

class MatPainter extends CustomPainter {
  const MatPainter(this.m);
  final Mat m;
  @override
  void paint(Canvas c, Size s) {
    final rect = Offset.zero & s; final rr = RRect.fromRectAndRadius(rect, Radius.circular(m.r));
    for (final d in m.drop) { c.drawRRect(rr.shift(d.offset).inflate(d.spreadRadius), Paint()..color = d.color..maskFilter = MaskFilter.blur(BlurStyle.normal, d.blurSigma)); }
    if (m.lip > 0) c.drawRRect(rr.shift(const Offset(0, 1)), Paint()..color = a(const Color(0xFFFFFFFF), m.lip));
    c.drawRRect(rr, Paint()..shader = ui.Gradient.linear(rect.topCenter, rect.bottomCenter, [m.top, m.bottom]));
    c.save(); c.clipRRect(rr);
    if (m.inset > 0) {
      c.drawRect(rect.topLeft & Size(s.width, math.min(5, s.height)), Paint()..shader = ui.Gradient.linear(Offset.zero, const Offset(0, 5), [a(const Color(0xFF000000), m.inset * .75), const Color(0x00000000)]));
      for (final (x0, x1) in <(double, double)>[]) { c.drawRect(Rect.fromLTRB(math.min(x0, x1), 0, math.max(x0, x1), s.height), Paint()..shader = ui.Gradient.linear(Offset(x0, 0), Offset(x1, 0), [a(const Color(0xFF000000), m.inset * .4), const Color(0x00000000)])); }
    }
    if (m.lit > 0) c.drawRRect(rr.deflate(m.edge.a > 0 ? 1.4 : .5), Paint()..style = PaintingStyle.stroke..strokeWidth = 1..shader = ui.Gradient.linear(rect.topCenter, rect.bottomCenter, [a(const Color(0xFFDCEBFF), m.lit), a(const Color(0xFFDCEBFF), m.lit * .16), a(const Color(0xFF000000), m.lit * 1.1)], [0, math.min(.5, 10 / s.height), 1]));
    c.restore();
    if (m.edge.a > 0) c.drawRRect(rr.deflate(.5), Paint()..style = PaintingStyle.stroke..strokeWidth = 1..color = m.edge);
  }
  @override
  bool shouldRepaint(MatPainter o) => o.m != m;
}
class Box extends StatelessWidget {
  const Box(this.m, {super.key, this.child, this.w, this.h, this.pad = EdgeInsets.zero, this.align});
  final Mat m; final Widget? child; final double? w, h; final EdgeInsets pad; final AlignmentGeometry? align;
  @override
  Widget build(BuildContext context) => CustomPaint(painter: MatPainter(m), child: Container(width: w, height: h, padding: pad, alignment: align, child: child));
}

// ---------- icons: 16-grid strokes ----------
const _icons = <String, String>{
  'play': 'M5 3.5 L12.5 8 L5 12.5 Z', 'pause': 'M5.5 4 V12 M10.5 4 V12', 'stop': 'M4.5 4.5 H11.5 V11.5 H4.5 Z', 'steer': 'M3 8 H11 M8 4.5 L11.5 8 L8 11.5',
  'search': 'M7 2.8 A4.2 4.2 0 1 0 7 11.2 A4.2 4.2 0 1 0 7 2.8 M10.2 10.2 L13.2 13.2', 'file': 'M4 2.5 H9.5 L12 5 V13.5 H4 Z M9.5 2.5 V5 H12', 'edit': 'M3 13 L3.6 10.2 L10.6 3.2 L12.8 5.4 L5.8 12.4 Z',
  'term': 'M2.5 3.5 H13.5 V12.5 H2.5 Z M5 6.2 L7 8 L5 9.8 M8 10 H11', 'branch': 'M5 3 V13 M5 10 C5 7.5 11 8.5 11 5 M11 5 V3', 'plus': 'M8 3.5 V12.5 M3.5 8 H12.5', 'check': 'M3.5 8.5 L6.5 11.5 L12.5 4.8', 'down': 'M4.5 6.5 L8 10 L11.5 6.5', 'right': 'M6.5 4.5 L10 8 L6.5 11.5',
  'shield': 'M8 2.5 L12.8 4.3 V8 C12.8 11 10.4 12.8 8 13.6 C5.6 12.8 3.2 11 3.2 8 V4.3 Z', 'clip': 'M11.5 7 L7.5 11 A2.4 2.4 0 0 1 4.1 7.6 L8.6 3.1 A1.6 1.6 0 0 1 10.9 5.4 L6.6 9.7', 'web': 'M8 2.5 A5.5 5.5 0 1 0 8 13.5 A5.5 5.5 0 1 0 8 2.5 M2.5 8 H13.5 M8 2.5 C10.5 5 10.5 11 8 13.5 C5.5 11 5.5 5 8 2.5', 'fork': 'M4.5 3 V6.5 C4.5 8.5 11.5 7.5 11.5 10 V13 M4.5 6.5 V13', 'x': 'M4.5 4.5 L11.5 11.5 M11.5 4.5 L4.5 11.5', 'list': 'M3 4.5 H13 M3 8 H13 M3 11.5 H9', 'think': 'M3 11 C5 11 5 5 8 5 C11 5 11 11 13 11',
};
Path _parse(String d) {
  final p = Path(); final tk = RegExp(r'[MLHVCAZ]|-?\d*\.?\d+').allMatches(d).map((m) => m[0]!).toList(); var i = 0; var cmd = ''; var x = 0.0, y = 0.0;
  double n() => double.parse(tk[i++]);
  while (i < tk.length) {
    if (RegExp('[A-Z]').hasMatch(tk[i])) cmd = tk[i++];
    switch (cmd) {
      case 'M': x = n(); y = n(); p.moveTo(x, y); cmd = 'L';
      case 'L': x = n(); y = n(); p.lineTo(x, y);
      case 'H': x = n(); p.lineTo(x, y);
      case 'V': y = n(); p.lineTo(x, y);
      case 'C': final q = [for (var k = 0; k < 6; k++) n()]; p.cubicTo(q[0], q[1], q[2], q[3], q[4], q[5]); x = q[4]; y = q[5];
      case 'A': final rx = n(), ry = n(); n(); final l = n(), s = n(); x = n(); y = n(); p.arcToPoint(Offset(x, y), radius: Radius.elliptical(rx, ry), largeArc: l == 1, clockwise: s == 1);
      case 'Z': p.close();
    }
  }
  return p;
}
class _IP extends CustomPainter { const _IP(this.n, this.c, this.fill, this.w); final String n; final Color c; final bool fill; final double w;
  @override void paint(Canvas v, Size s) { v.scale(s.width / 16); v.drawPath(_parse(_icons[n]!), Paint()..color = c..style = fill ? PaintingStyle.fill : PaintingStyle.stroke..strokeWidth = w..strokeCap = StrokeCap.round..strokeJoin = StrokeJoin.round); }
  @override bool shouldRepaint(_IP o) => true; }
Widget ic(String n, {double s = 14, Color c = ink2, bool fill = false, double w = 1.35}) => SizedBox.square(dimension: s, child: CustomPaint(painter: _IP(n, c, fill, w)));

// ---------- parts ----------
/// A lamp: a small lens set in the surface. Lit lamps carry a hot core, not a halo.
class _LampP extends CustomPainter { const _LampP(this.c, this.on); final Color c; final bool on;
  @override void paint(Canvas v, Size s) { final r = RRect.fromRectAndRadius(Offset.zero & s, Radius.circular(s.height / 2));
    
    final i = r.deflate(1);
    if (on) { v.drawRRect(i, Paint()..color = c); }
    else { v.drawRRect(i, Paint()..color = const Color(0xFF454C58)); } }
  @override bool shouldRepaint(_LampP o) => true; }
Widget lamp(Color c, {bool on = true, double w = 7, double h = 7}) => CustomPaint(size: Size(w, h), painter: _LampP(c, on));

/// A segmented meter in a recessed slot.
class _MeterP extends CustomPainter { const _MeterP(this.v, this.n, this.vert, this.hot); final double v; final int n; final bool vert; final Color hot;
  @override void paint(Canvas c, Size s) {
    MatPainter(bay.copy(r: 2)).paint(c, s); final l = vert ? s.height - 4 : s.width - 4; final seg = l / n;
    for (var i = 0; i < n; i++) { final on = i < (v * n).round(); final col = on ? Color.lerp(const Color(0xFF56607A), hot, i / n)! : const Color(0xFF131B26);
      final r = vert ? Rect.fromLTWH(2, s.height - 2 - (i + 1) * seg + .6, s.width - 4, seg - 1.2) : Rect.fromLTWH(2 + i * seg + .6, 2, seg - 1.2, s.height - 4); c.drawRect(r, Paint()..color = col);
      if (on) c.drawRect(vert ? r.topLeft & Size(r.width, .8) : r.topLeft & Size(r.width, .8), Paint()..color = const Color(0x2EFFFFFF)); } }
  @override bool shouldRepaint(_MeterP o) => true; }
Widget meter(double v, {double w = 80, double h = 8, int n = 24, bool vert = false, Color hot = volt}) => CustomPaint(size: Size(w, h), painter: _MeterP(v, n, vert, hot));

Widget btn(String icon, {Mat m = key, double w = 26, double h = 22, Color c = ink2, bool fill = false, String? label}) => Box(m, w: label == null ? w : null, h: h, pad: label == null ? EdgeInsets.zero : const EdgeInsets.symmetric(horizontal: 9), child: Row(mainAxisSize: MainAxisSize.min, mainAxisAlignment: MainAxisAlignment.center, children: [ic(icon, s: 12, c: c, fill: fill), if (label != null) ...[const SizedBox(width: 6), t(label, 12, 560, c)]]));
Widget kbd(String x) => Box(keyDown.copy(r: 3), pad: const EdgeInsets.symmetric(horizontal: 5, vertical: 1), child: Text(x, style: ms(9.5, 500, ink3)));
Widget gap(double n) => SizedBox(width: n, height: n);

/// Axiom's mark: two facets meeting at a lit edge.
class _MarkP extends CustomPainter { @override void paint(Canvas c, Size s) { c.scale(s.width / 20);
  c.drawPath(Path()..moveTo(10, 2)..lineTo(2, 18)..lineTo(6.4, 18)..lineTo(10, 10.4)..close(), Paint()..shader = ui.Gradient.linear(const Offset(10, 2), const Offset(3, 18), [const Color(0xFFD9DDE6), const Color(0xFF8B94A6)]));
  c.drawPath(Path()..moveTo(10, 2)..lineTo(18, 18)..lineTo(13.6, 18)..lineTo(10, 10.4)..close(), Paint()..shader = ui.Gradient.linear(const Offset(10, 2), const Offset(17, 18), [const Color(0xFF4A6CD4), const Color(0xFF263E8C)]));
  c.drawRect(const Rect.fromLTWH(7.4, 15.2, 5.2, 2.8), Paint()..color = const Color(0xFF4A6CD4)); }
  @override bool shouldRepaint(covariant CustomPainter o) => false; }
Widget mark([double s = 18]) => CustomPaint(size: Size.square(s), painter: _MarkP());

// ---------- scenario (static specimen text; nothing here is live) ----------
class Ag { const Ag(this.name, this.role, this.model, this.harness, this.conn, this.state, this.now, this.ctx, this.time, this.files, this.acts); final String name, role, model, harness, conn, state, now, time; final double ctx; final int files; final List<(String, String)> acts;
  Color get c => switch (state) { 'running' => volt, 'asks' => amber, 'failed' => red, 'done' => green, _ => ink4 }; }
const agents = [
  Ag('Lead', 'Coordinator', 'Opus 5.5', 'Axiom', 'Anthropic API', 'running', 'Reading review notes', .34, '12:41', 0, [('list', 'plan  5 of 7 done'), ('fork', 'assign  Reviewer'), ('think', 'Reading review notes')]),
  Ag('Architect', 'Design', 'Opus 5.5', 'Claude Code', 'Claude Max', 'done', 'Wrote docs/refresh.md', .52, '06:08', 1, [('file', 'session.ts  1–220'), ('search', 'rotate(  14 hits'), ('edit', 'docs/refresh.md')]),
  Ag('Builder', 'Implement', 'GPT-5.6', 'Codex', 'ChatGPT Pro', 'running', 'Editing refresh.ts', .71, '09:55', 3, [('edit', 'refresh.ts  +42 −18'), ('term', 'pnpm test auth'), ('edit', 'refresh.ts  +3 −1')]),
  Ag('Surveyor', 'Read', 'Gemini 3.5 Pro', 'Gemini CLI', 'Google AI Pro', 'running', 'Mapping call sites', .28, '03:17', 0, [('search', 'lock.acquire  61'), ('file', 'queue/worker.ts'), ('file', 'queue/retry.ts')]),
  Ag('Tester', 'Verify', 'Qwen3 4B', 'Axiom', 'LM Studio, local', 'asks', 'Wants to run a migration', .16, '02:20', 0, [('term', 'vitest  118 passed'), ('term', 'vitest  2 failed'), ('shield', 'pnpm db:migrate')]),
  Ag('Reviewer', 'Review', 'Sonnet 5.5', 'Claude Code', 'Claude Max', 'waiting', 'Waits for Builder', .04, '00:00', 0, [('list', 'depends on Builder')]),
];
const diff = [(' ', 41, 'async refresh(account: Account) {'), ('-', 42, '  if (!expired(session.token)) return session;'), ('-', 43, '  const lock = await this.locks.acquire(account.id);'), ('+', 42, '  return this.inflight.run(account.id, async () => {'), ('+', 43, '    await using lock = await this.locks.acquire(account.id);'), ('+', 44, '    const session = await this.store.read(account.id);'), ('+', 45, '    if (!expired(session.token)) return session;'), (' ', 46, '    const next = await this.provider.rotate(session);'), ('+', 47, '    await this.store.write(account.id, next);'), ('+', 48, '    return next;'), ('+', 49, '  });'), (' ', 50, '}')];
Widget diffView({double s = 11.5, int n = 12}) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [for (final (k, l, x) in diff.take(n)) Container(color: k == '+' ? const Color(0x1F4FA57F) : k == '-' ? const Color(0x24D45A52) : null, child: Row(children: [
  Container(width: 2, height: s * 1.6, color: k == '+' ? green : k == '-' ? red : null), SizedBox(width: 30, child: Text('$l', textAlign: TextAlign.right, style: ms(s - 1.5, 400, ink4))), SizedBox(width: 18, child: Text(k, textAlign: TextAlign.center, style: ms(s, 500, k == '+' ? green : red))), Expanded(child: Text.rich(code(x, s), maxLines: 1, softWrap: false, overflow: TextOverflow.clip))]))]);
TextSpan code(String x, double s) { final out = <TextSpan>[]; final re = RegExp(r"(\b(?:async|await|using|const|return|if)\b)|('[^']*')|(\b[a-z]\w*(?=\())|(\b[A-Z]\w+)");
  var p = 0; for (final k in re.allMatches(x)) { if (k.start > p) out.add(TextSpan(text: x.substring(p, k.start))); out.add(TextSpan(text: k[0], style: TextStyle(color: k[1] != null ? const Color(0xFF9AA5C9) : k[2] != null ? const Color(0xFFA3B59C) : k[3] != null ? const Color(0xFFD5DAE3) : const Color(0xFFB3A6C4)))); p = k.end; }
  out.add(TextSpan(text: x.substring(p))); return TextSpan(style: ms(s, 400, const Color(0xFFA5ADBA), 1.6), children: out); }
