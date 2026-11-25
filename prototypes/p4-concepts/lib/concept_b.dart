// Concept B, "Score": the run is an arrangement. Lanes are agents, clips are work, cords are delegation.
// Language: chart-room navy, flat planes, hairlines; depth only where something can be moved.
import 'dart:ui' as ui;
import 'package:flutter/widgets.dart';
import 'kit.dart';

const ground = Color(0xFF0B111B), plane = Color(0xFF101826), plane2 = Color(0xFF152032), hair = Color(0xFF1E2B3F), hair2 = Color(0xFF2A3B55);
class Clip { const Clip(this.s, this.e, this.kind, this.label, [this.hot = false]); final double s, e; final String kind, label; final bool hot; }
const lanes = <List<Clip>>[
  [Clip(0, .06, 'think', 'Plan'), Clip(.07, .12, 'list', 'Assign'), Clip(.34, .40, 'think', 'Read note'), Clip(.41, .44, 'list', 'Assign'), Clip(.80, .86, 'think', 'Review notes')],
  [Clip(.10, .16, 'file', 'Read 9 files'), Clip(.165, .22, 'search', 'rotate('), Clip(.23, .34, 'edit', 'docs/refresh.md')],
  [Clip(.43, .50, 'file', 'Read'), Clip(.505, .60, 'edit', 'refresh.ts +42'), Clip(.61, .69, 'term', 'pnpm test auth', true), Clip(.70, .86, 'edit', 'refresh.ts +8 −2')],
  [Clip(.12, .30, 'search', 'Map call sites'), Clip(.31, .44, 'file', 'Read 31 files'), Clip(.60, .86, 'search', 'Queue and retry paths')],
  [Clip(.13, .20, 'term', 'Reproduce'), Clip(.21, .29, 'edit', 'Failing test'), Clip(.62, .70, 'term', 'vitest', true), Clip(.71, .80, 'term', 'vitest'), Clip(.80, .86, 'ask', 'db:migrate')],
  [],
];
const cords = [(0, .12, 1, .10), (0, .12, 3, .12), (0, .12, 4, .13), (1, .34, 0, .34), (0, .44, 2, .43), (2, .60, 4, .62)];
Color clipC(Clip c) => c.hot ? const Color(0xFF8A4542) : switch (c.kind) { 'edit' => const Color(0xFF2F66A8), 'term' => const Color(0xFF3C5570), 'think' => const Color(0xFF4A5170), 'ask' => const Color(0xFF9A7636), 'list' => const Color(0xFF46566A), _ => const Color(0xFF2E4560) };

