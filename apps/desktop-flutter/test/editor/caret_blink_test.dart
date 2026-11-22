// The editor's caret (FL-PERF-001): it blinks for AxiomMotion.caretBlinkTimeout after the last
// edit, caret move or focus change, then stays solid and schedules no frames; an edit or a caret
// move restarts the blink; Reduced and Off draw it solid. The patch is in
// third_party/re_editor/PATCHES.md.

import 'package:axiom_desktop/editor/code_view.dart';
import 'package:axiom_desktop/ui/theme.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

const _half = Duration(milliseconds: 500); // re_editor's blink half-period

// The desktop's path through re_editor (on mobile it shows the caret 100 ms after a change).
final _linux = TargetPlatformVariant.only(TargetPlatform.linux);

class _Editor {
  _Editor(this.tester, this.theme);
  final WidgetTester tester;
  final AxiomThemeData theme;
  final controller = CodeLineEditingController.fromText('fn main() {\n    let x = 1;\n}\n');
  final focus = FocusNode();

  Future<void> pump() async {
    tester.view.physicalSize = const Size(800, 600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    addTearDown(controller.dispose);
    addTearDown(focus.dispose);
    await tester.pumpWidget(
      WidgetsApp(
        color: theme.colors.ground,
        debugShowCheckedModeBanner: false,
        pageRouteBuilder: <T>(s, b) =>
            PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
        home: CodeView(controller: controller, language: 'rust', focusNode: focus),
        builder: (context, child) => AxiomTheme(data: theme, child: child!),
      ),
    );
    focus.requestFocus();
    await tester.pump();
    // re_editor's own short fades after focus (about 0.3 s): less than a blink's half-period,
    // so the blink's phase below is still counted from the focus change.
    final settled = tester.binding.clock.now();
    await tester.pumpAndSettle(const Duration(milliseconds: 16));
    expect(tester.binding.clock.now().difference(settled), lessThan(_half));
  }

  /// Whether the last frame drew the caret (re_editor paints it as an accent-coloured rrect).
  bool get caretDrawn {
    final editor = tester.renderObject(find.byType(CodeEditor));
    final Object caret = paints..rrect(color: theme.colors.accent);
    return (caret as Matcher).matches(editor, <dynamic, dynamic>{});
  }

  /// Lets [d] pass on the test clock without drawing a frame, then says whether anything asked
  /// for one.
  Future<bool> frameRequestedWithin(Duration d) async {
    expect(tester.binding.hasScheduledFrame, isFalse, reason: 'a frame was already pending');
    await tester.binding.delayed(d);
    return tester.binding.hasScheduledFrame;
  }
}

AxiomThemeData _theme(MotionMode m) => AxiomThemeData(colors: AxiomColors.dark, motionMode: m);

void main() {
  final timeout = AxiomMotion(MotionMode.standard).caretBlinkTimeout;

  testWidgets('Standard: the caret blinks, then stays solid and asks for no frames', (
    tester,
  ) async {
    final e = _Editor(tester, _theme(MotionMode.standard));
    await e.pump();
    expect(e.caretDrawn, isTrue);
    await tester.pump(_half);
    expect(e.caretDrawn, isFalse, reason: 'half a period later the blink hides it');
    await tester.pump(_half);
    expect(e.caretDrawn, isTrue);

    // Past the timeout (counted from the focus change), the caret is shown and stays.
    await tester.pump(timeout);
    expect(e.caretDrawn, isTrue);
    expect(await e.frameRequestedWithin(const Duration(seconds: 5)), isFalse);
    await tester.pump();
    expect(e.caretDrawn, isTrue);

    // Focus leaves: no caret, even though the blink had stopped.
    e.focus.unfocus();
    await tester.pump();
    expect(e.caretDrawn, isFalse);
  }, variant: _linux);

  testWidgets('Standard: an edit or a caret move restarts the blink', (tester) async {
    final e = _Editor(tester, _theme(MotionMode.standard));
    await e.pump();
    await tester.pump(timeout + _half);
    expect(await e.frameRequestedWithin(const Duration(seconds: 2)), isFalse);

    // A caret move.
    e.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 4);
    await tester.pump();
    expect(e.caretDrawn, isTrue);
    expect(await e.frameRequestedWithin(_half), isTrue, reason: 'the blink is running again');
    await tester.pump();
    expect(e.caretDrawn, isFalse);

    // It blinks for the whole timeout again, then stops.
    await tester.pump(timeout);
    expect(e.caretDrawn, isTrue);
    expect(await e.frameRequestedWithin(const Duration(seconds: 2)), isFalse);

    // An edit.
    e.controller.replaceSelection('y');
    await tester.pump();
    expect(e.controller.codeLines[1].text, '    ylet x = 1;');
    expect(e.caretDrawn, isTrue);
    expect(await e.frameRequestedWithin(_half), isTrue, reason: 'the blink is running again');
    await tester.pump();
    expect(e.caretDrawn, isFalse);
  }, variant: _linux);

  for (final mode in [MotionMode.reduced, MotionMode.off]) {
    testWidgets('${mode.name}: the caret is solid and asks for no frames', (tester) async {
      final e = _Editor(tester, _theme(mode));
      await e.pump();
      expect(e.caretDrawn, isTrue);
      expect(await e.frameRequestedWithin(const Duration(seconds: 3)), isFalse);
      await tester.pump();
      expect(e.caretDrawn, isTrue);

      // A caret move shows it solid at the new place, still without a blink.
      e.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 2);
      await tester.pump();
      expect(e.caretDrawn, isTrue);
      expect(await e.frameRequestedWithin(const Duration(seconds: 3)), isFalse);
    }, variant: _linux);
  }
}
