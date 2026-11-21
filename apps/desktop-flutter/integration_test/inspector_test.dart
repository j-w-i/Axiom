// The Visual Inspector (M5-UI-001), through the real app and daemon, with the React + Vite fixture
// (tests/fixtures/inspect-react; `pnpm install` there first) and a real page in headless Chrome:
// Start preview in the Inspector; the page connects; inspect mode on; a click on the button in the
// page (sent as the person's mouse, over the browser's debugging port: the test's hand, not the
// product's) selects it; its source opens at line 11, column 9; the library's badge shows its
// source as uncertain; Ask AI attaches the element to the dock; the panels are named in both
// themes. Stop preview ends the dev server.
//
// Without the fixture's node_modules or a Chrome the suite is skipped with a note, unless
// AXIOM_REQUIRE_PREVIEW=1 (CI), where it fails.
// Run: apps/desktop-flutter/scripts/e2e.sh inspector_test.dart

import 'dart:io';
import 'dart:ui' show SemanticsAction;

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'browser.dart';
import 'support.dart';

/// Every node a person can act on has a name.
List<String> unnamed() => [
  for (final n in find.semantics.byPredicate((n) {
    final d = n.getSemanticsData();
    final acts =
        d.hasAction(SemanticsAction.tap) ||
        d.hasAction(SemanticsAction.focus) ||
        d.flagsCollection.isTextField;
    return acts && d.label.trim().isEmpty && d.value.trim().isEmpty && d.hint.trim().isEmpty;
  }).evaluate())
    '#${n.id} ${n.rect}',
];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the inspector: start, select in the page, source, uncertain, ask AI', (
    tester,
  ) async {
    final root = repoRoot();
    final fixture = '$root/tests/fixtures/inspect-react';
    final chrome = findChrome();
    final missing = previewUnavailable(fixture, chrome);
    if (missing != null) {
      if (Platform.environment['AXIOM_REQUIRE_PREVIEW'] == '1') fail(missing);
      // ignore: avoid_print
      print('SKIPPED: $missing');
      return;
    }

    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-inspector-data-');
    final ws = tempDir('axiom-fl-inspector-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    copyInspectFixture(fixture, ws);

    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    final s = app.inspector;

    // 1. The Inspector from the rail; Start preview.
    await tester.tap(named('Inspector'));
    await until(tester, () => app.destination == Destination.inspector, what: 'the Inspector');
    await until(tester, () => named('Start preview').evaluate().isNotEmpty, what: 'Start preview');
    final before = unnamed();
    await tester.tap(named('Start preview'));
    await until(
      tester,
      () => s.running,
      timeout: const Duration(seconds: 90),
      what: 'the preview',
      detail: () => '${s.preview?.toJson()} ${s.error}',
    );
    final url = s.preview!.url!;
    expect(url, startsWith('http://127.0.0.1:'));
    await until(
      tester,
      () => find.text('Waiting for the page: open it in a browser').evaluate().isNotEmpty,
      what: 'waiting for the page',
    );

    // 2. The person's browser opens it: the page connects.
    final page = await Page.open(chrome!, url);
    addTearDown(page.close);
    await until(
      tester,
      () => find.text('Page connected').evaluate().isNotEmpty,
      timeout: const Duration(seconds: 30),
      what: 'the page',
    );

    // 3. Inspect mode, then a click on the button in the page.
    await tester.tap(named('Inspect mode'));
    await until(tester, () => s.inspectMode, what: 'inspect mode');
    Future<Rect> boundsOf(String tag) async {
      for (var i = 0; i < 50; i++) {
        final snap = await app.client.call(
          Methods.uiSnapshot,
          SnapshotParams(previewId: s.preview!.previewId),
        );
        final n = snap.nodes.where((n) => n.typeName == tag).firstOrNull;
        if (n?.bounds != null) return n!.bounds!;
        await tester.pump(const Duration(milliseconds: 200));
      }
      fail('no $tag in the page');
    }

    final b = await boundsOf('button');
    await page.click(b.x + b.width / 2, b.y + b.height / 2);
    await until(
      tester,
      () => named('Open src/App.jsx:11:9').evaluate().isNotEmpty,
      what: 'the selected button and its source',
      detail: () => '${s.selection.map((n) => n.typeName)}',
    );
    expect(find.text('button "Pay now"'), findsWidgets);
    // Its properties (computed in the page) and its hierarchy.
    await until(tester, () => s.details != null, what: 'the details');
    expect(s.details!.properties?['css.margin-left'], '8px');
    expect(
      find.bySemanticsLabel(RegExp(r'^Select main')).evaluate(),
      isNotEmpty,
      reason: 'the hierarchy lists the ancestors',
    );

    // 4. The exact source opens at line 11, column 9.
    await tester.tap(named('Open src/App.jsx:11:9'));
    await until(
      tester,
      () => app.documents.activePath == 'src/App.jsx',
      what: 'App.jsx in the editor',
    );
    await until(
      tester,
      () => app.documents.cursorText.startsWith('Ln 11, Col 9'),
      what: 'the caret at 11:9',
      detail: () => app.documents.cursorText,
    );

    // 5. The library's element: its source is uncertain.
    final em = await boundsOf('em');
    await page.click(em.x + em.width / 2, em.y + em.height / 2);
    await until(
      tester,
      () => named('Open src/App.jsx:7:5, uncertain').evaluate().isNotEmpty,
      what: 'the badge, uncertain',
      detail: () =>
          '${find.semantics.byPredicate((n) => n.getSemanticsData().label.contains('src/App')).evaluate().map((n) => n.getSemanticsData().label).toList()}',
    );
    expect(find.textContaining('Uncertain: rendered by src/App.jsx:7:5'), findsOneWidget);

    // 6. Ask AI about this element: the dock opens with the element attached.
    await tester.tap(named('Ask AI about this element'));
    await until(tester, () => app.dockVisible, what: 'the dock');
    await until(
      tester,
      () => find.bySemanticsLabel(RegExp(r'^Attached: em')).evaluate().isNotEmpty,
      what: 'the attached element',
    );
    expect(app.agent.context.single.attachment.path, 'src/App.jsx');
    expect(app.agent.context.single.attachment.lines, [7, 26]);

    // 7. Named controls in both themes.
    final report = <String, List<String>>{};
    if (before.isNotEmpty) report['before the preview'] = before;
    for (final theme in ['dark', 'light']) {
      await app.settings.setValue('appearance.theme', theme);
      for (var i = 0; i < 10; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      final u = unnamed();
      if (u.isNotEmpty) report[theme] = u;
    }
    expect(report, isEmpty, reason: 'unnamed controls: $report');

    // 8. Stop preview: the dev server ends.
    await tester.tap(named('Stop preview'));
    await until(tester, () => s.preview == null, what: 'the preview stopped');
    await until(
      tester,
      () => named('Start preview').evaluate().isNotEmpty,
      what: 'Start preview again',
    );

    await quit(tester, app);
    semantics.dispose();
  });
}
