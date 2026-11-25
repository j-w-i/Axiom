import 'dart:io';

import 'package:axiom/app.dart';
import 'package:axiom/patterns/composer.dart';
import 'package:axiom/shell/prefs.dart';
import 'package:axiom/shell/settings.dart';
import 'package:axiom/ui/icons.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

void main() {
  late Directory dir;
  setUp(() => dir = Directory.systemTemp.createTempSync('axiom-prefs'));
  tearDown(() => dir.deleteSync(recursive: true));

  test('what was chosen is there the next time', () async {
    final f = PrefsFile('${dir.path}/a/window.json');
    expect(f.read(), const Prefs());
    const p = Prefs(
      look: Look.light,
      text: 1.3,
      zoom: 1.75,
      density: AxDensity.compact,
      enterSends: false,
    );
    await f.write(p);
    expect(f.read(), p);
    expect(Directory('${dir.path}/a').listSync(), hasLength(1), reason: 'nothing is left beside');
    // The last one counts, however quickly they come.
    for (final x in Prefs.sizes) {
      f.write(p.copyWith(text: x)).ignore();
    }
    await f.write(p.copyWith(text: 1.75));
    expect(f.read().text, 1.75);
  });

  test('what cannot be read is as it comes, and does not keep the window shut', () {
    final f = PrefsFile('${dir.path}/window.json');
    for (final (text, want) in [
      ('', const Prefs()),
      ('{', const Prefs()),
      ('[1]', const Prefs()),
      ('{"look":"dusk","text":9e9,"zoom":1.3,"density":3,"enterSends":"no"}', const Prefs()),
      ('{"look":"dark","new":true}', const Prefs(look: Look.dark)),
      ('{"zoom":0}', const Prefs()),
      ('{"zoom":-1}', const Prefs()),
      ('{"zoom":"2"}', const Prefs()),
      ('{"zoom":2}', const Prefs(zoom: 2)),
      ('{"text":1,"look":"light"}', const Prefs(look: Look.light)),
      ('"${'x' * 70000}"', const Prefs()),
    ]) {
      File(f.path).writeAsStringSync(text);
      expect(f.read(), want, reason: text.length > 99 ? 'too long' : text);
    }
  });

  testWidgets('the window is as was chosen: colours, text and what stands beside it', (t) async {
    final prefs = ValueNotifier(const Prefs());
    addTearDown(prefs.dispose);
    late AxTheme theme;
    late double text;
    await t.pumpWidget(
      AxiomApp(
        prefs: prefs,
        home: Builder(
          builder: (context) {
            theme = Ax.of(context);
            text = MediaQuery.textScalerOf(context).scale(10);
            return const Center(child: AxGlyph(AxIcon.search));
          },
        ),
      ),
    );
    final one = t.getSize(find.byType(AxGlyph)).width;
    t.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    addTearDown(t.platformDispatcher.clearPlatformBrightnessTestValue);
    prefs.value = const Prefs(look: Look.light, text: 1.5, density: AxDensity.roomy);
    await t.pump();
    expect((theme.colors, theme.density, text), (AxColors.light, AxDensity.roomy, 15));
    expect(t.getSize(find.byType(AxGlyph)).width, one * 1.5);
    prefs.value = const Prefs();
    await t.pump();
    expect((theme.colors, text), (AxColors.dark, 10));
  });

  testWidgets('it is chosen on a page', (t) async {
    final prefs = ValueNotifier(const Prefs());
    addTearDown(prefs.dispose);
    await mount(
      t,
      PrefsScope(prefs, child: const SettingsPage()),
      size: const Size(800, 600),
      dpr: 2,
    );
    await snap(t, 'settings');
    await t.tap(find.text('System'));
    await t.pumpAndSettle();
    await t.tap(find.descendant(of: find.byType(AxMenu<Look>), matching: find.text('Dark')));
    await t.pumpAndSettle();
    expect(prefs.value, const Prefs(look: Look.dark));
    await t.tap(find.bySemanticsLabel(RegExp('Send with Enter')).last);
    await t.pumpAndSettle();
    expect(prefs.value.enterSends, isFalse);
    expect(find.textContaining('starts a new line. '), findsOneWidget);
  });

  testWidgets('which key sends', (t) async {
    final prefs = ValueNotifier(const Prefs(enterSends: false));
    addTearDown(prefs.dispose);
    final sent = <String>[];
    await mount(
      t,
      PrefsScope(
        prefs,
        child: Composer(
          placeholder: 'Say',
          action: 'Send',
          onSubmit: (s) {
            sent.add(s);
            return true;
          },
        ),
      ),
    );
    await t.enterText(find.byType(EditableText), 'one');
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(sent, isEmpty);
    await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    expect(sent, ['one']);
    prefs.value = const Prefs();
    await t.pump();
    await t.enterText(find.byType(EditableText), 'two');
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(sent, ['one', 'two']);
  });

  test('sizes end where they end', () {
    var p = const Prefs();
    for (var i = 0; i < 20; i++) {
      p = p.sized(1);
    }
    expect((p.text, p.zoom), (2, 1));
    expect([for (final z in Prefs.zooms) Prefs(zoom: z).zoomed(0).zoom], Prefs.zooms);
    for (var i = 0; i < 20; i++) {
      p = p.sized(-1);
    }
    expect(p.text, .85);
  });
}
