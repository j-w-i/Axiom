// The grammar's colours reach an editor: re_editor's highlighter, through its shared worker
// isolates (FL-PERF-003, audit finding 7), delivers results for a document and for a second one
// of the same language (the case where a worker lacks the grammar and is asked again).

import 'package:axiom_desktop/editor/code_view.dart';
import 'package:axiom_desktop/editor/languages.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/foundation.dart' show TargetPlatform;
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

Future<void> _until(WidgetTester tester, bool Function() cond, String what) async {
  for (var i = 0; i < 100 && !cond(); i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 50)));
    await tester.pump();
  }
  expect(cond(), isTrue, reason: what);
}

void main() {
  testWidgets('highlight results reach each editor', (tester) async {
    final rust = Languages.instance.byId('rust');
    final theme = highlightTheme(rust, AxiomColors.dark.syntax)!;
    Widget editor(String text) => Directionality(
      textDirection: TextDirection.ltr,
      child: SizedBox(
        width: 600,
        height: 300,
        child: CodeEditor(
          controller: CodeLineEditingController.fromText(text),
          style: CodeEditorStyle(codeTheme: theme),
          cursorBlink: false,
        ),
      ),
    );
    final before = reEditorHighlightsDelivered;
    await tester.pumpWidget(editor('fn main() {\n    let x = 1;\n}\n'));
    await _until(
      tester,
      () => reEditorHighlightsDelivered > before,
      'the first editor highlighted',
    );
    final second = reEditorHighlightsDelivered;
    await tester.pumpWidget(editor('fn other() -> i32 {\n    2\n}\n'));
    await _until(
      tester,
      () => reEditorHighlightsDelivered > second,
      'the second editor highlighted',
    );
    await tester.pumpWidget(const SizedBox());
  }, variant: TargetPlatformVariant.only(TargetPlatform.linux));
}
