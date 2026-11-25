// Concept A, "Console": agents are channel strips in a deck; the selected strip owns the bay above.
import 'package:flutter/widgets.dart';
import 'kit.dart';

Widget strip(Ag g, {bool sel = false, bool lead = false}) {
  final on = g.state == 'running' || g.state == 'asks';
  return Box(sel ? plateSel : plate, pad: const EdgeInsets.fromLTRB(9, 8, 9, 9), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
    Row(children: [lamp(g.c, on: g.state != 'waiting', w: 14, h: 6), gap(7), Expanded(child: t(g.name, 13, 640, ink1)), t(g.time, 11, 450, ink3)]),
    gap(5),
    Row(children: [Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [t(g.model, 11.5, 500, ink2), gap(1), Row(children: [Flexible(child: t(g.harness, 11, 450, ink3)), t('  ·  ', 11, 400, ink4), Flexible(child: t(g.conn, 11, 400, ink3))])]))]),
    gap(8),
    Expanded(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Expanded(child: Box(bay, pad: const EdgeInsets.fromLTRB(7, 6, 6, 5), child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.end, children: [
        for (final (i, (k, x)) in g.acts.indexed) Padding(padding: const EdgeInsets.only(top: 3), child: Row(children: [ic(k, s: 11, c: i == g.acts.length - 1 ? (on ? g.c : ink3) : ink4), gap(6), Expanded(child: m(x, 10, i == g.acts.length - 1 ? ink1 : ink3))])),
      ]))),
      gap(7), Column(children: [Expanded(child: LayoutBuilder(builder: (_, b) => meter(g.ctx, w: 9, h: b.maxHeight, n: 22, vert: true, hot: g.ctx > .7 ? amber : volt))), gap(3), cap('ctx', ink4, 8)]),
    ])),
    gap(8),
    if (g.state == 'asks') Row(children: [Expanded(child: btn('shield', m: keyAmber, c: const Color(0xFFFBE7C3), label: 'Review request', h: 24)), gap(4), btn('stop', h: 24)])
    else Row(children: [btn(g.state == 'running' ? 'pause' : 'play', h: 24), gap(4), btn('stop', h: 24), gap(4), btn('steer', h: 24), const Spacer(), if (g.files > 0) ...[ic('edit', s: 11, c: ink3), gap(3), t('${g.files}', 11, 500, ink3)]]),
  ]));
}

Widget _say(String who, Widget body, {Color c = ink3}) => Padding(padding: const EdgeInsets.only(bottom: 15), child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [SizedBox(width: 74, child: Padding(padding: const EdgeInsets.only(top: 3), child: cap(who, c))), Expanded(child: body)]));
Widget _p(String x) => Text(x, style: ts(14, 400, const Color(0xFFD3DCE7), 100, 0, 1.55));
Widget _tool(String k, String n, String d, String r, {Color c = ink3}) => Padding(padding: const EdgeInsets.only(top: 4), child: Row(children: [ic(k, s: 13, c: steel), gap(8), t(n, 12, 560, ink2), gap(8), Expanded(child: m(d, 11.5, ink3)), t(r, 11, 450, c)]));

Widget sel(String c, String v, [double? w]) => Box(key, h: 34, w: w, pad: const EdgeInsets.symmetric(horizontal: 9), child: Row(mainAxisSize: MainAxisSize.min, children: [Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [cap(c, ink3, 8), Text(v, style: ts(12, 540, ink1, 100, 0, 1.2))]), gap(8), ic('down', s: 10, c: ink3)]));

