// The editor's frame timings (ADR-0020 §5, FL-EDIT-001's gate for re_editor): a 5000-line Rust
// file filling the window, in a profile build at the owner's scale.
//   AXIOM_FRAMES_TARGET=editor_frames_test.dart apps/desktop-flutter/scripts/frames.sh

import 'dart:io';

import 'package:axiom_desktop/editor/code_view.dart';
import 'package:axiom_desktop/editor/languages.dart';
import 'package:axiom_desktop/main.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'frame_report.dart';

String bigRust(int lines) {
  final b = StringBuffer();
  var n = 0;
  while (n < lines) {
    b.writeln('/// Computes the sum of the values in [items], skipping the negative ones (#$n).');
    b.writeln('pub fn sum_$n(items: &[i64]) -> Result<i64, String> {');
    b.writeln('    let mut total: i64 = 0; // running total');
    b.writeln('    for (i, v) in items.iter().enumerate() {');
    b.writeln('        if *v < 0 { continue; }');
    b.writeln('        total = total.checked_add(*v).ok_or_else(|| format!("overflow at {i}"))?;');
    b.writeln('    }');
    b.writeln('    Ok(total * $n + 0x2A)');
    b.writeln('}');
    b.writeln();
    n += 10;
  }
  return b.toString();
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

  testWidgets('editor frame timings', (tester) async {
    final controller = CodeLineEditingController.fromText(bigRust(5000));
    final focus = FocusNode();
    await tester.pumpWidget(
      AxiomApp(
        home: CodeView(
          controller: controller,
          language: Languages.instance.byId('rust'),
          focusNode: focus,
        ),
      ),
    );
    await Future<void>.delayed(const Duration(seconds: 2));
    focus.requestFocus();
    controller.selection = const CodeLineSelection.collapsed(index: 20, offset: 4);
    await Future<void>.delayed(const Duration(milliseconds: 500));

    final reports = <Report>[];
    final view = find.byType(CodeEditor);
    reports.add(
      await measureFrames('editor fling', () async {
        for (var i = 0; i < 4; i++) {
          await tester.fling(view, const Offset(0, -1500), 5000);
          await Future<void>.delayed(const Duration(milliseconds: 900));
        }
      }),
    );
    reports.add(
      await measureFrames('editor wheel', () async {
        for (var i = 0; i < 80; i++) {
          await tester.sendEventToBinding(
            PointerScrollEvent(position: tester.getCenter(view), scrollDelta: const Offset(0, 100)),
          );
          await Future<void>.delayed(const Duration(milliseconds: 16));
        }
      }),
    );
    reports.add(
      await measureFrames('typing', () async {
        for (final ch in 'let value = compute(items, 42); // typed\n'.split('')) {
          controller.replaceSelection(ch);
          await Future<void>.delayed(const Duration(milliseconds: 40));
        }
      }),
    );
    reports.add(
      await measureFrames('cursor down x60', () async {
        for (var i = 0; i < 60; i++) {
          controller.moveCursor(AxisDirection.down);
          await Future<void>.delayed(const Duration(milliseconds: 30));
        }
      }),
    );
    final size = tester.view.physicalSize;
    // ignore: avoid_print
    print(
      'window ${size.width.round()}x${size.height.round()} physical, dpr ${tester.view.devicePixelRatio}',
    );
    for (final r in reports) {
      // ignore: avoid_print
      print(r);
    }
    binding.reportData = {for (final r in reports) r.name: r.toJson()};
    if (Platform.environment['AXIOM_FRAMES_ASSERT'] != '0') {
      expect([
        for (final r in reports)
          if (r.over > 0) '${r.name}: ${r.over} of ${r.frames.length}',
      ], isEmpty);
    }
  });
}
