// The M5 exit scenario (M5-QA-001; milestones.md §M5) through the real app and daemon, on the
// inspector fixture with a real page in headless Chrome and the scripted model:
// 1 a click on the button in the page selects it and its source opens at line 11, column 9;
// 2 "Ask AI about this element" puts the element in the dock; the agent inspects it, applies the
//   spacing fix (approved in the UI), the page hot-reloads, and `ui.verify` with the geometry
//   check stated first reports `verified`, shown in the Inspector;
// 3 a deliberately wrong patch (40 px against a 24 px check) reports `failed`;
// 4 an element from the uninstrumented library is presented as uncertain, not exact.
//
// Skipped with a note without the fixture's node_modules or a Chrome, unless
// AXIOM_REQUIRE_PREVIEW=1 (CI), where it fails.
// Run: apps/desktop-flutter/scripts/e2e.sh inspect_loop_test.dart

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/chat/chat_page.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/agent.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'browser.dart';
import 'support.dart';

const _button = 'src/App.jsx:11:9#0';
const _amount = 'src/App.jsx:10:9#0';

Map<String, Object?> _call(String id, String name, Map<String, Object?> input) => {
  'type': 'toolCall',
  'id': id,
  'name': name,
  'input': input,
};

Map<String, Object?> _text(String t) => {'type': 'text', 'text': t};

/// `button.left == amount.right + offset`, the check stated before the patch.
Map<String, Object?> _gap(String id, int offset) => _call(id, 'ui.verify', {
  'assertions': [
    {
      'kind': 'geometry',
      'node': {'stableKey': _button},
      'edge': 'left',
      'op': 'eq',
      'other': {
        'node': {'stableKey': _amount},
        'edge': 'right',
      },
      'offset': offset,
    },
  ],
  'target': {'stableKey': _button},
});

/// Sends what the dock's composer holds plus [more], with Enter.
Future<void> _send(WidgetTester tester, String more) async {
  final view = tester.state<ChatViewState>(find.byWidgetPredicate((w) => w is ChatView && w.dock));
  view.composerFocus.requestFocus();
  await tester.pump(const Duration(milliseconds: 100));
  view.composer.text = '${view.composer.text}$more';
  await key(tester, LogicalKeyboardKey.enter);
  expect(view.composer.text, isEmpty, reason: 'Enter sent the message');
}

/// Approves the turn's writes in the UI until it ends; the tools asked about.
Future<List<String>> _approve(WidgetTester tester, AppState app) async {
  final asked = <String>[];
  await until(
    tester,
    () => app.agent.active.running || app.agent.approvals.isNotEmpty,
    what: "the turn's start",
    detail: () => 'send error "${app.agent.sendError}", error "${app.agent.error}"',
  );
  final end = DateTime.now().add(const Duration(minutes: 2));
  while (DateTime.now().isBefore(end)) {
    final a = app.agent.approvals.where((r) => r.sessionId == app.agent.activeId).firstOrNull;
    if (a != null && named('Allow once').evaluate().isNotEmpty) {
      asked.add(a.tool);
      await tester.tap(named('Allow once').first);
      await tester.pump(const Duration(milliseconds: 200));
      continue;
    }
    if (!app.agent.active.running && app.agent.approvals.isEmpty) return asked;
    await tester.pump(const Duration(milliseconds: 100));
  }
  fail('the turn did not end');
}