class ConceptA extends StatelessWidget {
  const ConceptA({super.key});
  @override
  Widget build(BuildContext context) => Box(chassis, child: Column(children: [
    // transport
    Container(height: 46, padding: const EdgeInsets.symmetric(horizontal: 12), child: Row(children: [
      const SizedBox(width: 66), mark(), gap(10), t('meridian', 13, 640, ink1), gap(7), ic('right', s: 9, c: ink4), gap(7), t('Double token rotation', 13, 450, ink2), gap(18),
      Box(bay.copy(r: 5), pad: const EdgeInsets.all(3), child: Row(children: [btn('play', m: keyDown, c: volt, fill: true, w: 34), gap(3), btn('pause', w: 34), gap(3), btn('stop', w: 34)])),
      gap(10), Box(bay, h: 28, pad: const EdgeInsets.symmetric(horizontal: 10), child: Row(children: [Text('00:12:41', style: ms(14, 500, const Color(0xFF9CC4F5), 1.2)), gap(12), lamp(volt), gap(5), t('3 running', 11.5, 450), gap(10), lamp(amber), gap(5), t('1 asks', 11.5, 450, amber)])),
      const Spacer(),
      for (final (n, v, s) in [('Claude Max', .62, 'resets 2h 10m'), ('ChatGPT Pro', .31, 'weekly'), ('API', .18, r'$1.84 of $10')]) ...[Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [Row(children: [cap(n), gap(6), t(s, 10, 400, ink4)]), gap(4), meter(v, w: 108, h: 7, n: 30)]), gap(16)],
      Box(bay, h: 28, w: 190, pad: const EdgeInsets.symmetric(horizontal: 9), child: Row(children: [ic('search', s: 12, c: ink3), gap(7), Expanded(child: t('Go to or run', 12, 400, ink3)), kbd('⌘K')])),
    ])),
    Expanded(child: Padding(padding: const EdgeInsets.fromLTRB(8, 0, 8, 8), child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      // conversation bay
      Expanded(flex: 60, child: Box(bay, child: Column(children: [
        Container(height: 36, padding: const EdgeInsets.symmetric(horizontal: 14), child: Row(children: [lamp(volt, w: 14, h: 6), gap(8), t('Builder', 13, 640, ink1), gap(10), t('worktree  refresh-guard', 12, 400, ink3), const Spacer(), t('from Lead', 12, 400, ink3), gap(12), ic('list', c: ink3)])),
        Container(height: 1, color: const Color(0xFF151D28)),
        Expanded(child: Padding(padding: const EdgeInsets.fromLTRB(16, 16, 22, 0), child: ClipRect(child: OverflowBox(alignment: Alignment.topCenter, maxHeight: 900, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          _say('Lead', _p('Two tabs can both see an expired token and rotate it twice. Follow docs/refresh.md: re-check expiry inside the lock and make rotation single-flight per account.')),
          _say('Work', Column(children: [_tool('file', 'Read', 'src/auth/refresh.ts', '220 lines'), _tool('search', 'Search', 'locks.acquire', '9 in 4 files'), _tool('term', 'Test', 'pnpm test auth', '2 failed', c: red)])),
          _say('Builder', _p('The expiry check happens before the lock is held, so the second tab acts on a token the first already replaced. I am moving the read inside the critical section and adding a single-flight map keyed by account.'), c: steel),
          _say('Edit', Box(const Mat(top: Color(0xFF0F1620), bottom: Color(0xFF111924), r: 4, edge: Color(0xFF1C2735)), child: Column(children: [Padding(padding: const EdgeInsets.fromLTRB(10, 6, 10, 6), child: Row(children: [ic('edit', s: 12, c: steel), gap(7), m('src/auth/refresh.ts', 11.5, ink1), const Spacer(), m('+8', 11, green), gap(6), m('−2', 11, red)])), Container(height: 1, color: const Color(0xFF1A2431)), Padding(padding: const EdgeInsets.symmetric(vertical: 5), child: diffView())]))),
        ]))))),
        Padding(padding: const EdgeInsets.fromLTRB(10, 0, 10, 10), child: Box(plate.copy(r: 6), pad: const EdgeInsets.all(8), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Box(bay.copy(r: 4), h: 46, pad: const EdgeInsets.fromLTRB(11, 8, 11, 8), child: Align(alignment: Alignment.topLeft, child: t('Steer Builder. It reads this after the current tool call.', 13.5, 400, ink3))), gap(8),
          Row(children: [sel('Harness', 'Codex'), gap(5), sel('Model', 'GPT-5.6'), gap(5), sel('Connection', 'ChatGPT Pro'), gap(5), sel('Effort', 'High'), gap(5), sel('Access', 'Worktree only'), const Spacer(), btn('clip', h: 34, w: 34), gap(5), btn('steer', m: keyVolt, c: const Color(0xFFFFFFFF), label: 'Queue', h: 34)]),
        ]))),
      ]))),
      gap(8),
      // inspector
      Expanded(flex: 34, child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Expanded(flex: 5, child: Box(plate, pad: const EdgeInsets.all(10), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Row(children: [cap('Plan'), const Spacer(), t('5 of 7', 11, 500, ink3)]), gap(8), Expanded(child: Box(bay, pad: const EdgeInsets.symmetric(vertical: 5), child: Column(children: [for (final (d, x, w) in [(2, 'Reproduce the double rotation', 'Tester'), (2, 'Survey call sites of rotate()', 'Surveyor'), (2, 'Write the design note', 'Architect'), (2, 'Add the failing test', 'Tester'), (1, 'Single-flight guard', 'Builder'), (1, 'Migrate the session table', 'Tester'), (0, 'Independent review', 'Reviewer')]) Container(height: 27, padding: const EdgeInsets.symmetric(horizontal: 10), color: d == 1 ? const Color(0x142E7BE0) : null, child: Row(children: [lamp(d == 2 ? green : volt, on: d > 0), gap(9), Expanded(child: t(x, 12.5, 450, d == 2 ? ink3 : d == 1 ? ink1 : ink2)), t(w, 11.5, 400, ink3)]))])))]))),
        gap(8),
        Expanded(flex: 4, child: Box(plate, pad: const EdgeInsets.all(10), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Row(children: [cap('Request from Tester', amber), const Spacer(), t('waiting 0:38', 11, 400, ink3)]), gap(8), t('Run a command outside the sandbox', 13, 560, ink1), gap(6), Box(bay, pad: const EdgeInsets.symmetric(horizontal: 10, vertical: 7), child: m(r'$ pnpm db:migrate --env test', 12, ink1)), gap(8), t('Writes to  db/test.sqlite     Network  none', 12, 400, ink2), const Spacer(), Row(children: [btn('check', m: keyVolt, c: const Color(0xFFFFFFFF), label: 'Allow once', h: 28), gap(5), btn('list', label: 'Always in this worktree', h: 28), const Spacer(), btn('x', label: 'Deny', h: 28)])]))),
      ])),
    ]))),
    // deck
    SizedBox(height: 206, child: Padding(padding: const EdgeInsets.fromLTRB(8, 0, 8, 8), child: Box(const Mat(top: Color(0xFF0C1016), bottom: Color(0xFF10151C), r: 6, inset: .7, lip: .08, edge: Color(0xFF04070B)), pad: const EdgeInsets.all(6), child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      for (final (i, g) in agents.indexed) ...[Expanded(child: strip(g, sel: i == 2)), gap(6)],
      Box(keyDown.copy(r: 5), w: 40, child: Center(child: ic('plus', c: ink3))),
    ])))),
  ]));
}
