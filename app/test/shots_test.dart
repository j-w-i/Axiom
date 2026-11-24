import 'package:axiom/gallery/workbench.dart';
import 'package:axiom/patterns/usage.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

void main() {
  testWidgets('workbench, dark', (t) async {
    await mount(t, const Workbench());
    await snap(t, 'workbench-dark');
    await t.tap(find.byType(UsageKey).first);
    await t.pump(const Duration(milliseconds: 50));
    await t.pump(const Duration(milliseconds: 300));
    await snap(t, 'workbench-usage');
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
