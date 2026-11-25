// Concept D, round 2: graphite hardware, navy displays. Console's deck + Folio's parallel work column + Score as a deck mode.
import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flutter/widgets.dart';
import 'kit.dart';

// graphite (neutral, warm-free) structure; navy only inside displays; steel on keys.
const gChassisT = Color(0xFF24272C), gChassisB = Color(0xFF17191D);
const gPlate = Mat(top: Color(0xFF32363D), bottom: Color(0xFF262A30), r: 5, lit: .13, edge: Color(0xFF0B0C0F), drop: [BoxShadow(color: Color(0x80000000), blurRadius: 6, offset: Offset(0, 2.5))]);
const gPlateSel = Mat(top: Color(0xFF3B4858), bottom: Color(0xFF2B3542), r: 5, lit: .22, edge: Color(0xFF0A0C10), drop: [BoxShadow(color: Color(0x99000000), blurRadius: 12, offset: Offset(0, 5))]);
const gBay = Mat(top: Color(0xFF080D15), bottom: Color(0xFF0B121D), r: 5, inset: .8, lip: .11, edge: Color(0xFF030508));
const gKey = Mat(top: Color(0xFF474D57), bottom: Color(0xFF343941), r: 4, lit: .19, edge: Color(0xFF0D0F12), drop: [BoxShadow(color: Color(0x73000000), blurRadius: 2, offset: Offset(0, 1.5))]);
const gDown = Mat(top: Color(0xFF14171B), bottom: Color(0xFF1B1F25), r: 4, inset: .8, lip: .09, edge: Color(0xFF050608));
const gWell = Mat(top: Color(0xFF111316), bottom: Color(0xFF16181C), r: 7, inset: .9, lip: .10, edge: Color(0xFF060709));
const dInk2 = Color(0xFFB4BBC5), dInk3 = Color(0xFF878F9B), dInk4 = Color(0xFF5E6672);

class Chassis extends CustomPainter {
  @override
  void paint(Canvas c, Size s) {
    final r = Offset.zero & s;
    c.drawRect(r, Paint()..shader = ui.Gradient.linear(r.topLeft, Offset(s.width * .35, s.height), const [gChassisT, gChassisB]));
    // one lamp, above and left of centre
    c.drawRect(r, Paint()..shader = ui.Gradient.radial(Offset(s.width * .38, -120), s.width * .75, const [Color(0x1AB9CCE6), Color(0x00B9CCE6)]));
    c.drawRect(Rect.fromLTWH(0, 0, s.width, 1), Paint()..color = const Color(0x24E4EEFF));
  }
  @override
  bool shouldRepaint(covariant CustomPainter o) => false;
}
class Seam extends StatelessWidget { const Seam({super.key, this.v = false, this.len}); final bool v; final double? len;
  @override Widget build(BuildContext context) => v ? SizedBox(width: 2, height: len, child: const Row(children: [ColoredBox(color: Color(0xFF0B0C0E), child: SizedBox(width: 1, height: double.infinity)), ColoredBox(color: Color(0x14FFFFFF), child: SizedBox(width: 1, height: double.infinity))])) : const Column(children: [ColoredBox(color: Color(0xFF0B0C0E), child: SizedBox(height: 1, width: double.infinity)), ColoredBox(color: Color(0x14FFFFFF), child: SizedBox(height: 1, width: double.infinity))]); }

Widget dbtn(String i, {Mat m = gKey, double w = 28, double h = 24, Color c = const Color(0xFFCDD4DD), bool fill = false, String? label}) => btn(i, m: m, w: w, h: h, c: c, fill: fill, label: label);

/// Context meter: a scale with ticks, engraved on the plate, next to its slot.
class CtxP extends CustomPainter { const CtxP(this.v); final double v;
  @override void paint(Canvas c, Size s) {
    const slot = 7.0; MatPainter(gBay.copy(r: 1.5)).paint(c, Size(slot, s.height)); const n = 20; final seg = (s.height - 4) / n;
    for (var i = 0; i < n; i++) { final on = i < (v * n).round(); final col = !on ? const Color(0xFF141C28) : i >= 16 ? amber : Color.lerp(const Color(0xFF34557D), const Color(0xFF5FA6FA), i / 16)!; final r = Rect.fromLTWH(1.8, s.height - 2 - (i + 1) * seg + .5, slot - 3.6, seg - 1); c.drawRect(r, Paint()..color = col); }
    for (final k in [0.0, .25, .5, .75, 1.0]) { final y = 2 + (1 - k) * (s.height - 4); c.drawRect(Rect.fromLTWH(slot + 2, y - .5, k == .5 || k == 0 || k == 1 ? 4 : 2.5, 1), Paint()..color = const Color(0x40DCE6F2)); } }
  @override bool shouldRepaint(CtxP o) => o.v != v; }

