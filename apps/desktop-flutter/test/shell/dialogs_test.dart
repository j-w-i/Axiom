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

List<String> labels(WidgetTester tester) => tester
    .widgetList<AxButton>(
      find.descendant(of: find.byType(AxQuestion), matching: find.byType(AxButton)),
    )
    .map((b) => b.label)
    .toList();

/// Shows the conflict question the way the shell does ([showConflictQuestion], following
/// [current]), runs [answer], and gives the answer.
Future<String?> ask(
  WidgetTester tester,
  ValueNotifier<ConflictView?> current,
  Future<void> Function() answer,
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
            chosen = await showConflictQuestion(context, current, () => current.value);
            answered = true;
          },
        ),
      ),
    ),
    const Size(900, 600),
  );
  await tester.tap(find.text('Ask'));
  await tester.pumpAndSettle();
  await answer();
  await tester.pumpAndSettle();
  expect(answered, isTrue, reason: 'the question was not answered');
  return chosen;
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

  testWidgets('"cannot be opened": no Reload; Cancel is the default, Enter cancels', (
    tester,
  ) async {
    final (chosen, buttons) = await enterOn(
      tester,
      'Cannot open file',
      () => conflictQuestion('x.dat', deleted: false, cannotOpen: 'it is not a UTF-8 text file'),
    );
    expect(buttons.map((b) => b.label), ['Overwrite', 'Close Without Saving', 'Cancel']);
    expect(buttons.firstWhere((b) => b.label == 'Cancel').kind, ButtonKind.primary);
    expect(chosen, 'cancel');
  });

  testWidgets('the "deleted" question\'s Save answers create-only, not overwrite', (tester) async {
    final chosen = await ask(
      tester,
      ValueNotifier<ConflictView?>((name: 'x.txt', kind: ConflictKind.deleted, reason: '')),
      () => tester.tap(find.text('Save')),
    );
    expect(chosen, 'create');
  });

  testWidgets('E1: an open "deleted" question becomes "changed" when the file comes back, and '
      'Enter still cancels', (tester) async {
    final semantics = tester.ensureSemantics();
    final current = ValueNotifier<ConflictView?>((
      name: 'x.txt',
      kind: ConflictKind.deleted,
      reason: '',
    ));
    final chosen = await ask(tester, current, () async {
      expect(find.text('x.txt was deleted on disk'), findsOneWidget);
      expect(find.bySemanticsLabel('Deleted on disk'), findsOneWidget);
      expect(labels(tester), ['Save', 'Close Without Saving', 'Cancel']);
      // The person moves the focus to Save (Shift+Tab from Cancel, twice).
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.pump();
      // Another program writes the file: the store's conflict is "changed" now.
      current.value = (name: 'x.txt', kind: ConflictKind.changed, reason: '');
      await tester.pumpAndSettle();
      expect(find.text('x.txt was deleted on disk'), findsNothing);
      expect(find.text('x.txt changed on disk'), findsOneWidget);
      expect(find.bySemanticsLabel('Changed on disk'), findsOneWidget);
      expect(labels(tester), ['Overwrite', 'Reload', 'Cancel']);
      // The new question's default answer has the focus, not Overwrite where Save was: Enter
      // does not overwrite.
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    });
    expect(chosen, 'cancel');
    semantics.dispose();
  });

  testWidgets('a "cannot be opened" question becomes "changed" when the file opens again', (
    tester,
  ) async {
    final current = ValueNotifier<ConflictView?>((
      name: 'x.dat',
      kind: ConflictKind.unreadable,
      reason: 'it is not a UTF-8 text file',
    ));
    final chosen = await ask(tester, current, () async {
      expect(find.text('x.dat cannot be opened'), findsOneWidget);
      expect(find.textContaining('(it is not a UTF-8 text file)'), findsOneWidget);
      expect(find.text('Reload'), findsNothing);
      current.value = (name: 'x.dat', kind: ConflictKind.changed, reason: '');
      await tester.pumpAndSettle();
      expect(labels(tester), ['Overwrite', 'Reload', 'Cancel']);
      await tester.tap(find.text('Reload'));
    });
    expect(chosen, 'reload');
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