class ScoreP extends CustomPainter {
  @override
  void paint(Canvas c, Size s) {
    final h = s.height / 6; double x(double v) => v * s.width;
    for (var i = 0; i < 6; i++) { c.drawRect(Rect.fromLTWH(0, i * h, s.width, h), Paint()..color = i == 2 ? const Color(0xFF131F31) : i.isEven ? const Color(0xFF0D1521) : const Color(0xFF0F1825)); c.drawLine(Offset(0, i * h), Offset(s.width, i * h), Paint()..color = hair); }
    for (var i = 0; i <= 28; i++) { c.drawLine(Offset(x(i / 28), 0), Offset(x(i / 28), s.height), Paint()..color = a(hair, i % 4 == 0 ? 1 : .42)); }
    c.drawRect(Rect.fromLTRB(x(.86), 0, s.width, s.height), Paint()..color = const Color(0x66070B12));
    for (final (l, v, l2, v2) in cords) {
      final p = Offset(x(v), (l + .5) * h), q = Offset(x(v2) + 2, (l2 + .5) * h); final mid = (p.dy + q.dy) / 2;
      final path = Path()..moveTo(p.dx, p.dy)..cubicTo(p.dx + 26, mid, q.dx - 26, mid, q.dx, q.dy);
      c.drawPath(path, Paint()..style = PaintingStyle.stroke..strokeWidth = 3.4..color = const Color(0xAA060A10)); c.drawPath(path, Paint()..style = PaintingStyle.stroke..strokeWidth = 1.3..color = const Color(0xFF7FA6D2)); c.drawCircle(q, 2.6, Paint()..color = const Color(0xFF9CC4F5)); c.drawCircle(p, 2, Paint()..color = const Color(0xFF7FA6D2));
    }
    for (final (i, cs) in lanes.indexed) { for (final k in cs) {
      final r = RRect.fromRectAndRadius(Rect.fromLTRB(x(k.s) + 1, i * h + 5, x(k.e) - 1, (i + 1) * h - 5), const Radius.circular(3)); final col = clipC(k); final live = k.e > .855;
      c.drawRRect(r.shift(const Offset(0, 1.5)), Paint()..color = const Color(0x73000000)..maskFilter = const ui.MaskFilter.blur(BlurStyle.normal, 1.5));
      c.drawRRect(r, Paint()..shader = ui.Gradient.linear(r.outerRect.topCenter, r.outerRect.bottomCenter, [Color.lerp(col, const Color(0xFFFFFFFF), live ? .12 : .03)!, Color.lerp(col, ground, live ? .18 : .34)!]));
      c.drawRRect(r.deflate(.5), Paint()..style = PaintingStyle.stroke..strokeWidth = 1..color = a(const Color(0xFFCFE4FF), live ? .30 : .12));
      c.drawRect(Rect.fromLTWH(r.left + 1, r.top + 13.5, r.width - 2, 1), Paint()..color = const Color(0x33000000));
      c.save(); c.clipRRect(r);
      // activity trace inside the clip: token rate
      final seed = (k.s * 991).round(); final tp = Path(); for (var dx = 0.0; dx < r.width; dx += 2.5) { final v = ((seed * 31 + dx.toInt() * 17 + (dx * dx).toInt()) % 23) / 23; final y = r.bottom - 3 - v * (r.height - 22); dx == 0 ? tp.moveTo(r.left, y) : tp.lineTo(r.left + dx, y); }
      c.drawPath(tp, Paint()..style = PaintingStyle.stroke..strokeWidth = 1..color = a(const Color(0xFFDCEBFF), .34));
      final tp2 = TextPainter(text: TextSpan(text: k.label, style: ts(10, 540, const Color(0xFFDDE8F5), 92)), textDirection: TextDirection.ltr, maxLines: 1)..layout(); if (r.width > 26) tp2.paint(c, Offset(r.left + 5, r.top - .5)); c.restore();
    } }
    final px = x(.86); c.drawLine(Offset(px, 0), Offset(px, s.height), Paint()..color = volt..strokeWidth = 1.2);
    c.drawPath(Path()..moveTo(px - 5, 0)..lineTo(px + 5, 0)..lineTo(px, 7)..close(), Paint()..color = volt);
  }
  @override
  bool shouldRepaint(covariant CustomPainter o) => false;
}
Widget line({bool v = false}) => Container(width: v ? 1 : null, height: v ? null : 1, color: hair);