Widget dStrip(int n, Ag g, {bool sel = false}) {
  final last = g.acts.length - 1; final live = g.state == 'running' || g.state == 'asks';
  return Box(sel ? gPlateSel : gPlate, pad: const EdgeInsets.fromLTRB(9, 7, 7, 8), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
    Row(children: [lamp(g.c, on: g.state != 'waiting', w: 15, h: 6), gap(7), Expanded(child: t(g.name, 13, 650, ink1)), t(g.time, 11, 450, dInk3), gap(6), Text('$n', style: ts(10, 620, dInk4, 90).copyWith(shadows: const [Shadow(color: Color(0xCC000000), offset: Offset(0, -.6))]))]),
    gap(3), Row(children: [Flexible(child: t(g.model, 11.5, 520, dInk2)), gap(6), Flexible(child: t(g.harness, 11.5, 400, dInk3))]),
    t(g.conn, 11, 400, dInk4), gap(6),
    Expanded(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Expanded(child: Box(gBay, pad: const EdgeInsets.fromLTRB(7, 4, 5, 5), child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.end, children: [for (final (i, (k, x)) in g.acts.indexed) Padding(padding: const EdgeInsets.only(top: 2.5), child: Row(children: [ic(k, s: 11, c: i == last && live ? g.c : ink4), gap(6), Expanded(child: m(x, 10, i == last && live ? ink1 : ink3))]))]))),
      gap(6), LayoutBuilder(builder: (_, b) => CustomPaint(size: Size(13, b.maxHeight), painter: CtxP(g.ctx))),
    ])),
    gap(7),
    if (g.state == 'asks') Row(children: [Expanded(child: dbtn('shield', m: keyAmber, c: const Color(0xFFFFF0D2), label: 'Answer')), gap(4), dbtn('stop')])
    else Row(children: [dbtn(g.state == 'running' ? 'pause' : 'play'), gap(4), dbtn('stop'), gap(4), dbtn('steer'), const Spacer(), if (g.files > 0) ...[ic('edit', s: 11, c: dInk3), gap(4), t('${g.files}', 11, 520, dInk2), gap(3)]]),
  ]));
}

Widget dSel(String c, String v) => Box(gKey, h: 32, pad: const EdgeInsets.symmetric(horizontal: 9), child: Row(mainAxisSize: MainAxisSize.min, children: [Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [Text(c, style: ts(9, 500, dInk3, 92, .2, 1.1)), Text(v, style: ts(12, 560, ink1, 100, 0, 1.2))]), gap(7), ic('down', s: 9, c: dInk3)]));
Widget dWork(String k, String n, String d, String r, [Color c = ink3]) => Padding(padding: const EdgeInsets.only(bottom: 5), child: Row(children: [ic(k, s: 13, c: steel), gap(8), t(n, 12, 580, ink2), gap(7), Expanded(child: m(d, 11, ink3)), gap(6), t(r, 11, 450, c)]));
Widget dProse(String x, {Color c = const Color(0xFFD6DEE8)}) => Text(x, style: ts(14, 400, c, 100, .22, 1.6));
class SpineP extends CustomPainter { const SpineP(this.marks); final List<(double, double, Color)> marks;
  @override void paint(Canvas c, Size s) { final x = s.width / 2; c.drawRect(Rect.fromLTWH(x - 1, 0, 1, s.height), Paint()..color = const Color(0xFF04070B)); c.drawRect(Rect.fromLTWH(x, 0, 1, s.height), Paint()..color = const Color(0xFF1B2636));
    for (final (y, h, col) in marks) { c.drawRRect(RRect.fromRectAndRadius(Rect.fromLTWH(x - 1.5, y, 3, h), const Radius.circular(1.5)), Paint()..color = col); } }
  @override bool shouldRepaint(covariant CustomPainter o) => true; }
Widget fRow(Widget l, Widget r, {double top = 20, required Color c, double mark = 0}) => Padding(padding: EdgeInsets.only(top: top), child: IntrinsicHeight(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [SizedBox(width: 520, child: l), SizedBox(width: 40, child: OverflowBox(maxHeight: double.infinity, child: const SizedBox())), Expanded(child: r)])));

