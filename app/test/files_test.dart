import 'dart:convert';
import 'dart:typed_data';

import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/models.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/shell/browser.dart';
import 'package:axiom/shell/changes.dart';
import 'package:axiom/shell/folder.dart';
import 'package:axiom/transcript/inline.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/terminal.dart';
import 'package:axiom/ui/tokens.dart';
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
      s.dirs = {
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
          JobInfo(
            's',
            'a',
            1,
            'npm run dev',
            age: 75,
            last: 'ready on :5173\n',
            processes: [
              ProcessInfo(70, 'npm', cpuMs: 100, memory: 40 << 20, ports: [5173]),
              ProcessInfo(71, 'node', parent: 1, cpuMs: 900, memory: 300 << 20, orphan: true),
            ],
          ),
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
      // What it serves is said, and opened in the agent's browser, never the person's own
      // (decision 0021).
      expect(find.text('Serves 127.0.0.1:5173'), findsOneWidget);
      expect(find.text('npm  70'), findsOneWidget);
      expect(find.text('node  71  left behind'), findsOneWidget);
      expect(find.textContaining(RegExp(r'(^| )300\.0 MiB$')), findsOneWidget);
      await snap(t, 'jobs');
      await press(t, find.text('Stop'));
      expect(s.calls.last, ['stop', 'a', 1]);
      expect(find.text('Stop'), findsNothing, reason: 'only one runs');
    });
  });

  group("an agent's page", () {
    testWidgets('is opened from what its job serves, used, pointed at and sized', (t) async {
      // A whole picture, three by two, red.
      const png =
          'iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAEUlEQVR4nGP4z8DwH4YZkDkAm34L9XKwuTwAAAAASUVORK5CYII=';
      BrowserView view(int version, {bool person = false}) => BrowserView(
        version: version,
        frame: png,
        width: 1280,
        height: 800,
        url: 'http://127.0.0.1:5173/',
        title: 'App',
        person: person,
        refused: const ['https://example.com/a.png'],
      );
      final s = Asked()
        ..running = const [
          JobInfo(
            's',
            'a',
            1,
            'npm run dev',
            age: 5,
            last: '',
            processes: [
              ProcessInfo(70, 'npm', ports: [5173]),
            ],
          ),
        ];
      await shell(t, s);
      await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')).first);
      await settled(t);
      if (!Browsers.made) {
        // No browser on Windows yet: nothing offers one.
        expect(find.text('Serves 127.0.0.1:5173'), findsOneWidget);
        expect(find.text('Open'), findsNothing);
        return;
      }
      await press(t, find.text('Open'));
      expect(s.calls.last, ['open page', 's', 'a', 5173]);
      s.showPage(view(1));
      await settled(t);
      final page = find.bySemanticsLabel('The page App');
      expect(page, findsOneWidget);
      expect(find.text('App'), findsOneWidget);
      // What the page asked for elsewhere, refused.
      await press(t, find.text('Refused 1 address'));
      expect(find.text('https://example.com/a.png'), findsOneWidget);

      // Nothing of the person's reaches the page until they take it.
      // A point of the picture, all of it: what can be seen of it may be less, and it moves.
      Offset at() {
        final r = t.getRect(
          find.descendant(of: find.byType(AgentPage), matching: find.byType(Image)),
        );
        return r.topLeft + Offset(r.width / 2, r.height / 4);
      }

      await t.tapAt(at());
      await settled(t);
      expect(s.calls.where((c) => c.first == 'page input'), isEmpty);
      await press(t, find.text('Use it'));
      expect(s.calls.last, ['hold', 'a', true]);
      expect(find.text('The agent waits while you use the page.'), findsOneWidget);
      s.showPage(view(2, person: true));
      await settled(t);
      expect(find.text('Give back'), findsOneWidget);
      await t.tapAt(at());
      await settled(t);
      // Where it was pressed, on the page's own scale.
      final down = s.calls.firstWhere((c) => c.first == 'page input')[1]! as Json;
      expect([down['kind'], down['event'], down['button']], ['mouse', 'pressed', 'left']);
      expect([(down['x']! as double).round(), (down['y']! as double).round()], [640, 200]);

      // Pointed at: what it is goes into the message, as the page's.
      await press(t, find.text('Point'));
      expect(find.text('Press what you mean on the page.'), findsOneWidget);
      await t.tapAt(at());
      await settled(t);
      expect(s.calls.last, ['point', 640, 200]);
      expect(find.text('Added to your message.'), findsOneWidget);
      final message = find.byWidgetPredicate(
        (w) => w is EditableText && w.controller.text.contains('h1#t'),
      );
      expect(message, findsOneWidget);
      final said = t.widget<EditableText>(message).controller.text;
      expect(said, contains("what it says is the page's"));
      expect(said, contains('font-size: 32px'));

      await press(t, find.text('Phone'));
      expect(s.calls.last, ['page size', 390, 844, false]);
      await press(t, find.text('Dark'));
      expect(s.calls.last, ['page size', 1280, 800, true]);
      await snap(t, 'agents-page');

      // Closed: it ends, and what the person held goes back.
      await press(t, find.bySemanticsLabel('Close the browser'));
      expect(s.calls, anyElement(equals(['close page', 'a'])));
      expect(s.calls.last, ['hold', 'a', false]);
      expect(page, findsNothing);
    });
  });

  group('folders, as their person names them', () {
    testWidgets('renamed, pinned, and new sessions there begin as one of them did', (t) async {
      final s = Asked();
      await shell(t, s);
      expect(find.text('w'), findsOneWidget);
      await press(t, find.bySemanticsLabel('Settings of w'));
      await press(t, find.text('Rename…'));
      await t.enterText(field('Name of the folder'), 'Web shop');
      await t.testTextInput.receiveAction(TextInputAction.done);
      await settled(t);
      expect(s.calls.last, ['folder', '/w', 'Web shop', false, null, null]);
      expect(find.text('Web shop'), findsOneWidget);
      await press(t, find.bySemanticsLabel('Settings of Web shop'));
      await press(t, find.text('Pin to the top'));
      expect(s.calls.last, ['folder', '/w', 'Web shop', true, null, null]);
      await press(t, find.bySemanticsLabel('Settings of Web shop'));
      expect(find.text('Unpin'), findsOneWidget);
      await press(t, find.text('Begin new sessions like this one'));
      expect(s.calls.last, ['folder', '/w', 'Web shop', true, 'claude-opus-5-5', Access.ask]);
      // Another way to begin is chosen, and a new session there begins as the folder says.
      s.folders = [
        FolderInfo(
          'f_1',
          '/w',
          name: 'Web shop',
          execution: const Execution(harness: 'axiom', model: 'gpt-5.6', connection: 'c1'),
          access: Access.read,
        ),
      ];
      await press(t, find.bySemanticsLabel('New session in Web shop'));
      expect(find.text('GPT-5.6'), findsWidgets);
      expect(find.text('Read only'), findsWidgets);
      await press(t, find.bySemanticsLabel('Settings of Web shop'));
      await press(t, find.text('Forget this folder\'s settings'));
      expect(s.calls.last, ['forget folder', 'f_1']);
      expect(find.text('Web shop'), findsNothing);
      expect(find.text('w'), findsWidgets);
    });

    testWidgets('a pinned folder stands first', (t) async {
      SessionInfo info(String id, String w, int at) => SessionInfo.decode({
        'id': id,
        'title': 'In $w',
        'workspace': w,
        'created': at,
        'updated': at,
        'last_seq': 1,
        'running': false,
      });
      final s = Asked()
        ..sessions = [info('s2', '/new', 2), info('s1', '/old', 1)]
        ..folders = const [FolderInfo('f_1', '/old', pinned: true)];
      await shell(t, s);
      double y(String text) => t.getTopLeft(find.text(text).first).dy;
      expect(y('old'), lessThan(y('new')));
    });

    testWidgets('one that another folder took the place of is not used', (t) async {
      final s = Asked()..folders = const [FolderInfo('f_1', '/w', name: 'Old', moved: true)];
      await shell(t, s);
      expect(find.text('Old'), findsNothing);
      expect(find.text('w'), findsOneWidget);
    });
  });

  group('terminals of the person\'s own', () {
    test('keys and pastes as a terminal takes them, and paths in it', () {
      expect(pasted('a\nb', bracketed: false), 'a\rb');
      expect(pasted('a\r\nb\x1b[201~c', bracketed: true), '\x1b[200~a\rbc\x1b[201~');
      expect(pathAt('error at src/a.rs:42:7.', 12), ('src/a.rs', 42));
      expect(pathAt('see (lib/x.dart) now', 6), ('lib/x.dart', null));
      expect(pathAt('nothing here', 3), isNull);
      expect(pathAt('x', 9), isNull);
      expect(
        terminalOutput('zsh', 'a\n```b\n\n'),
        'Output of the terminal "zsh":\n```\na\n\'\'\'b\n```',
      );
      const c = AxColors.dark;
      expect(terminalColor(null, c), isNull);
      expect(terminalColor('#102030', c), const Color(0xFF102030));
      expect(terminalColor('16', c), const Color(0xFF000000));
      expect(terminalColor('231', c), const Color(0xFFFFFFFF));
      expect(terminalColor('232', c), const Color(0xFF080808));
      expect(terminalColor('1', c), isNot(terminalColor('1', AxColors.light)));
      for (final no in ['256', '-1', 'x', '#12']) {
        expect(terminalColor(no, c), isNull, reason: no);
      }
    });

    testWidgets('one is opened in the panel, typed into, and a path in it opens the file', (
      t,
    ) async {
      final s = Asked()
        ..texts = {'lib/cache.dart': cache}
        ..folders = const [
          FolderInfo('f_1', '/w', commands: [FolderCommand('Dev', 'npm run dev', key: 1)]),
        ];
      await shell(t, s);
      await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')).first);
      await settled(t);
      // Not made there yet (decision 0022): nothing offers one.
      if (!Terminals.made) {
        expect(find.bySemanticsLabel('New terminal'), findsNothing);
        return;
      }
      await press(t, find.bySemanticsLabel('New terminal'));
      expect(s.calls.last, ['terminal', 's']);
      expect(find.byType(TerminalView), findsOneWidget);
      s.show(
        const Screen(
          't_1',
          version: 1,
          rows: [
            [Span('w\$ ls')],
            [Span('lib/cache.dart:300', fg: '1'), Span(' ok')],
          ],
          cursor: (0, 4),
        ),
      );
      await settled(t);
      expect(find.textContaining('lib/cache.dart:300', findRichText: true), findsOneWidget);
      await snap(t, 'terminal');
      await t.sendKeyEvent(LogicalKeyboardKey.keyL);
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.keyC);
      await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      await settled(t);
      expect(
        [
          for (final c in s.calls)
            if (c.first == 'type') c[1],
        ],
        ['l', '\r', '\x03', '\x1b[A'],
      );
      // A program that asks for it has its arrows as an application has them.
      s.show(Screen('t_1', version: 2, rows: s.shown!.rows, appCursor: true));
      await settled(t);
      await t.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      await settled(t);
      expect(s.calls.last, ['type', '\x1bOA']);
      // Lines pasted where the program does not mark them are asked about first.
      t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async => call.method == 'Clipboard.getData' ? {'text': 'make\nmake install'} : null,
      );
      addTearDown(
        () => t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
      final typed = s.calls.length;
      await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.keyV);
      await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await settled(t);
      expect(find.text('Paste 2 lines? Each runs as it arrives.'), findsOneWidget);
      expect(s.calls.length, typed, reason: 'nothing typed yet');
      await press(t, find.text('Paste'));
      expect(s.calls.last, ['type', 'make\rmake install']);
      // A path in it is pressed: the file opens at its line.
      final row = find.textContaining('lib/cache.dart:300', findRichText: true);
      await t.tapAt(t.getTopLeft(row) + const Offset(20, 6));
      await settled(t);
      await t.pump();
      expect(find.byType(FileReader), findsOneWidget);
      expect(find.textContaining('line 300 forget()', findRichText: true), findsOneWidget);
      await press(t, find.bySemanticsLabel('Close lib/cache.dart'));
      // A folder's command runs in a new terminal, by its key too.
      s.terms = const [TerminalInfo('t_1', 's', 'zsh')];
      await t.pump(const Duration(seconds: 3));
      await press(t, find.text('Dev'));
      expect(s.calls.last, ['terminal', 's', 'npm run dev']);
      await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.digit1);
      await t.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await settled(t);
      expect(
        [
          for (final c in s.calls)
            if (c.first == 'terminal') c.length,
        ],
        [2, 3, 3],
      );
      // Busy, it is closed only with a second word.
      s.closing = const Failed(1, 'busy', 'npm still runs in it');
      await press(t, find.bySemanticsLabel('Close the terminal'));
      expect(find.text('Something still runs in it.'), findsOneWidget);
      await press(t, find.text('End it'));
      expect(s.calls.last, ['close', 't_1', true]);
      expect(find.byType(TerminalView), findsNothing);
    });

    testWidgets('commands are set from the folder\'s menu', (t) async {
      final s = Asked();
      await shell(t, s);
      await press(t, find.bySemanticsLabel('Settings of w'));
      await press(t, find.text('Commands…'));
      await t.enterText(field('Name of the command'), 'Test');
      await t.enterText(field('What is typed'), 'cargo test');
      await press(t, find.text('Add'));
      expect(s.calls.last.last, '1: Test = cargo test');
      expect(find.textContaining('cargo test'), findsWidgets);
    });
  });

  group('git for a person', () {
    testWidgets('chosen files are committed with a message, and the own branch is asked about', (
      t,
    ) async {
      final s = Asked()
        ..now = const Changes(
          repository: true,
          branch: 'main',
          defaultBranch: 'main',
          hooks: true,
          upstream: 'origin/main',
          ahead: 1,
          behind: 0,
          remote: 'origin',
          remoteUrl: 'git@example.com:a/b.git',
          files: [
            Changed('a.rs', 'changed', digest: 'da'),
            Changed('b.rs', 'new', digest: 'db'),
            Changed('gone.rs', 'removed'),
          ],
          diff: '--- a.rs\n+++ a.rs\n@@ -1 +1 @@\n-x\n+y\n',
        )
        ..log = const [CommitInfo('c0ffee12345', 'Ada', 1, 'First of all')];
      await shell(t, s);
      expect(find.text('1 to push (origin/main, as of the last fetch)'), findsOneWidget);
      expect(find.textContaining('hooks. Axiom does not run them'), findsOneWidget);
      await press(t, find.bySemanticsLabel('Commit b.rs'));
      await t.enterText(field('Commit message'), 'Make it so');
      await settled(t);
      await press(t, find.text('Commit 2 files'));
      expect(find.text('main is the repository\'s own branch. Commit to it?'), findsOneWidget);
      expect(s.calls.where((c) => c.first == 'commit'), isEmpty, reason: 'asked first');
      await snap(t, 'git-commit');
      await press(t, find.text('Commit to main'));
      expect(s.calls.where((c) => c.first == 'commit').single, [
        'commit',
        ('a.rs', 'da'),
        ('gone.rs', null),
        'Make it so',
        'onto default',
      ]);
      expect(
        find.textContaining('Committed abcdef12. The repository\'s hooks did not run.'),
        findsOneWidget,
      );
      // On a new branch, nothing more is asked.
      await t.enterText(field('Commit message'), 'Again');
      await t.enterText(field('New branch'), 'topic');
      await settled(t);
      await press(t, find.text('Commit 3 files'));
      expect(s.calls.where((c) => c.first == 'commit').last, [
        'commit',
        ('a.rs', 'da'),
        ('b.rs', 'db'),
        ('gone.rs', null),
        'Again',
        'topic',
      ]);
      await press(t, find.text('Push'));
      expect(s.calls.where((c) => c.first == 'push'), hasLength(1));
      expect(find.text('main is on origin as main'), findsOneWidget);
      // Compared with a branch, and its history.
      await press(t, find.bySemanticsLabel('Compare: since the last commit'));
      await press(t, find.text('Since where topic parted'));
      expect(s.calls.where((c) => c.first == 'changes').last, ['changes', 's', 'topic', 'base']);
      await press(t, find.bySemanticsLabel('History'));
      expect(find.text('First of all'), findsOneWidget);
      await press(t, find.text('First of all'));
      expect(find.text('Body of it'), findsOneWidget);
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
