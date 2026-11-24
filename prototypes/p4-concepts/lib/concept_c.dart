// Concept C, "Folio": the conversation is a document; work sits beside the sentence that caused it.
// Language: charcoal and slate sheets stacked under one lamp; type carries hierarchy, few boxes.
import 'dart:ui' as ui;
import 'package:flutter/widgets.dart';
import 'kit.dart';

const desk = Color(0xFF0E1116);
const sheet = Mat(top: Color(0xFF1E232B), bottom: Color(0xFF191D24), r: 3, lit: .09, edge: Color(0xFF0A0C10), drop: [BoxShadow(color: Color(0x8C000000), blurRadius: 24, offset: Offset(0, 10)), BoxShadow(color: Color(0x59000000), blurRadius: 3, offset: Offset(0, 1))]);
const under = Mat(top: Color(0xFF171B21), bottom: Color(0xFF15181E), r: 3, lit: .06, edge: Color(0xFF0A0C10), drop: [BoxShadow(color: Color(0x66000000), blurRadius: 10, offset: Offset(0, 4))]);
const rule = Color(0xFF2A313B);

class _Spine extends CustomPainter { const _Spine(this.marks); final List<(double, Color, bool)> marks;
  @override void paint(Canvas c, Size s) { c.drawLine(Offset(s.width / 2, 0), Offset(s.width / 2, s.height), Paint()..color = rule..strokeWidth = 1); for (final (y, col, big) in marks) { c.drawRRect(RRect.fromRectAndRadius(Rect.fromCenter(center: Offset(s.width / 2, y), width: big ? 9 : 5, height: big ? 3 : 2), const Radius.circular(1)), Paint()..color = col); } }
  @override bool shouldRepaint(covariant CustomPainter o) => true; }

Widget work(String k, String n, String d, String r, [Color c = ink3]) => Padding(padding: const EdgeInsets.only(bottom: 5), child: Row(children: [ic(k, s: 13, c: steel), gap(8), t(n, 12, 560, ink2), gap(7), Expanded(child: m(d, 11, ink3)), gap(6), t(r, 11, 450, c)]));
Widget row(Widget prose, Widget side, {double top = 0}) => Padding(padding: EdgeInsets.only(top: top), child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [SizedBox(width: 560, child: prose), const SizedBox(width: 56), Expanded(child: side)]));
Widget prose(String x, {Color c = const Color(0xFFD7DDE5), int w = 400}) => Text(x, style: ts(14.5, w, c, 100, .2, 1.62));