class ConceptB extends StatelessWidget {
  const ConceptB({super.key});
  @override
  Widget build(BuildContext context) => ColoredBox(color: ground, child: Column(children: [
    Container(height: 42, color: plane, padding: const EdgeInsets.only(left: 78, right: 12), child: Row(children: [mark(17), gap(10), t('meridian', 13, 640, ink1), t('  /  ', 13, 400, ink4), t('Double token rotation', 13, 450), const Spacer(), Text('00:12:41', style: ms(15, 400, const Color(0xFFBBD6F5), 1.2)), gap(14), ic('pause', c: ink1, s: 15), gap(12), ic('stop', c: ink2, s: 15), const Spacer(), t(r'$1.84', 12, 560, ink1), gap(5), t('of  \$10', 12, 400, ink3), gap(16), t('Claude Max 62%', 12, 450), gap(16), t('ChatGPT Pro 31%', 12, 450), gap(18), ic('search', c: ink2)])),
    line(),
    SizedBox(height: 378, child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      SizedBox(width: 216, child: Column(children: [Container(height: 24, color: plane, padding: const EdgeInsets.symmetric(horizontal: 10), child: Row(children: [cap('Agents'), const Spacer(), ic('plus', s: 12, c: ink3)])), line(),
        for (final (i, g) in agents.indexed) Expanded(child: Container(decoration: BoxDecoration(color: i == 2 ? const Color(0xFF182538) : plane, border: Border(bottom: const BorderSide(color: hair), left: BorderSide(color: i == 2 ? volt : plane, width: 2))), padding: const EdgeInsets.fromLTRB(9, 0, 9, 0), child: Row(children: [Expanded(child: Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [Row(children: [lamp(g.c, on: g.state != 'waiting', w: 6, h: 6), gap(7), t(g.name, 13, 620, ink1), gap(6), Flexible(child: t(g.model, 11.5, 400, ink3))]), gap(2), Padding(padding: const EdgeInsets.only(left: 13), child: t('${g.harness}, ${g.conn}', 11, 400, ink4))])), Column(mainAxisAlignment: MainAxisAlignment.center, children: [ic(g.state == 'running' ? 'pause' : g.state == 'asks' ? 'shield' : 'play', s: 12, c: g.state == 'asks' ? amber : ink3)])])))])),
      line(v: true),
      Expanded(child: Column(children: [Container(height: 24, color: plane, child: Row(children: [for (var i = 0; i < 7; i++) Expanded(child: Padding(padding: const EdgeInsets.only(left: 4), child: m('00:${(i * 2).toString().padLeft(2, '0')}', 10, ink4)))])), line(), Expanded(child: ClipRect(child: CustomPaint(size: Size.infinite, painter: ScoreP())))])),
    ])),
    line(),
    Expanded(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Expanded(flex: 60, child: Column(children: [
        Container(height: 34, color: plane, padding: const EdgeInsets.symmetric(horizontal: 14), child: Row(children: [t('Builder', 13, 640, ink1), gap(10), t('refresh-guard', 12, 400, ink3), const Spacer(), for (final x in ['Conversation', 'Changes 3', 'Terminal', 'Context']) Padding(padding: const EdgeInsets.only(left: 18), child: t(x, 12, x == 'Conversation' ? 600 : 450, x == 'Conversation' ? ink1 : ink3))])), line(),
        Expanded(child: Padding(padding: const EdgeInsets.fromLTRB(18, 14, 60, 0), child: ClipRect(child: OverflowBox(alignment: Alignment.topLeft, maxHeight: 700, child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('Two tabs can both see an expired token and rotate it twice. Follow docs/refresh.md: re-check expiry inside the lock and make rotation single-flight per account.', style: ts(14, 450, const Color(0xFF8FB7E6), 100, .18, 1.55)), gap(12),
          for (final (k, n, d, r) in [('file', 'Read', 'src/auth/refresh.ts', '220 lines'), ('search', 'Search', 'locks.acquire', '9 in 4 files'), ('term', 'Test', 'pnpm test auth', '2 failed')]) Padding(padding: const EdgeInsets.only(bottom: 4), child: Row(children: [ic(k, s: 13, c: steel), gap(8), t(n, 12, 560), gap(8), Expanded(child: m(d, 11.5, ink3)), t(r, 11, 450, r.contains('fail') ? red : ink3)])), gap(10),
          Text('The expiry check happens before the lock is held, so the second tab acts on a token the first already replaced. I am moving the read inside the critical section and adding a single-flight map keyed by account.', style: ts(14, 400, const Color(0xFFD3DCE7), 100, .18, 1.55)),
        ]))))),
        Container(margin: const EdgeInsets.fromLTRB(12, 0, 12, 12), padding: const EdgeInsets.fromLTRB(12, 10, 10, 10), decoration: BoxDecoration(color: plane2, border: Border.all(color: hair2), borderRadius: BorderRadius.circular(5)), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Steer Builder. It reads this after the current tool call.', 13.5, 400, ink3), gap(14), Row(children: [for (final x in ['Codex', 'GPT-5.6', 'ChatGPT Pro', 'High effort', 'Worktree only']) Padding(padding: const EdgeInsets.only(right: 16), child: Row(children: [t(x, 12, 520, ink2), gap(4), ic('down', s: 9, c: ink4)])), const Spacer(), btn('steer', m: keyVolt, c: const Color(0xFFFFFFFF), label: 'Queue', h: 26)])])),
      ])),
      line(v: true),
      Expanded(flex: 46, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Container(height: 34, color: plane, padding: const EdgeInsets.symmetric(horizontal: 14), child: Row(children: [ic('edit', s: 13, c: steel), gap(8), m('src/auth/refresh.ts', 12, ink1), const Spacer(), m('+8', 11, green), gap(8), m('−2', 11, red)])), line(), Expanded(child: Container(color: const Color(0xFF0A101A), padding: const EdgeInsets.symmetric(vertical: 8), child: ClipRect(child: OverflowBox(alignment: Alignment.topLeft, maxHeight: 600, child: diffView(s: 12))))), line(),
        Container(color: const Color(0xFF1B1A1A), padding: const EdgeInsets.fromLTRB(14, 10, 12, 12), child: Row(children: [ic('shield', c: amber), gap(10), Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Tester asks to run outside the sandbox', 12.5, 560, ink1), gap(2), m('pnpm db:migrate --env test', 11.5, amber)])), btn('check', label: 'Allow once', h: 26), gap(5), btn('x', label: 'Deny', h: 26)]))])),
    ])),
  ]));
}
