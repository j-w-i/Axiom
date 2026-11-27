import 'dart:convert';
import 'dart:typed_data';

import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/models.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/shell/changes.dart';
import 'package:axiom/shell/folder.dart';
import 'package:axiom/transcript/inline.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'agents_test.dart' show Asked, key, settled, working, x;
import 'shot.dart';

/// What M4 gives a person to look at in a folder: changes as they like them, files, jobs.
void main() {
  Future<void> press(WidgetTester t, Finder f) async {
    await t.ensureVisible(f);
    await t.tap(f);
    await settled(t);
  }

  Future<(Asked, ValueNotifier<Prefs>)> shell(
    WidgetTester t,
    Asked s, {
    Size size = const Size(1600, 900),
  }) async {
    s
      ..current ??= working()
      ..connections = const [key];
    addTearDown(s.dispose);
    final prefs = ValueNotifier(const Prefs());
    addTearDown(prefs.dispose);
    await mount(t, PrefsScope(prefs, child: Shell(s)), size: size, dpr: 1);
    await settled(t);
    return (s, prefs);
  }

  group('a diff, as its person likes to see it', () {
    const two =
        '--- a\n+++ a\n@@ -10,3 +10,3 @@\n same\n-  old  x\n+old x\n tail\n'
        '@@ -40,2 +40,3 @@\n-gone\n+came\n+more\n end\n';
    test('parts are marked where they begin, and the same lines know both numbers', () {
      final f = readDiffs(two).single;
      expect(
        [for (final l in f.lines) (l.kind, l.number, l.was, l.start)],
        [
          (DiffKind.same, 10, 10, false),
          (DiffKind.remove, 11, 0, false),
          (DiffKind.add, 11, 0, false),
          (DiffKind.same, 12, 12, false),
          (DiffKind.remove, 40, 0, true),
          (DiffKind.add, 40, 0, false),
          (DiffKind.add, 41, 0, false),
          (DiffKind.same, 42, 41, false),
        ],
      );
    });

    test('white space that is ignored turns a pair into a line that stayed', () {
      final f = readDiffs(two).single;
      final bare = withoutSpace(f.lines);
      expect(
        [for (final l in bare) (l.kind, l.number, l.was, l.text)],
        [
          (DiffKind.same, 10, 10, 'same'),
          (DiffKind.same, 11, 11, 'old x'),
          (DiffKind.same, 12, 12, 'tail'),
          (DiffKind.remove, 40, 0, 'gone'),
          (DiffKind.add, 40, 0, 'came'),
          (DiffKind.add, 41, 0, 'more'),
          (DiffKind.same, 42, 41, 'end'),
        ],
      );
      // What differs in more than white space stays a change; the order of what is left is kept.
      const l = [
        DiffLine(DiffKind.remove, 1, 'a'),
        DiffLine(DiffKind.remove, 2, 'b b'),
        DiffLine(DiffKind.add, 1, 'z'),
        DiffLine(DiffKind.add, 2, 'bb'),
      ];
      expect(
        [for (final x in withoutSpace(l)) (x.kind, x.text)],
        [(DiffKind.remove, 'a'), (DiffKind.add, 'z'), (DiffKind.same, 'bb')],
      );
    });

    test('two sides put what was beside what came for it', () {
      final rows = sideBySide(readDiffs(two).single.lines);
      expect(
        [for (final (l, r) in rows) (l?.text, r?.text)],
        [
          ('same', 'same'),
          ('  old  x', 'old x'),
          ('tail', 'tail'),
          ('gone', 'came'),
          (null, 'more'),
          ('end', 'end'),
        ],
      );
      // Lines of two parts are not put beside each other.
      const l = [DiffLine(DiffKind.remove, 3, 'x'), DiffLine(DiffKind.add, 90, 'y', start: true)];
      expect([for (final (a, b) in sideBySide(l)) (a?.text, b?.text)], [('x', null), (null, 'y')]);
    });

    testWidgets('side by side, white space and wrapping are chosen once, for every file', (
      t,
    ) async {
      final (s, prefs) = await shell(
        t,
        Asked()
          ..now = const Changes(
            repository: true,
            files: [Changed('a', 'changed'), Changed('b', 'changed')],
            diff: '$two--- b\n+++ b\n@@ -1 +1 @@\n-x\n+y\n',
          ),
      );
      await press(t, find.bySemanticsLabel(RegExp('^a, changed')));
      await press(t, find.bySemanticsLabel(RegExp('^b, changed')));
      expect(find.text('gone'), findsOneWidget);
      // Unified: each line once, old and new one under the other.
      double y(String text) => t.getTopLeft(find.text(text)).dy;
      expect(y('came'), greaterThan(y('gone')));
      await press(t, find.bySemanticsLabel('How changes are shown'));
      await press(t, find.text('Side by side'));
      expect(prefs.value.diffSide, isTrue);
      await snap(t, 'changes-side');
      expect(y('came'), y('gone'), reason: 'beside each other');
      expect(find.text('same'), findsNWidgets(2), reason: 'what stayed is on both sides');
      await press(t, find.bySemanticsLabel('How changes are shown'));
      await press(t, find.text('Ignore white space'));
      expect(prefs.value.diffSpace, isFalse);
      expect(find.text('  old  x'), findsNothing);
      expect(find.text('old x'), findsNWidgets(2));
      await press(t, find.bySemanticsLabel('How changes are shown'));
      await press(t, find.text('Close all files'));
      expect(find.text('gone'), findsNothing);
      expect(find.byType(ChangesView), findsOneWidget);
      // Narrow, there is no room for two sides, and it is one again.
      prefs.value = prefs.value.copyWith(diffSpace: true);
      await t.binding.setSurfaceSize(const Size(900, 860));
      await settled(t);
      await press(t, find.bySemanticsLabel(RegExp('^a, changed')));
      expect(y('came'), greaterThan(y('gone')));
      expect(s.calls.first, ['changes', 's']);
    });

    test('what is kept of it survives being read again, and nothing else is made up', () {
      const p = Prefs(diffSide: true, diffSpace: false, diffWrap: true);
      expect(Prefs.decode(p.encode()), p);
      expect(Prefs.decode({'diffSide': 'yes', 'diffSpace': 0}), const Prefs());
    });
  });

  /// A session whose agent has answered so.
  SessionModel answered(String text) {
    var n = 0;
    Event e(Body b) => Event(++n, n, b);
    return SessionModel('s')..apply([
      e(const SessionCreated('Fix the cache', '/w')),
      e(const AgentAdded('a', 'Lead', x, null, access: Access.ask)),
      e(const RunStarted('r', 'a')),
      e(const ItemStarted('p', 'r', 'a', ItemKind.prompt, '', null)),
      e(const ItemEnded('p', Outcome.completed, 'Where is it?', null)),
      e(const ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
      e(ItemEnded('i', Outcome.completed, text, null)),
      e(const RunEnded('r', Outcome.completed, null)),
      e(const AgentStateChanged('a', AgentPhase.done)),
    ]);
  }

  Finder field(String label) => find.descendant(
    of: find.byWidgetPredicate((w) => w is AxField && w.label == label),
    matching: find.byType(EditableText),
  );

  Future<void> ctrl(WidgetTester t, LogicalKeyboardKey k, {bool shift = false}) async {
    if (shift) await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(k);
    await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    if (shift) await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await settled(t);
  }

  final cache = FileText(
    'lib/cache.dart',
    size: 99,
    digest: 'd',
    text: [for (var i = 1; i <= 400; i++) 'line $i${i == 300 ? ' forget()' : ''}'].join('\n'),
  );

  group('a file, read', () {
    test('a path as agents write it, with a line or without; words with dots are not', () {
      expect(pathIn('src/a.rs:42'), ('src/a.rs', 42));
      expect(pathIn('./lib/x.dart#L7'), ('lib/x.dart', 7));
      expect(pathIn('lib/x.dart:7:3'), ('lib/x.dart', 7));
      expect(pathIn('README.md'), ('README.md', null));
      expect(pathIn('main.c'), ('main.c', null));
      expect(pathIn('@scope/pkg/index.ts'), ('@scope/pkg/index.ts', null));
      for (final no in ['e.g', 'src/', 'a b.rs', '../x.rs', 'https://x.org/a.js', 'v1.2', 'x:1']) {
        expect(pathIn(no), isNull, reason: no);
      }
    });

    testWidgets('a path in an answer opens the file at its line, beside the conversation', (
      t,
    ) async {
      final s = Asked()
        ..current = answered('It is in\n\n`lib/cache.dart:300`\n\nand `e.g` is no file.')
        ..texts = {'lib/cache.dart': cache};
      await shell(t, s);
      await t.tap(find.textContaining('lib/cache.dart:300', findRichText: true));
      await settled(t);
      await t.pump();
      expect(s.calls, contains(equals(['read', 'lib/cache.dart'])));
      expect(find.byType(FileReader), findsOneWidget);
      expect(
        find.textContaining('line 300 forget()', findRichText: true),
        findsOneWidget,
        reason: 'at its line',
      );
      expect(find.textContaining('line 1\n', findRichText: true), findsNothing);
      await snap(t, 'file-read');
      // Found words are counted and stepped through.
      await t.enterText(field('Find in the file'), 'forget');
      await settled(t);
      expect(find.text('1 of 1'), findsOneWidget);
      await t.enterText(field('Find in the file'), 'line 39');
      await settled(t);
      expect(find.text('1 of 11'), findsOneWidget);
      await t.enterText(field('Go to line'), '12');
      await t.testTextInput.receiveAction(TextInputAction.done);
      await settled(t);
      await t.pump();
      expect(find.textContaining('line 12', findRichText: true), findsWidgets);
      // Handed over, never edited here.
      await press(t, find.bySemanticsLabel('Open lib/cache.dart in your editor'));
      expect(s.calls.last, ['edit', 'lib/cache.dart', 12]);
      await press(t, find.bySemanticsLabel(RegExp('^Show lib/cache.dart in')));
      expect(s.calls.last, ['show', 'lib/cache.dart']);
      await press(t, find.bySemanticsLabel('Mention lib/cache.dart in the message'));
      expect(find.textContaining('@lib/cache.dart'), findsWidgets);
      expect(find.byType(EditableText), findsNWidgets(3), reason: 'two fields and the message');
      await press(t, find.bySemanticsLabel('Close lib/cache.dart'));
      expect(find.byType(FileReader), findsNothing);
    });

    testWidgets('one that cannot be read says why, and a picture is a picture', (t) async {
      // A whole picture, three by two, red.
      const png =
          'iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAEUlEQVR4nGP4z8DwH4YZkDkAm34L9XKwuTwAAAAASUVORK5CYII=';
      final s = Asked()
        ..current = answered('`shot.png`\n\n`gone.rs`')
        ..texts = {
          'shot.png': FileText('shot.png', size: 60, kind: 'image', image: 'png', bytes: png),
        };
      await shell(t, s);
      await t.tap(find.textContaining('gone.rs', findRichText: true));
      await settled(t);
      expect(find.textContaining('no such file'), findsOneWidget);
      await t.tap(find.textContaining('shot.png', findRichText: true));
      for (var i = 0; i < 20 && find.bySemanticsLabel('Picture shot.png').evaluate().isEmpty; i++) {
        await t.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 50)));
        await t.pump();
      }
      expect(find.bySemanticsLabel('Picture shot.png'), findsOneWidget);
    });

    test('a picture is measured before it is decoded, and a huge one is not', () async {
      expect(pictureSize(_png(3, 2)), (3, 2));
      expect(pictureSize(Uint8List.fromList(utf8.encode('<svg/>'))), isNull);
      final (ok, _) = await picture(base64Encode(_png(3, 2)));
      expect(ok, isNotNull);
      final (big, why) = await picture(base64Encode(_png(20000, 20000)));
      expect((big, why), (null, 'The picture is too large to show (20000 × 20000).'));
      expect((await picture('%%%')).$2, 'The picture could not be read.');
    });

    test('a table from CSV keeps quoted commas in their cell', () {
      expect(
        csvTable('a,b\n"x, y","say ""hi"""\r\n1,2'),
        '| a | b |\n|---|---|\n| x, y | say "hi" |\n| 1 | 2 |\n',
      );
      expect(csvTable('a|b\n', rows: 1), '| a\\|b |\n|---|\n');
    });
  });

  group('files found', () {
    testWidgets('by name with its key, and by what is in them', (t) async {
      final s = Asked()
        ..fits = const ['lib/cache.dart', 'test/cache_test.dart']
        ..texts = {'lib/cache.dart': cache}
        ..found = const FoundLines([
          Hit('lib/cache.dart', 300, 'line 300 forget()'),
          Hit('test/cache_test.dart', 4, 'forget'),
        ], files: 2);
      await shell(t, s);
      await ctrl(t, LogicalKeyboardKey.keyP);
      expect(find.text('Go to file'), findsOneWidget);
      await t.enterText(field('File name'), 'cache');
      await t.pump(const Duration(milliseconds: 200));
      await settled(t);
      expect(s.calls, contains(equals(['find', 'cache'])));
      expect(find.text('test/cache_test.dart'), findsOneWidget);
      await t.testTextInput.receiveAction(TextInputAction.done);
      await settled(t);
      expect(s.calls.last, ['read', 'lib/cache.dart']);
      await ctrl(t, LogicalKeyboardKey.keyF, shift: true);
      expect(find.byType(FileSearch), findsOneWidget);
      await t.enterText(field('Search in files'), 'forget');
      await t.testTextInput.receiveAction(TextInputAction.done);
      await settled(t);
      expect(s.calls.last, ['search', 'forget']);
      expect(find.text('2 lines in 2 files.'), findsOneWidget);
      await snap(t, 'file-search');
      await press(t, find.text('Whole words'));
      expect(s.calls.last, ['search', 'forget', 'word']);
      await press(t, find.bySemanticsLabel('lib/cache.dart, line 300'));
      await t.pump();
      expect(find.textContaining('line 300 forget()', findRichText: true), findsOneWidget);
      // The folder's own files, folder by folder.
      await press(t, find.bySemanticsLabel('Close lib/cache.dart'));
      s.folders = {
        '': const FileList('', [FileEntry('lib', folder: true), FileEntry('README.md')]),
        'lib': const FileList('lib', [FileEntry('cache.dart')]),
      };
      expect(find.byType(FileSearch), findsOneWidget, reason: 'back to what was found');
      await press(t, find.bySemanticsLabel('Close the search'));
      await press(t, find.bySemanticsLabel('Files'));
      await press(t, find.bySemanticsLabel('lib, folder'));
      expect(find.text('cache.dart'), findsOneWidget);
      await press(t, find.text('cache.dart'));
      expect(s.calls.last, ['read', 'lib/cache.dart']);
    });
  });

  group('jobs agents left running', () {
    testWidgets('are listed with what they said last, and stopped', (t) async {
      final s = Asked()
        ..running = const [
          JobInfo('s', 'a', 1, 'npm run dev', age: 75, last: 'ready on :5173\n'),
          JobInfo('s', 'a', 2, 'cargo build', age: 9, ended: 'ended: exit 0', last: 'done'),
          JobInfo('other', 'b', 1, 'elsewhere', last: ''),
        ];
      await shell(t, s);
      await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')).first);
      await settled(t);
      expect(find.text('1 running job'), findsOneWidget);
      expect(find.text('npm run dev'), findsOneWidget);
      expect(find.text('Lead · running for 1 min'), findsOneWidget);
      expect(find.text('elsewhere'), findsNothing, reason: 'another session');
      await press(t, find.bySemanticsLabel(RegExp('^npm run dev')));
      expect(find.text('ready on :5173'), findsOneWidget);
      await snap(t, 'jobs');
      await press(t, find.text('Stop'));
      expect(s.calls.last, ['stop', 'a', 1]);
      expect(find.text('Stop'), findsNothing, reason: 'only one runs');
    });
  });
}

/// A picture of that size, as its first bytes say: enough to be measured.
Uint8List _png(int w, int h) {
  final b = BytesBuilder()
    ..add([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13])
    ..add('IHDR'.codeUnits)
    ..add([
      w >> 24,
      w >> 16 & 255,
      w >> 8 & 255,
      w & 255,
      h >> 24,
      h >> 16 & 255,
      h >> 8 & 255,
      h & 255,
    ])
    ..add([8, 6, 0, 0, 0, 0, 0, 0, 0]);
  return b.toBytes();
}