class ConceptC extends StatelessWidget {
  const ConceptC({super.key});
  @override
  Widget build(BuildContext context) => Container(decoration: const BoxDecoration(gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Color(0xFF13171D), desk])), child: Column(children: [
    SizedBox(height: 44, child: Padding(padding: const EdgeInsets.only(left: 80, right: 16), child: Row(children: [mark(17), gap(12), t('meridian', 13, 600, ink1), gap(26), for (final (x, on) in [('Double token rotation', true), ('Queue backpressure', false), ('Release notes 4.2', false)]) Padding(padding: const EdgeInsets.only(right: 22), child: t(x, 13, on ? 560 : 420, on ? ink1 : ink3)), ic('plus', s: 12, c: ink3), const Spacer(), t('12:41', 12, 520, ink2), gap(18), t(r'$1.84', 12, 520, ink2), gap(18), t('Claude Max 62%', 12, 450, ink3), gap(18), ic('search', c: ink3)]))),
    Expanded(child: Padding(padding: const EdgeInsets.fromLTRB(14, 0, 14, 14), child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      SizedBox(width: 214, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        for (final (i, g) in agents.indexed) Padding(padding: EdgeInsets.only(left: i == 0 ? 0 : 14, bottom: 5, right: i == 2 ? 0 : 8), child: Box(i == 2 ? sheet : under, pad: const EdgeInsets.fromLTRB(10, 8, 10, 9), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Row(children: [lamp(g.c, on: g.state != 'waiting', w: 6, h: 6), gap(8), t(g.name, 13, 620, i == 2 ? ink1 : ink2), const Spacer(), t(g.time, 11, 400, ink4)]), gap(3), Padding(padding: const EdgeInsets.only(left: 14), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t(g.now, 11.5, 420, g.state == 'asks' ? amber : ink3), gap(1), t('${g.model}, ${g.harness}', 11, 400, ink4)]))]))),
        const Spacer(), Padding(padding: const EdgeInsets.only(left: 4, right: 12), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Plan', 12, 600, ink2), gap(6), for (final (d, x) in [(2, 'Design note'), (2, 'Failing test'), (1, 'Single-flight guard'), (1, 'Migrate sessions'), (0, 'Independent review')]) Padding(padding: const EdgeInsets.only(bottom: 5), child: Row(children: [ic(d == 2 ? 'check' : 'right', s: 11, c: d == 2 ? green : d == 1 ? volt : ink4), gap(8), t(x, 12, 420, d == 1 ? ink1 : ink3)]))])),
      ])),
      Expanded(child: Box(sheet, child: Column(children: [
        Expanded(child: Stack(children: [
          Positioned(left: 34 + 560 + 14, top: 24, bottom: 0, width: 28, child: CustomPaint(painter: _Spine([(12, steel, true), (108, steel, false), (131, steel, false), (154, red, false), (210, volt, true), (330, green, false), (560, amber, true)]))),
          Padding(padding: const EdgeInsets.fromLTRB(34, 24, 26, 0), child: ClipRect(child: OverflowBox(alignment: Alignment.topLeft, maxHeight: 1000, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            row(Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Lead', 12, 620, steel), gap(4), prose('Two tabs can both see an expired token and rotate it twice. Follow docs/refresh.md: re-check expiry inside the lock and make rotation single-flight per account.', c: const Color(0xFFAFC6E0))]), Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Attached', 11.5, 450, ink4), gap(4), work('file', 'docs/refresh.md', '', '84 lines')])),
            row(Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Builder', 12, 620, ink2), gap(4), prose('I will read the refresh path and its callers first, then run the new test to see it fail.')]), Column(children: [work('file', 'Read', 'src/auth/refresh.ts', '220 lines'), work('search', 'Search', 'locks.acquire', '9 in 4 files'), work('term', 'Test', 'pnpm test auth', '2 failed', red)]), top: 22),
            row(Column(crossAxisAlignment: CrossAxisAlignment.start, children: [prose('The expiry check happens before the lock is held, so the second tab acts on a token the first already replaced. I am moving the read inside the critical section and adding a single-flight map keyed by account.'), gap(10), prose('Both writers now go through one entry point. The store keeps its existing interface, so nothing outside the auth module changes.'), gap(10), prose('The migration adds a rotated_at column. Tester is asking to run it against the test database.')]),
              Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Row(children: [ic('edit', s: 13, c: steel), gap(8), m('src/auth/refresh.ts', 11.5, ink1), const Spacer(), m('+8', 11, green), gap(6), m('−2', 11, red)]), gap(6), Box(const Mat(top: Color(0xFF12161B), bottom: Color(0xFF14181E), r: 3, inset: .5, lip: .06, edge: Color(0xFF0A0C10)), pad: const EdgeInsets.symmetric(vertical: 5), child: diffView(s: 10.5, n: 12)), gap(16), Row(children: [ic('shield', c: amber, s: 13), gap(8), t('Tester asks', 12, 600, amber), gap(6), t('outside the sandbox', 12, 400, ink3)]), gap(6), m('pnpm db:migrate --env test', 11.5, ink1), gap(8), Row(children: [btn('check', label: 'Allow once', h: 26), gap(5), btn('list', label: 'Always here', h: 26), gap(5), btn('x', label: 'Deny', h: 26)])]), top: 22),
          ])))),
        ])),
        Container(height: 1, color: const Color(0xFF12161B)), Container(height: 1, color: const Color(0xFF2B323C)),
        Padding(padding: const EdgeInsets.fromLTRB(34, 14, 22, 16), child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t('Steer Builder. It reads this after the current tool call.', 14.5, 400, ink3), gap(16), Row(children: [for (final (k, v) in [('Harness', 'Codex'), ('Model', 'GPT-5.6'), ('Account', 'ChatGPT Pro'), ('Effort', 'High'), ('Access', 'Worktree only')]) Padding(padding: const EdgeInsets.only(right: 20), child: Row(children: [t(k, 12, 400, ink4), gap(6), t(v, 12, 560, ink2), gap(4), ic('down', s: 9, c: ink4)]))])])), btn('clip', h: 28, w: 32), gap(6), btn('steer', m: keyVolt, c: const Color(0xFFFFFFFF), label: 'Queue', h: 28)])),
      ]))),
    ]))),
  ]));
}
// ignore: unused_element
final _ = ui.Offset.zero;