class ConceptD extends StatelessWidget {
  const ConceptD({super.key, this.score = false});
  final bool score;
  @override
  Widget build(BuildContext context) => CustomPaint(painter: Chassis(), child: Column(children: [
    SizedBox(height: 48, child: Padding(padding: const EdgeInsets.symmetric(horizontal: 10), child: Row(children: [
      const SizedBox(width: 70), mark(19), gap(11), t('meridian', 13, 650, ink1), gap(8), ic('right', s: 9, c: dInk4), gap(8), t('Double token rotation', 13, 450, dInk2), gap(16),
      Box(gWell.copy(r: 6), pad: const EdgeInsets.all(3), child: Row(children: [dbtn('play', m: gDown, c: volt, fill: true, w: 36, h: 24), gap(3), dbtn('pause', w: 36, h: 24), gap(3), dbtn('stop', w: 36, h: 24)])), gap(9),
      Box(gBay, h: 30, pad: const EdgeInsets.symmetric(horizontal: 11), child: Row(children: [Text('12:41', style: ms(15, 500, const Color(0xFFA6CBF7), 1.2)), gap(14), lamp(volt), gap(6), t('3 running', 11.5, 450), gap(12), lamp(amber), gap(6), t('1 asking', 11.5, 520, amber)])),
      const Spacer(),
      for (final (n, v, s) in [('Claude Max', .62, '62%'), ('ChatGPT Pro', .31, '31%'), ('API', .18, r'$1.84')]) ...[Column(mainAxisAlignment: MainAxisAlignment.center, children: [SizedBox(width: 112, child: Row(children: [t(n, 11, 520, dInk2), const Spacer(), t(s, 11, 450, dInk3)])), gap(3), CustomPaint(size: const Size(112, 7), painter: HMeter(v))]), gap(16)],
      dbtn('search', w: 32, h: 28),
    ]))),
    const Seam(),
    Expanded(child: Padding(padding: const EdgeInsets.fromLTRB(8, 8, 8, 8), child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      // index
      SizedBox(width: 196, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(padding: const EdgeInsets.fromLTRB(6, 2, 4, 6), child: Row(children: [t('Sessions', 12, 600, dInk2), const Spacer(), ic('plus', s: 12, c: dInk3)])),
        for (final (x, s, on) in [('Double token rotation', 'now', true), ('Queue backpressure', '2h', false), ('Release notes 4.2', '1d', false), ('Flaky export test', '3d', false)]) Box(on ? gDown.copy(r: 4) : const Mat(top: Color(0x00000000), bottom: Color(0x00000000)), h: 28, pad: const EdgeInsets.symmetric(horizontal: 8), child: Row(children: [Expanded(child: t(x, 12.5, on ? 560 : 430, on ? ink1 : dInk2)), t(s, 11, 400, dInk4)])),
        gap(14), const Seam(), gap(10),
        Padding(padding: const EdgeInsets.fromLTRB(6, 0, 6, 6), child: Row(children: [t('Plan', 12, 600, dInk2), const Spacer(), t('4 of 7', 11, 450, dInk3)])),
        for (final (d, x, w) in [(2, 'Reproduce rotation', 5), (2, 'Survey call sites', 4), (2, 'Design note', 2), (2, 'Failing test', 5), (1, 'Single-flight guard', 3), (1, 'Migrate sessions', 5), (0, 'Independent review', 6)]) SizedBox(height: 25, child: Padding(padding: const EdgeInsets.symmetric(horizontal: 8), child: Row(children: [d == 2 ? ic('check', s: 11, c: green) : lamp(volt, on: d == 1), gap(d == 2 ? 6 : 10), Expanded(child: t(x, 12.5, 430, d == 2 ? dInk3 : d == 1 ? ink1 : dInk2)), Text('$w', style: ts(10.5, 620, dInk4, 90))]))),
      ])),
      gap(8),
      Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Expanded(child: Box(gBay.copy(r: 6), child: Column(children: [
          SizedBox(height: 36, child: Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: Row(children: [lamp(volt, w: 15, h: 6), gap(9), t('Builder', 13, 650, ink1), gap(12), ic('branch', s: 12, c: ink4), gap(5), t('refresh-guard', 12, 420, ink3), const Spacer(), for (final (x, on) in [('Conversation', true), ('Changes 3', false), ('Terminal', false), ('Context 71%', false)]) Padding(padding: const EdgeInsets.only(left: 20), child: t(x, 12, on ? 620 : 430, on ? ink1 : ink3))]))),
          Container(height: 1, color: const Color(0xFF141D2A)),
          Expanded(child: ClipRect(child: Stack(children: [
            const Positioned(left: 24 + 520 + 6, width: 28, top: 0, bottom: 0, child: CustomPaint(painter: SpineP([(20, 46, steel), (104, 12, Color(0xFF5D7CA0)), (124, 12, Color(0xFF5D7CA0)), (145, 12, red), (186, 250, volt)]))),
            Padding(padding: const EdgeInsets.fromLTRB(24, 0, 20, 0), child: OverflowBox(alignment: Alignment.topLeft, maxHeight: 1000, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              fRow(dProse('Two tabs can both see an expired token and rotate it twice. Follow docs/refresh.md: re-check expiry inside the lock and make rotation single-flight per account.', c: const Color(0xFF9FC0E6)), Column(children: [dWork('file', 'docs/refresh.md', '', '84 lines')]), c: steel, top: 16),
              fRow(dProse('I will read the refresh path and its callers first, then run the new test to see it fail.'), Column(children: [dWork('file', 'Read', 'src/auth/refresh.ts', '220 lines'), dWork('search', 'Search', 'locks.acquire', '9 in 4 files'), dWork('term', 'Test', 'pnpm test auth', '2 failed', red)]), c: steel),
              fRow(Column(crossAxisAlignment: CrossAxisAlignment.start, children: [dProse('The expiry check happens before the lock is held, so the second tab acts on a token the first already replaced. I am moving the read inside the critical section and adding a single-flight map keyed by account.'), gap(10), dProse('Both writers now go through one entry point. The store keeps its existing interface, so nothing outside the auth module changes.')]),
                Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Row(children: [ic('edit', s: 13, c: steel), gap(8), m('src/auth/refresh.ts', 11.5, ink1), const Spacer(), m('+8', 11, green), gap(6), m('−2', 11, red)]), gap(6), Container(decoration: BoxDecoration(color: const Color(0xFF0E1622), border: Border.all(color: const Color(0xFF182334)), borderRadius: BorderRadius.circular(4)), padding: const EdgeInsets.symmetric(vertical: 4), child: diffView(s: 10.5, n: score ? 7 : 12))]), c: volt),
            ]))),
          ]))),
          // the request, one line, above the composer
          Padding(padding: const EdgeInsets.fromLTRB(8, 0, 8, 8), child: Box(const Mat(top: Color(0xFF3A352B), bottom: Color(0xFF2C2A26), r: 5, lit: .16, edge: Color(0xFF14110B), drop: [BoxShadow(color: Color(0x80000000), blurRadius: 6, offset: Offset(0, 2))]), h: 40, pad: const EdgeInsets.only(left: 11, right: 7), child: Row(children: [lamp(amber, w: 15, h: 6), gap(10), t('Tester asks to run outside the sandbox', 12.5, 600, const Color(0xFFF1E2C6)), gap(12), Box(gBay.copy(r: 3), h: 24, pad: const EdgeInsets.symmetric(horizontal: 9), align: null, child: Row(mainAxisSize: MainAxisSize.min, children: [m('pnpm db:migrate --env test', 11.5, const Color(0xFFF3CF8E))])), gap(12), Expanded(child: t('writes db/test.sqlite, no network', 12, 400, const Color(0xFFB7AC97))), dbtn('check', label: 'Allow once', h: 26), gap(4), dbtn('list', label: 'Always here', h: 26), gap(4), dbtn('x', label: 'Deny', h: 26)]))),
          Padding(padding: const EdgeInsets.fromLTRB(8, 0, 8, 8), child: Box(gPlate.copy(r: 6), pad: const EdgeInsets.all(7), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Box(gBay.copy(r: 4), h: 38, pad: const EdgeInsets.fromLTRB(11, 8, 11, 6), child: Align(alignment: Alignment.topLeft, child: t('Steer Builder. It reads this after the current tool call.', 13.5, 400, ink3))), gap(7),
            Row(children: [dSel('Harness', 'Codex'), gap(4), dSel('Model', 'GPT-5.6'), gap(4), dSel('Connection', 'ChatGPT Pro'), gap(4), dSel('Effort', 'High'), gap(4), dSel('Access', 'Worktree only'), const Spacer(), dbtn('clip', w: 32, h: 32), gap(4), btn('steer', m: keyVolt, c: const Color(0xFFFFFFFF), label: 'Queue', h: 32)]),
          ]))),
        ]))),
      ])),
    ]))),
    SizedBox(height: score ? 296 : 198, child: Padding(padding: const EdgeInsets.fromLTRB(8, 0, 8, 8), child: Box(gWell, pad: const EdgeInsets.all(6), child: score ? const ScoreDeck() : Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      for (final (i, g) in agents.indexed) ...[Expanded(child: dStrip(i + 1, g, sel: i == 2)), gap(5)],
      SizedBox(width: 30, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Expanded(child: dbtn('plus', h: 200)), gap(5), dbtn('list', h: 28, m: gDown, c: volt)])),
    ])))),
  ]));
}
class HMeter extends CustomPainter { const HMeter(this.v); final double v;
  @override void paint(Canvas c, Size s) { MatPainter(gBay.copy(r: 1.5)).paint(c, s); const n = 28; final seg = (s.width - 4) / n; for (var i = 0; i < n; i++) { final on = i < (v * n).round(); c.drawRect(Rect.fromLTWH(2 + i * seg + .5, 2, seg - 1, s.height - 4), Paint()..color = on ? Color.lerp(const Color(0xFF34557D), const Color(0xFF5FA6FA), i / n)! : const Color(0xFF141C28)); } }
  @override bool shouldRepaint(HMeter o) => o.v != v; }

