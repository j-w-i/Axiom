import 'package:axiom/ui/icons.dart';
import 'package:axiom/ui/marks.dart';
import 'package:axiom/ui/svg_path.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('every mark and icon is drawn inside its grid', () {
    for (final m in ServiceMark.values) {
      if (m == ServiceMark.axiom || m == ServiceMark.local) continue;
      final b = m.path!.getBounds();
      expect(b.isEmpty, isFalse, reason: m.name);
      expect(b.left, greaterThan(-.2), reason: m.name);
      expect(b.top, greaterThan(-.2), reason: m.name);
      expect(b.right, lessThan(24.3), reason: m.name);
      expect(b.bottom, lessThan(24.3), reason: m.name);
      expect(b.longestSide, greaterThan(14), reason: m.name);
    }
    for (final i in AxIcon.values) {
      final b = i.path.getBounds();
      expect(b.left >= 0 && b.right <= 16 && b.top >= 0 && b.bottom <= 16, isTrue, reason: i.name);
    }
  });

  test('path data: relative and absolute, repeated, packed flags, smooth curves', () {
    final a = svgPath('M2 2h4v4H2z').getBounds();
    expect([a.left, a.top, a.right, a.bottom], [2, 2, 6, 6]);
    expect(svgPath('m1 1 2 0 0 2').getBounds().right, 3);
    expect(svgPath('M0 0a2 2 0 014 0').getBounds().width, closeTo(4, .01));
    expect(svgPath('M0 0c1 0 1 1 2 1s1 1 2 1').getBounds().right, closeTo(4, .01));
    expect(svgPath('M0 0q1 1 2 0t2 0').getBounds().right, closeTo(4, .01));
    expect(svgPath('M1.5.5L-1-.5').getBounds().left, -1);
    expect(() => svgPath('M0 0X1'), throwsFormatException);
  });

  test('a service is named only when the name says so', () {
    for (final (n, m) in [
      ('Opus 5.5', ServiceMark.claude),
      ('claude-sonnet-5-5', ServiceMark.claude),
      ('Claude Code', ServiceMark.claudecode),
      ('GPT-5.6', ServiceMark.openai),
      ('Gemini 3.5 Pro', ServiceMark.gemini),
      ('Qwen3 4B', ServiceMark.qwen),
      ('OpenCode', ServiceMark.opencode),
      ('my-model', null),
      ('', null),
      (null, null),
    ]) {
      expect(ServiceMark.of(n), m, reason: n);
    }
  });
}
