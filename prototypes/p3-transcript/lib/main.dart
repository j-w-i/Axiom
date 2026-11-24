import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';
import 'kit.dart';

// ---- incremental block model ----
class Block { Block(this.kind, this.text); final int kind; final String text; } // 0 paragraph, 1 heading, 2 bullet, 3 code
/// Closed blocks never change. Only [tail] is re-split, and only from its own start.
class Doc extends ChangeNotifier {
  final List<Block> closed = []; String tail = ''; bool fence = false; int version = 0; bool _dirty = false; int raw = 0;
  void append(String s) { tail += s; raw += s.length;
    while (true) {
      if (fence || tail.startsWith('```')) { fence = true; final e = tail.indexOf('\n```', 3); if (e < 0) break; closed.add(Block(3, tail.substring(tail.indexOf('\n') + 1, e))); final n = tail.indexOf('\n', e + 1); tail = n < 0 ? '' : tail.substring(n + 1); fence = false; }
      else { final e = tail.indexOf('\n\n'); if (e < 0) break; final x = tail.substring(0, e); if (x.trim().isNotEmpty) closed.add(mk(x)); tail = tail.substring(e + 2); }
    }
    if (!_dirty) { _dirty = true; SchedulerBinding.instance.scheduleFrameCallback((_) { _dirty = false; version++; notifyListeners(); }); } }
  static Block mk(String x) => x.startsWith('#') ? Block(1, x.replaceFirst(RegExp('^#+ '), '')) : x.startsWith('- ') ? Block(2, x) : Block(0, x);
}
/// The naive model: one growing string, parsed whole on every update.
List<Block> parseAll(String s) { final d = <Block>[]; var rest = s; while (rest.isNotEmpty) { if (rest.startsWith('```')) { final e = rest.indexOf('\n```', 3); if (e < 0) { d.add(Block(3, rest)); break; } d.add(Block(3, rest.substring(rest.indexOf('\n') + 1, e))); final n = rest.indexOf('\n', e + 1); rest = n < 0 ? '' : rest.substring(n + 1); } else { final e = rest.indexOf('\n\n'); if (e < 0) { d.add(Doc.mk(rest)); break; } d.add(Doc.mk(rest.substring(0, e))); rest = rest.substring(e + 2); } } return d; }

TextSpan inline(String s, TextStyle st) { final out = <TextSpan>[]; final re = RegExp(r'`([^`]+)`|\*\*([^*]+)\*\*'); var p = 0; for (final k in re.allMatches(s)) { if (k.start > p) out.add(TextSpan(text: s.substring(p, k.start))); out.add(k[1] != null ? TextSpan(text: k[1], style: ms(12.5, 400, const Color(0xFF9FC0E6))) : TextSpan(text: k[2], style: const TextStyle(fontVariations: [FontVariation('wght', 650)]))); p = k.end; } out.add(TextSpan(text: s.substring(p))); return TextSpan(style: st, children: out); }
final _body = ts(14, 400, const Color(0xFFD6DEE8), 100, .22, 1.6), _head = ts(16, 650, ink1);
class BlockView extends StatelessWidget { const BlockView(this.b, {super.key}); final Block b;
  @override Widget build(BuildContext context) => Padding(padding: const EdgeInsets.fromLTRB(18, 0, 18, 10), child: switch (b.kind) {
    3 => Container(decoration: BoxDecoration(color: const Color(0xFF0E1622), border: Border.all(color: const Color(0xFF182334)), borderRadius: BorderRadius.circular(4)), padding: const EdgeInsets.all(8), child: Text.rich(TextSpan(children: [for (final l in b.text.split('\n')) TextSpan(children: [code(l, 11.5), const TextSpan(text: '\n')])]))),
    1 => Text(b.text, style: _head), _ => Text.rich(inline(b.text, _body)) }); }

class Incremental extends StatelessWidget { const Incremental(this.d, {super.key}); final Doc d;
  // reverse list: index 0 is the tail, so appends never shift the viewport and only visible blocks exist
  @override Widget build(BuildContext context) => RepaintBoundary(child: ListenableBuilder(listenable: d, builder: (_, _) { final n = d.closed.length; return ListView.builder(reverse: true, padding: const EdgeInsets.only(top: 12), itemCount: n + 1, findChildIndexCallback: (k) => n - (k as ValueKey<int>).value, itemBuilder: (_, i) => i == 0 ? BlockView(Doc.mk(d.tail), key: ValueKey(n)) : _Closed(d.closed[n - i], key: ValueKey(n - i))); })); }
class _Closed extends StatefulWidget { const _Closed(this.b, {super.key}); final Block b; @override State<_Closed> createState() => _ClosedS(); }
class _ClosedS extends State<_Closed> { late final Widget w = RepaintBoundary(child: BlockView(widget.b)); @override Widget build(BuildContext c) => w; }
class Naive extends StatelessWidget { const Naive(this.s, {super.key}); final ValueNotifier<String> s;
  @override Widget build(BuildContext context) => ValueListenableBuilder(valueListenable: s, builder: (_, v, _) => ListView(reverse: true, children: [for (final b in parseAll(v).reversed) BlockView(b)])); }

