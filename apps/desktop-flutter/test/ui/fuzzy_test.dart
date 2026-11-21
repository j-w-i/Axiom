// The fuzzy ranking (palette, Go to File): the Tauri shell's tests (fuzzy.test.ts), ported.

import 'package:axiom_desktop/ui/fuzzy.dart';
import 'package:flutter_test/flutter_test.dart';

const paths = [
  'apps/desktop-tauri/src/components/EditorArea.tsx',
  'apps/desktop-tauri/src/components/Explorer.tsx',
  'apps/desktop-tauri/src/editor/surface.ts',
  'crates/axiomd/src/main.rs',
  'crates/lsp/src/lib.rs',
  'crates/term/src/lib.rs',
  'docs/roadmap/milestones.md',
  'README.md',
  'src/main.rs',
  'tests/helpers/fake-lsp/src/main.rs',
  'beads.txt',
];

List<String> top(String q, [int n = 1]) => [
  for (final r in rankFuzzy(paths, q, (p) => p).take(n)) r.item,
];

void main() {
  test('a subsequence matches; anything else does not', () {
    expect(fuzzyMatch('mnrs', 'src/main.rs'), isNotNull);
    expect(fuzzyMatch('zzz', 'src/main.rs'), isNull);
  });

  test('the file name and segment starts win', () {
    expect(top('mainrs'), ['src/main.rs']);
    expect(top('lsplib'), ['crates/lsp/src/lib.rs']);
    expect(top('termlib'), ['crates/term/src/lib.rs']);
    expect(top('readme'), ['README.md']);
    expect(top('miles'), ['docs/roadmap/milestones.md']);
  });

  test('camelCase humps', () {
    expect(top('EA'), ['apps/desktop-tauri/src/components/EditorArea.tsx']);
    expect(top('edar'), ['apps/desktop-tauri/src/components/EditorArea.tsx']);
  });

  test('highlight ranges follow the chosen alignment', () {
    expect(fuzzyMatch('main', 'src/main.rs')!.ranges, [(4, 8)]);
    expect(fuzzyMatch('EA', 'components/EditorArea.tsx')!.ranges, [(11, 12), (17, 18)]);
  });

  test('20 000 paths rank within a keystroke budget', () {
    final many = [for (var i = 0; i < 20000; i++) 'crates/c${i % 50}/src/module_$i/file_$i.rs'];
    final t = Stopwatch()..start();
    final r = rankFuzzy(many, 'mod12fil', (p) => p);
    expect(r, isNotEmpty);
    expect(t.elapsedMilliseconds, lessThan(500));
  });
}
