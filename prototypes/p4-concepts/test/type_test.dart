import 'dart:ui';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'shot.dart';
TextStyle st(String f, double s, int w, [double wd = 100, Color c = const Color(0xFFD5DCE6), double ls = 0]) => TextStyle(fontFamily: f, fontSize: s, height: 1.45, color: c, letterSpacing: ls, fontVariations: [FontVariation('wght', w.toDouble()), FontVariation('wdth', wd)]);
void main() => testWidgets('type', (t) async {
  Widget sans(String f, double narrow) => Padding(padding: const EdgeInsets.only(bottom: 12), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
    Text('$f  Refresh rotates the token before the lock', style: st(f, 18, 600)),
    Text('The refresh path reads `session.token` outside the lock, so two tabs can both see an expired token and rotate it twice. I moved the check inside the critical section and added a single-flight guard keyed by account. Ilf1 0O 8B rn m, 1,284 → 96 ms.', style: st(f, 14, 400)),
    Text('Tokens 18,402   Cached 91%   Elapsed 04:12   Edited src/auth/refresh.ts   Waiting for approval', style: st(f, 12, 450, 100, const Color(0xFF97A3B3))),
    Text('CONTEXT   INPUT   CACHE   REASONING   PERMISSIONS   Model   Harness   Reviewer', style: st(f, 10.5, 600, narrow, const Color(0xFF97A3B3), .3)),
  ]));
  Widget mono(String f) => Padding(padding: const EdgeInsets.only(bottom: 10), child: Text('$f  final guard = inflight.putIfAbsent(id, () => rotate(id)); // 0O1lI\n+ if (!lock.held) return {ok: false, code: 0x1F}; => != === <= 12:04:55', style: st(f, 12, 400)));
  await shot(t, ColoredBox(color: const Color(0xFF131A24), child: Padding(padding: const EdgeInsets.all(20), child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
    Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [sans('Mona', 80), sans('Archivo', 80), sans('Instrument', 80), sans('Sofia', 100), sans('Hanken', 100)])),
    const SizedBox(width: 24),
    Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [sans('SofiaSemi', 100), for (final m in ['JB', 'Martian', 'Spline', 'Sometype']) mono(m)])),
  ]))), 'type', size: const Size(1440, 800));
});