// --- Score deck: clips carry real quantities only (duration; token output as bar height; failures). ---
class SC { const SC(this.s, this.e, this.kind, this.label, {this.out = 0, this.fail = false}); final double s, e, out; final String kind, label; final bool fail; }
const sLanes = <List<SC>>[
  [SC(0, 1.0, 'think', 'Plan', out: .5), SC(1.1, 1.8, 'list', 'Assign 3', out: .2), SC(5.0, 5.8, 'think', 'Read note', out: .3), SC(5.9, 6.4, 'list', 'Assign 1', out: .1), SC(11.5, 12.68, 'think', 'Review notes', out: .3)],
  [SC(1.8, 2.5, 'file', 'Read 9'), SC(2.55, 3.1, 'search', 'rotate('), SC(3.2, 5.0, 'edit', 'docs/refresh.md', out: .9)],
  [SC(6.4, 7.4, 'file', 'Read 4'), SC(7.45, 9.0, 'edit', 'refresh.ts +42 −18', out: 1), SC(9.1, 10.2, 'term', 'pnpm test auth', fail: true), SC(10.3, 12.68, 'edit', 'refresh.ts +8 −2', out: .7)],
  [SC(1.8, 4.6, 'search', 'Map call sites', out: .2), SC(4.7, 6.6, 'file', 'Read 31', out: .1), SC(9.0, 12.68, 'search', 'Queue and retry paths', out: .4)],
  [SC(1.9, 3.0, 'term', 'Reproduce', out: .1), SC(3.1, 4.4, 'edit', 'Failing test', out: .5), SC(9.1, 10.3, 'term', 'vitest', fail: true), SC(10.4, 11.8, 'term', 'vitest', out: .1), SC(11.85, 12.68, 'ask', 'asks: db:migrate')],
  [],
];
class ScoreP2 extends CustomPainter { @override void paint(Canvas c, Size s) {
  const span = 14.5, head = 20.0; final lh = (s.height - head) / 6; double x(double v) => v / span * s.width;
  for (var i = 0; i < 6; i++) { c.drawRect(Rect.fromLTWH(0, head + i * lh, s.width, lh), Paint()..color = i == 2 ? const Color(0xFF101C2D) : i.isEven ? const Color(0x00000000) : const Color(0x08A9C8F0)); c.drawRect(Rect.fromLTWH(0, head + i * lh, s.width, 1), Paint()..color = const Color(0xFF131C29)); }
  for (var i = 0; i <= 14; i++) { c.drawRect(Rect.fromLTWH(x(i.toDouble()), i.isEven ? 6 : 12, 1, s.height), Paint()..color = i.isEven ? const Color(0xFF17212F) : const Color(0xFF111926)); if (i.isEven) { (TextPainter(text: TextSpan(text: '$i:00', style: ms(9.5, 400, ink4, 1)), textDirection: TextDirection.ltr)..layout()).paint(c, Offset(x(i.toDouble()) + 4, 5)); } }
  for (final (l, v, l2, v2) in [(0, 1.8, 1, 1.8), (0, 1.8, 3, 1.8), (0, 1.8, 4, 1.9), (1, 5.0, 0, 5.0), (0, 6.4, 2, 6.4), (2, 9.0, 4, 9.1)]) { final p = Offset(x(v), head + (l + .5) * lh), q = Offset(x(v2), head + (l2 + .5) * lh); final pa = Path()..moveTo(p.dx, p.dy)..lineTo(p.dx + 5, p.dy)..lineTo(p.dx + 5, q.dy)..lineTo(q.dx + (v2 == v ? 5 : 5), q.dy); c.drawPath(pa, Paint()..style = PaintingStyle.stroke..strokeWidth = 1..color = const Color(0xFF55779E)); }
  for (final (i, cs) in sLanes.indexed) { for (final k in cs) {
    final r = RRect.fromRectAndRadius(Rect.fromLTRB(x(k.s) + 1, head + i * lh + 4, x(k.e) - 1, head + (i + 1) * lh - 4), const Radius.circular(2.5)); final live = k.e > 12.6;
    final col = k.fail ? const Color(0xFF7C3F3C) : switch (k.kind) { 'edit' => const Color(0xFF2D62A0), 'ask' => const Color(0xFF8D6B30), 'think' => const Color(0xFF3B4456), 'list' => const Color(0xFF3B4456), _ => const Color(0xFF2A3D56) };
    MatPainter(Mat(top: Color.lerp(col, const Color(0xFFFFFFFF), .07)!, bottom: Color.lerp(col, const Color(0xFF000000), .22)!, r: 2.5, lit: live ? .34 : .16, edge: const Color(0xFF04070B))).paint(Canvas2(c, r.outerRect.topLeft).c, r.outerRect.size); c.restore();
    // produced tokens, as a filled bar along the foot
    if (k.out > 0) c.drawRect(Rect.fromLTWH(r.left + 2, r.bottom - 4, (r.width - 4) * math.min(1, k.out), 2), Paint()..color = const Color(0x8CCFE4FF));
    c.save(); c.clipRRect(r); (TextPainter(text: TextSpan(text: k.label, style: ts(10.5, 520, const Color(0xFFE3EDF8), 94, 0, 1.2)), textDirection: TextDirection.ltr, maxLines: 1)..layout()).paint(c, Offset(r.left + 6, r.top + (lh - 8) / 2 - 8)); c.restore();
  } }
  final px = x(12.68); c.drawRect(Rect.fromLTWH(px - .5, 0, 1.5, s.height), Paint()..color = volt); c.drawPath(Path()..moveTo(px - 5, 0)..lineTo(px + 6, 0)..lineTo(px + .5, 7)..close(), Paint()..color = volt);
} @override bool shouldRepaint(covariant CustomPainter o) => false; }
class Canvas2 { Canvas2(this.c, Offset o) { c.save(); c.translate(o.dx, o.dy); } final Canvas c; }
class ScoreDeck extends StatelessWidget { const ScoreDeck({super.key});
  @override Widget build(BuildContext context) => Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
    SizedBox(width: 196, child: Column(children: [SizedBox(height: 20, child: Row(children: [gap(6), t('Agents', 11, 600, dInk2)])), for (final (i, g) in agents.indexed) Expanded(child: Padding(padding: const EdgeInsets.only(bottom: 2), child: Box(i == 2 ? gPlateSel.copy(r: 4) : gPlate.copy(r: 4, drop: const []), pad: const EdgeInsets.symmetric(horizontal: 8), child: Row(children: [lamp(g.c, on: g.state != 'waiting', w: 15, h: 6), gap(8), t(g.name, 12.5, 620, ink1), gap(6), Expanded(child: t('${g.model}, ${g.harness}', 11, 400, dInk3)), ic(g.state == 'asks' ? 'shield' : g.state == 'running' ? 'pause' : 'play', s: 11, c: g.state == 'asks' ? amber : dInk3)]))))])),
    gap(6), Expanded(child: Box(gBay.copy(r: 4), child: ClipRect(child: CustomPaint(size: Size.infinite, painter: ScoreP2())))), gap(5),
    SizedBox(width: 30, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Expanded(child: dbtn('plus', h: 200)), gap(5), dbtn('list', h: 28)])),
  ]); }
