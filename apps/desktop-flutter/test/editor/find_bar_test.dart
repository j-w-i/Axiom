// The find bar in a narrow editor group (a split, a small window; AUD-053): at 400 px, and down
// to the 240 px a split group keeps, it fits without overflowing, and the match count, Next,
// Replace and Close stay on screen with their names. The test font is wider than the UI's, so
// this errs on the narrow side.

import 'package:axiom_desktop/editor/editor_area.dart';
import 'package:axiom_desktop/main.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/text_input.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

Future<CodeFindController> pumpBar(WidgetTester tester, double width) async {
  tester.view.physicalSize = const Size(1000, 400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final editing = CodeLineEditingController.fromText('needle one\nneedle two\n');
  final find = CodeFindController(editing);
  addTearDown(() {
    find.dispose();
    editing.dispose();
  });
  await tester.pumpWidget(
    AxiomApp(
      home: Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: width,
          child: FindBar(controller: find),
        ),
      ),
    ),
  );
  find.findMode();
  await tester.pump();
  return find;
}

Finder iconButton(String label) =>
    find.byWidgetPredicate((w) => w is AxIconButton && w.label == label);

void expectInside(WidgetTester tester, Finder f, double width) {
  expect(f, findsOneWidget);
  final r = tester.getRect(f);
  expect(r.left, greaterThanOrEqualTo(0), reason: '$f at $r');
  expect(r.right, lessThanOrEqualTo(width + 0.01), reason: '$f at $r');
}

void main() {
  for (final width in [400.0, 320.0, 240.0]) {
    testWidgets('at ${width.toInt()} px: no overflow; count, Next, Replace and Close are shown', (
      tester,
    ) async {
      final semantics = tester.ensureSemantics();
      final bar = await pumpBar(tester, width);
      bar.findInputController.text = 'needle';
      await tester.pump();
      // An overflowing Row reports a layout error, which fails the test on its own.
      expect(tester.takeException(), isNull);
      expectInside(
        tester,
        find.byWidgetPredicate((w) => w is AxTextInput && w.label == 'Find'),
        width,
      );
      expectInside(tester, find.textContaining(RegExp(r'^(No results|\d+ of \d+)$')), width);
      expectInside(tester, iconButton('Next match'), width);
      expectInside(tester, iconButton('Replace…'), width);
      expectInside(tester, iconButton('Close find'), width);
      // The toggles show short names and keep their full ones for assistive technology.
      expect(find.bySemanticsLabel('Match case'), findsOneWidget);
      expect(find.bySemanticsLabel('Regular expression'), findsOneWidget);

      bar.toggleMode(); // the replace row
      await tester.pump();
      expect(tester.takeException(), isNull);
      expectInside(
        tester,
        find.byWidgetPredicate((w) => w is AxTextInput && w.label == 'Replace with'),
        width,
      );
      expectInside(tester, find.widgetWithText(AxButton, 'Replace all'), width);
      expectInside(tester, iconButton('Close find'), width);
      semantics.dispose();
    });
  }
}
