import 'package:axiom/gallery/workbench.dart';
import 'package:axiom/patterns/context_gauge.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/patterns/usage.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

void main() {
  testWidgets('workbench, dark', (t) async {
    await mount(t, const Workbench());
    await snap(t, 'workbench-dark');
    await t.tap(find.byType(UsageKey).first);
    await t.pump(const Duration(milliseconds: 50));
    await t.pump(const Duration(milliseconds: 600));
    await snap(t, 'workbench-usage');
  });
  testWidgets('workbench, what the agent runs on', (t) async {
    await mount(t, const Workbench());
    Future<void> tap(Finder f) async {
      await t.tap(f);
      await t.pump(const Duration(milliseconds: 50));
      await t.pump(const Duration(milliseconds: 600));
    }

    await tap(find.byType(ExecutionKey));
    await snap(t, 'execution-open');
    await tap(find.bySemanticsLabel('Model: GPT-5.6'));
    await tap(find.bySemanticsLabel('GPT-5.6 mini'));
    await snap(t, 'execution-cost');
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pump();
    await tap(find.byType(ContextGauge));
    await snap(t, 'context-open');
  });
  testWidgets('workbench, narrow', (t) async {
    await mount(t, const Workbench(), size: const Size(820, 700));
    await snap(t, 'workbench-narrow');
  });
  testWidgets('workbench, light', (t) async {
    await mount(t, const Workbench(), theme: const AxTheme(colors: AxColors.light));
    await snap(t, 'workbench-light');
  });
}