List<String> _tools(AppState app) => [
  for (final i in app.agent.items())
    if (i is ToolItem) '${i.tool}:${i.state.name}',
];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the M5 exit scenario: select, source, fix, verified; wrong, failed; uncertain', (
    tester,
  ) async {
    final fixture = '${repoRoot()}/tests/fixtures/inspect-react';
    final chrome = findChrome();
    final missing = previewUnavailable(fixture, chrome);
    if (missing != null) {
      if (Platform.environment['AXIOM_REQUIRE_PREVIEW'] == '1') fail(missing);
      // ignore: avoid_print
      print('SKIPPED: $missing');
      return;
    }
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-m5-data-');
    final ws = tempDir('axiom-fl-m5-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    copyInspectFixture(fixture, ws);
    final appJsx = File('${ws.path}/src/App.jsx');

    final script = '${data.path}/script.json';
    File(script).writeAsStringSync(
      jsonEncode({
        'turns': [
          // The fix: the selected element (from the Inspector) and its source lines are in the
          // turn; inspect, state the check, patch, verify.
          {
            'expect': {
              'userContains': 'button "Pay now", key $_button',
              'systemContains': 'ui.verify',
              'toolsInclude': ['ui.inspect', 'ui.verify', 'fs.applyPatch'],
            },
            'events': [
              _call('i', 'ui.inspect', {'stableKey': _button}),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'i', 'contains': 'css.margin-left: 8px'},
            },
            'events': [
              _text('Success means the button starts 24 px after the amount.'),
              _call('p1', 'fs.applyPatch', {
                'path': 'src/App.jsx',
                'edits': [
                  {'search': 'marginLeft: 8 }', 'replace': 'marginLeft: 24 }'},
                ],
              }),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'p1', 'isError': false},
            },
            'events': [_gap('v1', 24)],
          },
          {
            'expect': {
              'toolResult': {'callId': 'v1', 'contains': 'verdict: verified'},
            },
            'events': [_text('Verified: the gap is 24 px.')],
          },
          // The deliberately wrong patch: 40 px, checked against 24.
          {
            'expect': {'userContains': 'wrong'},
            'events': [
              _call('p2', 'fs.applyPatch', {
                'path': 'src/App.jsx',
                'edits': [
                  {'search': 'marginLeft: 24 }', 'replace': 'marginLeft: 40 }'},
                ],
              }),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'p2', 'isError': false},
            },
            'events': [_gap('v2', 24)],
          },
          {
            'expect': {
              'toolResult': {'callId': 'v2', 'contains': 'verdict: failed'},
            },
            'events': [_text('The check failed: the gap is not 24 px.')],
          },
        ],
      }),
    );

    final app = await boot(tester, data.path, env: {'AXIOM_TEST_SCRIPTED_MODEL': script});
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    // The default policy: reads run, writes ask (and are approved in the UI below).
    await app.client.callRaw('agent/setPolicy', {
      'default': 'ask',
      'rules': [
        {'effect': 'read', 'decision': 'allow'},
      ],
    });
    final s = app.inspector;

    // The preview, and the person's browser on it.
    await tester.tap(named('Inspector'));
    await until(tester, () => named('Start preview').evaluate().isNotEmpty, what: 'Start preview');
    await tester.tap(named('Start preview'));
    await until(
      tester,
      () => s.running,
      timeout: const Duration(seconds: 90),
      what: 'the preview',
      detail: () => '${s.preview?.toJson()} ${s.error}',
    );
    final page = await Page.open(chrome!, s.preview!.url!);
    addTearDown(page.close);
    await until(tester, () => s.connected, timeout: const Duration(seconds: 30), what: 'the page');
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

    // 1. A click on the button in the page: its source opens at line 11, column 9.
    final b = await boundsOf('button');
    await page.click(b.x + b.width / 2, b.y + b.height / 2);
    await until(
      tester,
      () => named('Open src/App.jsx:11:9').evaluate().isNotEmpty,
      what: 'the button selected',
    );
    await tester.tap(named('Open src/App.jsx:11:9'));
    await until(
      tester,
      () => app.documents.cursorText.startsWith('Ln 11, Col 9'),
      what: 'the caret at 11:9',
      detail: () => '${app.documents.activePath} ${app.documents.cursorText}',
    );

    // 2. Ask AI about it: the fix, approved in the UI, hot-reloaded and verified.
    await tester.tap(named('Ask AI about this element'));
    await until(
      tester,
      () =>
          find.byWidgetPredicate((w) => w is ChatView && w.dock).evaluate().isNotEmpty &&
          app.agent.model != null &&
          find.bySemanticsLabel(RegExp(r'^Attached: button')).evaluate().isNotEmpty,
      what: 'the dock with the element attached',
    );
    await _send(tester, 'make the gap to the amount 24 px.');
    final asked1 = await _approve(tester, app);
    expect(asked1, ['fs.applyPatch']);
    expect(_tools(app), ['ui.inspect:ok', 'fs.applyPatch:ok', 'ui.verify:ok']);
    expect(appJsx.readAsStringSync(), contains('marginLeft: 24 }'));
    await until(
      tester,
      () => named('Verification: Verified').evaluate().isNotEmpty,
      what: 'Verified in the Inspector',
      detail: () => '${s.report?.report.toJson()}',
    );
    final r1 = s.report!.report;
    expect(
      r1.checks.where((c) => c.kind == CheckKind.geometry).single.passed,
      isTrue,
      reason: '${r1.toJson()}',
    );
    expect(app.agent.context, isEmpty, reason: 'the attachment went with the message');

    // 3. A deliberately wrong patch: failed.
    await _send(tester, 'Now apply the wrong value on purpose.');
    final asked2 = await _approve(tester, app);
    expect(asked2, ['fs.applyPatch']);
    expect(appJsx.readAsStringSync(), contains('marginLeft: 40 }'));
    await until(
      tester,
      () => named('Verification: Failed').evaluate().isNotEmpty,
      what: 'Failed in the Inspector',
      detail: () => '${s.report?.report.toJson()}',
    );
    expect(
      s.report!.report.checks.where((c) => c.kind == CheckKind.geometry).single.passed,
      isFalse,
    );

    // 4. The library's element: uncertain, not exact.
    final em = await boundsOf('em');
    await page.click(em.x + em.width / 2, em.y + em.height / 2);
    await until(
      tester,
      () => named('Open src/App.jsx:7:5, uncertain').evaluate().isNotEmpty,
      what: 'the badge, uncertain',
    );
    expect(named('Open src/App.jsx:7:5'), findsNothing);

    await tester.tap(named('Stop preview'));
    await until(tester, () => s.preview == null, what: 'the preview stopped');
    await quit(tester, app);
    semantics.dispose();
  });
}
