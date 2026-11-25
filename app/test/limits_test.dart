import 'dart:async';

import 'package:axiom/host/proto.dart';
import 'package:axiom/shell/settings.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

const all = [
  HostLimit(
    'A run of Axiom\'s own agents',
    'Steps, each an answer of the model',
    200,
    LimitUnit.count,
  ),
  HostLimit('A run of Axiom\'s own agents', 'Output tokens', null, LimitUnit.count),
  HostLimit(
    'A run of Axiom\'s own agents',
    'Time, without waiting for you',
    14400,
    LimitUnit.seconds,
  ),
  HostLimit('Files', 'Files in an agent\'s copy of a folder', 100000, LimitUnit.count),
  HostLimit('Files', 'An agent\'s copy of a folder', 16 << 30, LimitUnit.bytes),
];

void main() {
  Future<void> page(
    WidgetTester t,
    Future<List<HostLimit>> Function()? limits, {
    Size size = const Size(800, 900),
    AxTheme theme = const AxTheme(),
    double text = 1,
  }) async {
    final prefs = ValueNotifier(const Prefs());
    addTearDown(prefs.dispose);
    await mount(
      t,
      ColoredBox(
        color: theme.colors.canvas.fill,
        child: PrefsScope(prefs, child: SettingsPage(limits: limits)),
      ),
      size: size,
      theme: theme,
      textScale: text,
    );
  }

  Future<void> fold(WidgetTester t) async {
    await t.ensureVisible(find.text('Limits'));
    await t.pump();
    await t.tap(find.text('Limits'));
    await t.pump();
    await t.pump();
  }

  testWidgets('they are asked for when somebody looks, and said in words', (t) async {
    var asked = 0;
    final s = t.ensureSemantics();
    final answers = <Completer<List<HostLimit>>>[];
    await page(t, () {
      asked++;
      answers.add(Completer());
      return answers.last.future;
    });
    expect(asked, 0);
    expect(find.text('Files'), findsNothing);
    await fold(t);
    expect(asked, 1);
    expect(find.text('Asking the host'), findsOneWidget);
    answers.last.complete(all);
    await t.pump();
    await t.pump();
    expect(find.text('Asking the host'), findsNothing);
    expect(find.text('A run of Axiom\'s own agents'), findsOneWidget, reason: 'once for all of it');
    for (final w in ['200', 'No limit', '4 hours', '100\u{a0}000', '16 GiB']) {
      expect(find.text(w), findsOneWidget, reason: w);
    }
    // Read as one, and nothing in it can be pressed or written.
    expect(find.bySemanticsLabel(RegExp('Output tokens\nNo limit')), findsOneWidget);
    final files = t.getTopLeft(find.text('Files')).dy;
    expect(t.getTopLeft(find.text('4 hours')).dy, lessThan(files));
    expect(t.getTopLeft(find.text('16 GiB')).dy, greaterThan(files));
    await snap(t, 'settings-limits');
    await fold(t);
    expect(find.text('Files'), findsNothing);
    expect(asked, 1);
    // Another host may keep others.
    await fold(t);
    expect(asked, 2);
    answers.last.complete(const [HostLimit('Files', 'One', 1, LimitUnit.count)]);
    await t.pump();
    await t.pump();
    expect(find.text('200'), findsNothing);
    s.dispose();
  });

  testWidgets('a host that does not answer is said, and can be asked again', (t) async {
    var fail = true;
    await page(t, () async => fail ? throw const Failed(1, 'internal', 'the host is gone') : all);
    await fold(t);
    expect(find.textContaining('the host is gone'), findsOneWidget);
    expect(find.text('Files'), findsNothing);
    fail = false;
    await fold(t);
    await fold(t);
    expect(find.text('16 GiB'), findsOneWidget);
    expect(find.textContaining('is gone'), findsNothing);
  });

  testWidgets('what comes too late changes nothing', (t) async {
    final late = Completer<List<HostLimit>>();
    await page(t, () => late.future);
    await fold(t);
    await t.pumpWidget(const SizedBox());
    late.complete(all);
    await t.pump();
    expect(t.takeException(), isNull);
  });

  testWidgets('without a host there is no such part', (t) async {
    await page(t, null);
    expect(find.text('Limits'), findsNothing);
  });

  testWidgets('it fits a narrow window, large text and a host that says odd things', (t) async {
    final odd = [
      ...all,
      HostLimit('x' * 400, 'word ' * 90, 9007199254740991, LimitUnit.unknown),
      const HostLimit('a\nb\u{202e}', 'c\u{0}d\n', 1, LimitUnit.seconds),
    ];
    for (final theme in [const AxTheme(), const AxTheme(colors: AxColors.light)]) {
      await page(t, () async => odd, size: const Size(380, 260), theme: theme, text: 2);
      await fold(t);
      expect(t.takeException(), isNull);
      expect(find.textContaining('\u{202e}'), findsNothing);
      await t.pumpWidget(const SizedBox());
    }
  });
}
