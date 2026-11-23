// The shell's own questions about a save, as the shell shows them (shell.dart builds its dialogs
// with these): Enter on "changed on disk" or "deleted on disk" must not overwrite or close
// (AUD-018); Enter on "unsaved changes" saves, never discards.

import 'package:axiom_desktop/shell/shell.dart';
import 'package:axiom_desktop/state/documents.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/overlays.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../ui/overlays_test.dart' show pumpAt;

/// Shows [question] the way the shell does and presses Enter: the answer, and the buttons.
Future<(String?, List<AxButton>)> enterOn(
  WidgetTester tester,
  String label,
  AxQuestion Function() question,
) async {
  String? chosen;
  var answered = false;
  await pumpAt(
    tester,
    Builder(
      builder: (context) => Center(
        child: AxButton(
          label: 'Ask',
          onPressed: () async {
            chosen = await showAxDialog<String>(
              context: context,
              label: label,
              builder: (_) => question(),
            );
            answered = true;
          },
        ),
      ),
    ),
    const Size(900, 600),
  );
  await tester.tap(find.text('Ask'));
  await tester.pumpAndSettle();
  final buttons = tester
      .widgetList<AxButton>(
        find.descendant(of: find.byType(AxQuestion), matching: find.byType(AxButton)),
      )
      .toList();
  await tester.sendKeyEvent(LogicalKeyboardKey.enter);
  await tester.pumpAndSettle();
  expect(answered, isTrue, reason: 'Enter did not answer the question');
  return (chosen, buttons);
}

void main() {
  testWidgets('"changed on disk": Cancel is the default, Enter cancels', (tester) async {
    final (chosen, buttons) = await enterOn(
      tester,
      'Changed on disk',
      () => conflictQuestion('x.txt', deleted: false),
    );
    expect(buttons.map((b) => b.label), ['Overwrite', 'Reload', 'Cancel']);
    expect(buttons.firstWhere((b) => b.label == 'Cancel').kind, ButtonKind.primary);
    expect(chosen, 'cancel');
  });

  testWidgets('"deleted on disk": Cancel is the default, Enter cancels', (tester) async {
    final (chosen, buttons) = await enterOn(
      tester,
      'Deleted on disk',
      () => conflictQuestion('x.txt', deleted: true),
    );
    expect(buttons.map((b) => b.label), ['Save', 'Close Without Saving', 'Cancel']);
    expect(buttons.firstWhere((b) => b.label == 'Cancel').kind, ButtonKind.primary);
    expect(chosen, 'cancel');
  });

  testWidgets('"unsaved changes": Enter saves, it never discards', (tester) async {
    final (chosen, buttons) = await enterOn(
      tester,
      'Unsaved changes',
      () => unsavedQuestion(const Prompt('quit', ['x.txt'])),
    );
    expect(buttons.map((b) => b.label), ['Save', "Don't Save", 'Cancel']);
    expect(chosen, 'save');
  });
}
