import 'dart:io';

import 'package:axiom/app.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/models.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/shell/settings.dart';
import 'package:axiom/transcript/inline.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/fonts.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

/// Stands for a family a person has installed.
const chosen = 'Installed Here';

AgentModel talk() {
  var n = 0;
  Event e(Body b) => Event(++n, n, b);
  final m = SessionModel('s')
    ..apply([
      e(const SessionCreated('T', '/w')),
      e(const AgentAdded('a', 'Lead', Execution(harness: 'axiom'), null)),
      e(const RunStarted('r', 'a')),
      e(const ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
      e(
        const ItemEnded(
          'i',
          Outcome.completed,
          'Words and `inline()` and **strong**.\n\n```rust\nfn fenced() {}\n```',
          null,
        ),
      ),
      e(const ItemStarted('t', 'r', 'a', ItemKind.tool, 'read', {'path': 'src/read.ts'})),
      e(const ItemEnded('t', Outcome.completed, '', {'summary': '220 lines'})),
    ]);
  return m.agents.values.single;
}

/// Everything that is written, with the family it is drawn in and how wide it came out.
Map<String, (String, double)> written(WidgetTester t) {
  final out = <String, (String, double)>{};
  void walk(RenderObject o) {
    if (o is RenderParagraph) {
      final base = o.text.style?.fontFamily ?? '';
      o.text.visitChildren((s) {
        if (s is TextSpan && (s.text?.trim().isNotEmpty ?? false)) {
          final all = o.text.toPlainText();
          final at = all.indexOf(s.text!);
          final boxes = o.getBoxesForSelection(
            TextSelection(baseOffset: at, extentOffset: at + s.text!.length),
          );
          final wide = boxes.fold<double>(0, (w, b) => w + b.right - b.left);
          out[s.text!.trim()] = (s.style?.fontFamily ?? base, wide);
        }
        return true;
      });
    }
    o.visitChildren(walk);
  }

  walk(t.renderObject(find.byType(AxiomApp)));
  return out;
}

Widget everything() => Column(
  children: [
    SizedBox(height: 340, child: TranscriptView(talk())),
    const AxField(label: 'Path', mono: true, placeholder: 'a/path'),
    Expanded(
      child: DiffLines([
        DiffLine(DiffKind.remove, 1, 'gone()'),
        DiffLine(DiffKind.add, 1, 'came()'),
      ]),
    ),
  ],
);

void main() {
  late ValueNotifier<Prefs> prefs;
  setUp(() => prefs = ValueNotifier(const Prefs(look: Look.dark)));
  tearDown(() => prefs.dispose());

  Future<void> open(WidgetTester t, Widget home) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(1000, 800);
    addTearDown(t.view.reset);
    t.platformDispatcher.accessibilityFeaturesTestValue = const FakeAccessibilityFeatures(
      disableAnimations: true,
    );
    addTearDown(t.platformDispatcher.clearAccessibilityFeaturesTestValue);
    await t.runAsync(loadFonts);
    await t.runAsync(() async {
      final l = FontLoader(chosen)
        ..addFont(Future.value(ByteData.sublistView(File('fonts/Inter.ttf').readAsBytesSync())));
      await l.load();
    });
    await t.pumpWidget(
      RepaintBoundary(
        key: stage,
        child: AxiomApp(prefs: prefs, home: home),
      ),
    );
    await t.pump(const Duration(milliseconds: 400));
  }

  testWidgets('it reaches all that is code, and nothing else', (t) async {
    await open(t, everything());
    final before = written(t);
    final code = {
      for (final MapEntry(:key, :value) in before.entries)
        if (value.$1 == AxType.literals) key,
    };
    // Inline, fenced, a diff, a path, and what stands in a field for one.
    for (final w in ['inline()', 'fn', 'fenced', 'gone', 'came', 'src/read.ts', 'a/path']) {
      expect(code.any((x) => x.contains(w)), isTrue, reason: '$w in $code');
    }
    prefs.value = prefs.value.copyWith(code: chosen);
    await t.pump();
    final after = written(t);
    expect(after.keys, before.keys);
    for (final MapEntry(:key, :value) in after.entries) {
      if (code.contains(key)) {
        expect(value.$1, chosen, reason: key);
        // As wide as the letters of the family that was named.
      } else {
        expect(value, before[key], reason: key);
      }
    }
    final wide = [for (final k in code) (k, before[k]!.$2, after[k]!.$2)];
    expect(wide.where((w) => w.$2 != w.$3), isNotEmpty, reason: 'it is drawn with it');
    prefs.value = prefs.value.copyWith(code: '');
    await t.pump();
    expect(written(t), before);
  });

  testWidgets('one that is not installed is drawn as the shipped one', (t) async {
    await open(t, everything());
    final before = written(t);
    final shown = t.getRect(find.byType(DiffLines));
    prefs.value = prefs.value.copyWith(code: 'No Such Family');
    await t.pump();
    final after = written(t);
    for (final k in before.keys) {
      expect(after[k]!.$2, before[k]!.$2, reason: k);
    }
    expect(t.getRect(find.byType(DiffLines)), shown);
  });

  testWidgets('code among words keeps its tile, also in the family of the words', (t) async {
    await open(t, everything());
    int tiles() => t
        .renderObjectList<RenderCodeTiles>(find.byType(CodeTiles))
        .fold(0, (n, r) => n + r.ranges.length);
    expect(tiles(), 1);
    for (final f in [chosen, AxType.words, 'No Such Family']) {
      prefs.value = prefs.value.copyWith(code: f);
      await t.pump();
      expect(tiles(), 1, reason: f);
    }
  });

  testWidgets('it is seen whether there is such a family', (t) async {
    await open(t, const SizedBox());
    for (final (name, there) in [
      (chosen, true),
      (AxType.words, true),
      (AxType.literals, true),
      ('No Such Family', false),
      ('', false),
    ]) {
      expect(hasFamily(name), there, reason: name);
    }
  });

  testWidgets('it is named on the page, and what cannot be had is said', (t) async {
    await open(t, const SettingsPage());
    final field = find.byType(EditableText);
    expect(find.text('JetBrains Mono'), findsOneWidget, reason: 'what there is without a word');
    Future<void> name(String s) async {
      await t.enterText(field, s);
      await t.pump();
      await t.testTextInput.receiveAction(TextInputAction.done);
      await t.pump();
    }

    await t.enterText(field, 'Inst');
    await t.pump();
    expect(prefs.value.code, '', reason: 'not letter by letter');
    await name('  $chosen ');
    expect(prefs.value.code, chosen);
    expect(find.textContaining('not installed'), findsNothing);
    await name('No Such Family');
    expect(find.text('No Such Family is not installed. JetBrains Mono is used.'), findsOneWidget);
    expect(t.takeException(), isNull);
    await snap(t, 'settings-code-font');
    await name('x' * 101);
    expect(prefs.value.code, '');
    // Left from the middle of it.
    await t.enterText(field, chosen);
    FocusManager.instance.primaryFocus!.unfocus();
    await t.pump();
    expect(prefs.value.code, chosen);
    await t.enterText(field, '');
    await t.pumpWidget(AxiomApp(prefs: prefs, home: const SizedBox()));
    await t.pump();
    expect(prefs.value.code, '');
  });

  test('a family\'s name is one line and not endless', () {
    for (final (s, want) in [
      (' Menlo ', 'Menlo'),
      ('', ''),
      ('a\nb', ''),
      ('a\u0000', ''),
      ('x' * 100, 'x' * 100),
      ('x' * 101, ''),
    ]) {
      expect(Prefs.family(s), want);
      expect(Prefs.decode({'code': s}).code, want);
    }
    expect(Prefs.decode({'code': 3}).code, '');
    expect(Prefs.decode(const Prefs(code: 'Menlo').encode()), const Prefs(code: 'Menlo'));
    expect(const Prefs(code: 'a') == const Prefs(), isFalse);
    expect(const AxTheme(code: 'a') == const AxTheme(), isFalse);
  });
}
