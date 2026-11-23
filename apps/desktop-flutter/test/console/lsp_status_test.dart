// The status bar's language-server segment does something in every state (AUD-056) and shows
// keyboard focus (AUD-070).

import 'package:axiom_desktop/console/console.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/main.dart';
import 'package:axiom_desktop/ui/pressable.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> pump(WidgetTester tester, Widget w) async {
  tester.view.physicalSize = const Size(800, 400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    AxiomApp(
      home: Align(alignment: Alignment.bottomRight, child: w),
    ),
  );
  await tester.pump();
}

void main() {
  // First in the file: the test binding drops keyboard handlers between tests, and the focus
  // rings' keyboard modality (FocusVisibility) is registered once per process.
  testWidgets('Tab focuses the segment with a visible ring', (tester) async {
    await pump(
      tester,
      LspStatusView(serverId: 'rust-analyzer', state: ServerState.ready, onRestart: () {}),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    expect(
      tester
          .widgetList<FocusRing>(
            find.descendant(of: find.byType(LspStatusView), matching: find.byType(FocusRing)),
          )
          .any((r) => r.visible),
      isTrue,
    );
  });

  for (final state in [
    ServerState.ready,
    ServerState.idle,
    ServerState.starting,
    ServerState.failed,
  ]) {
    testWidgets('${state.wire}: the menu shows the state and restarts the server', (tester) async {
      var restarts = 0;
      await pump(
        tester,
        LspStatusView(
          serverId: 'rust-analyzer',
          state: state,
          progress: state == ServerState.starting ? 'indexing 40%' : null,
          onRestart: () => restarts++,
        ),
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      await tester.pump();
      expect(find.textContaining('rust-analyzer: '), findsOneWidget);
      await tester.tap(find.text('Restart rust-analyzer'));
      await tester.pump();
      expect(restarts, 1);
    });
  }

  testWidgets('missing: the menu offers the install, disabled while it runs', (tester) async {
    var installs = 0;
    await pump(
      tester,
      LspStatusView(
        serverId: 'rust-analyzer',
        state: ServerState.missing,
        installLabel: 'Install rust-analyzer…',
        onRestart: () {},
        onInstall: () => installs++,
      ),
    );
    await tester.tap(find.text('rust-analyzer'));
    await tester.pump();
    await tester.pump();
    expect(find.text('rust-analyzer: not installed'), findsOneWidget);
    expect(find.text('Restart rust-analyzer'), findsNothing);
    await tester.tap(find.text('Install rust-analyzer…'));
    await tester.pump();
    expect(installs, 1);

    await pump(
      tester,
      LspStatusView(
        serverId: 'rust-analyzer',
        state: ServerState.missing,
        installLabel: 'Installing rust-analyzer…',
        onRestart: () {},
      ),
    );
    final entries = tester.widget<LspStatusView>(find.byType(LspStatusView)).entries();
    expect(entries.last.label, 'Installing rust-analyzer…');
    expect(entries.last.enabled, isFalse);
  });
}