const sample = '## Findings\n\nThe expiry check happens **before** the lock is held, so the second tab acts on a token the first already replaced. I am moving the read inside `refresh()` and adding a single-flight map keyed by account.\n\n- Both writers now go through one entry point\n- The store keeps its existing interface\n\n```ts\nasync refresh(account: Account) {\n  return this.inflight.run(account.id, async () => {\n    await using lock = await this.locks.acquire(account.id);\n    const session = await this.store.read(account.id);\n    if (!expired(session.token)) return session;\n  });\n}\n```\n\nNothing outside the auth module changes. The migration adds a `rotated_at` column, and the tester will run it against the test database before the review starts.\n\n';

class StripModel extends ChangeNotifier { double v = .1; String now = 'starting'; int n = 0; void tick() { n++; v = (v + .0007) % 1; now = 'refresh.ts  +${n % 90} −${n % 30}'; notifyListeners(); } }
Widget stripW(int i, StripModel s) => RepaintBoundary(child: Box(plate, pad: const EdgeInsets.all(8), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Row(children: [lamp(volt, w: 15, h: 6), gap(7), Expanded(child: t('Agent $i', 13, 650, ink1))]), gap(8), Expanded(child: Row(children: [Expanded(child: Box(bay, pad: const EdgeInsets.all(6), child: RepaintBoundary(child: ListenableBuilder(listenable: s, builder: (_, _) => Align(alignment: Alignment.bottomLeft, child: m(s.now, 10, ink1)))))), gap(6), RepaintBoundary(child: ListenableBuilder(listenable: s, builder: (_, _) => LayoutBuilder(builder: (_, b) => meter(s.v, w: 9, h: b.maxHeight, n: 20, vert: true))))])), gap(6), Row(children: [btn('pause'), gap(4), btn('stop'), gap(4), btn('steer')])])));

void main() {
  final env = Platform.environment; final mode = env['P3_MODE'] ?? 'inc'; final panes = int.parse(env['P3_PANES'] ?? '1'); final rate = int.parse(env['P3_RATE'] ?? '100'); final history = int.parse(env['P3_HISTORY'] ?? '200'); final secs = int.parse(env['P3_SECS'] ?? '8');
  WidgetsFlutterBinding.ensureInitialized();
  final docs = [for (var i = 0; i < panes; i++) Doc()], naive = [for (var i = 0; i < panes; i++) ValueNotifier('')]; final strips = [for (var i = 0; i < 10; i++) StripModel()];
  for (var p = 0; p < panes; p++) { final pre = sample * (history ~/ 6); mode == 'naive' ? naive[p].value = pre : docs[p].append(pre); }
  runApp(Directionality(textDirection: TextDirection.ltr, child: Box(chassis, pad: const EdgeInsets.fromLTRB(8, 34, 8, 8), child: Column(children: [
    Expanded(child: Row(children: [for (var p = 0; p < panes; p++) Expanded(child: Padding(padding: const EdgeInsets.all(3), child: Box(bay, child: ClipRect(child: mode == 'naive' ? Naive(naive[p]) : Incremental(docs[p])))))])), gap(8),
    SizedBox(height: 170, child: Row(children: [for (var i = 0; i < 10; i++) Expanded(child: Padding(padding: const EdgeInsets.symmetric(horizontal: 2.5), child: stripW(i + 1, strips[i])))])) ]))));
  final ft = <FrameTiming>[]; var on = false; SchedulerBinding.instance.addTimingsCallback((x) { if (on) ft.addAll(x); });
  var pos = 0; final sw = Stopwatch(); var fed = 0;
  Timer(const Duration(seconds: 3), () { on = true; sw.start();
    Timer.periodic(const Duration(milliseconds: 4), (tm) {
      final due = sw.elapsedMilliseconds * rate ~/ 1000; // tokens per agent
      while (fed < due) { fed++; final tok = [for (var k = 0; k < 4; k++) sample[(pos + k) % sample.length]].join(); pos = (pos + 4) % sample.length;
        for (var p = 0; p < panes; p++) { mode == 'naive' ? naive[p].value += tok : docs[p].append(tok); } if (fed % 4 == 0) { for (final s in strips) { s.tick(); } } }
      if (sw.elapsed.inSeconds >= secs) { tm.cancel(); on = false; Timer(const Duration(milliseconds: 300), () {
        String q(Duration Function(FrameTiming) f) { final l = [for (final x in ft) f(x).inMicroseconds / 1000]..sort(); return 'p50 ${l[l.length ~/ 2].toStringAsFixed(2)} p99 ${l[l.length * 99 ~/ 100].toStringAsFixed(2)} max ${l.last.toStringAsFixed(2)}'; }
        final v = [for (var i = 1; i < ft.length; i++) (ft[i].timestampInMicroseconds(ui.FramePhase.vsyncStart) - ft[i - 1].timestampInMicroseconds(ui.FramePhase.vsyncStart)) / 1000]..sort();
        final over = ft.where((x) => x.buildDuration.inMicroseconds > 8333 || x.rasterDuration.inMicroseconds > 8333).length;
        stdout.writeln('RESULT $mode panes=$panes rate=$rate/s history=$history frames=${ft.length} vsync p50 ${v[v.length ~/ 2].toStringAsFixed(2)} ms | build ${q((x) => x.buildDuration)} | raster ${q((x) => x.rasterDuration)} | >8.33ms: $over | rss ${(ProcessInfo.currentRss / 1048576).toStringAsFixed(0)} MB'); exit(0); }); } }); });
}
