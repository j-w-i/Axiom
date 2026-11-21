// The M3 exit scenario in the Flutter shell (FL-QA-001; the Tauri suite agent.mjs), with the
// scripted model, real cargo and the in-repo fake MCP server and ACP agent, in the AI dock:
// 1 "fix the build" completes through tools only (read → build fails → patch, approved in the UI
// → build passes); 2 the turn's checkpoint comes before its first non-read tool; 3 a denied
// `rm -rf src` does not run; 4 Restore puts the file back; 5 after quitting and starting again,
// the session replays and continues with its context; 6 an MCP server's tools run after approval
// and its error is a failed call; 7 an ACP agent's write in the folder asks and happens after
// Allow once.
// Run: apps/desktop-flutter/scripts/e2e.sh agent_test.dart

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/chat/chat_page.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/agent.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

const _original = 'fn main() {\n    println!("{}", add(2, 3));\n}\n';

String _helper(String name) {
  final p = '${Directory.current.parent.parent.path}/target/debug/$name';
  if (!File(p).existsSync()) throw StateError('$p is missing (scripts/e2e.sh builds it)');
  return p;
}

/// Sends [text] from the dock's composer.
Future<void> _send(WidgetTester tester, String text) async {
  final view = tester.state<ChatViewState>(find.byWidgetPredicate((w) => w is ChatView && w.dock));
  view.composerFocus.requestFocus();
  await tester.pump(const Duration(milliseconds: 100));
  // Typed into the composer's controller (the test text input follows a field only once its
  // connection opens); Enter, a real key, sends.
  view.composer.text = text;
  await key(tester, LogicalKeyboardKey.enter);
  expect(view.composer.text, isEmpty, reason: 'Enter sent the message');
}

/// Answers every approval card with [answer] until the turn ends; the tools asked about.
Future<List<String>> _answer(WidgetTester tester, AppState app, String answer) async {
  final asked = <String>[];
  await until(
    tester,
    () => app.agent.active.running || app.agent.approvals.isNotEmpty,
    what: "the turn's start",
    detail: () =>
        'active ${app.agent.activeId}, send error "${app.agent.sendError}", error '
        '"${app.agent.error}", items ${app.agent.items().map((i) => i.runtimeType).toList()}, '
        'notices ${app.agent.items().whereType<NoticeItem>().map((n) => n.message).toList()}',
  );
  final end = DateTime.now().add(const Duration(minutes: 3));
  while (DateTime.now().isBefore(end)) {
    final a = app.agent.approvals.where((r) => r.sessionId == app.agent.activeId).firstOrNull;
    if (a != null && named(answer).evaluate().isNotEmpty) {
      asked.add([a.tool, ...?a.argv].join(' '));
      await tester.tap(named(answer).first);
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

  testWidgets('the M3 exit scenario in the dock', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-agent-data-');
    final ws = tempDir('axiom-fl-agent-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(
      ws,
      'Cargo.toml',
      '[package]\nname = "exit"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
    );
    writeFile(ws, '.gitignore', 'target/\n');
    writeFile(ws, 'src/main.rs', _original);
    final main = File('${ws.path}/src/main.rs');
    Map<String, Object?> call(String id, String name, Map<String, Object?> input) => {
      'type': 'toolCall',
      'id': id,
      'name': name,
      'input': input,
    };
    Map<String, Object?> text(String t) => {'type': 'text', 'text': t};
    final script1 = '${data.path}/script-1.json';
    File(script1).writeAsStringSync(
      jsonEncode({
        'turns': [
          {
            'expect': {
              'userContains': 'fix the build',
              'toolsInclude': ['fs.applyPatch', 'build.run'],
            },
            'events': [
              text("I'll look first."),
              call('r', 'fs.read', {'path': 'src/main.rs'}),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'r', 'contains': 'add(2, 3)'},
            },
            'events': [
              call('b1', 'build.run', {'task': 'build'}),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'b1', 'contains': 'E0425'},
            },
            'events': [
              text('`add` is missing; adding it.'),
              call('p', 'fs.applyPatch', {
                'path': 'src/main.rs',
                'edits': [
                  {
                    'search': 'fn main() {',
                    'replace': 'fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\nfn main() {',
                  },
                ],
              }),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'p', 'isError': false},
            },
            'events': [
              call('b2', 'build.run', {'task': 'build'}),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'b2', 'contains': 'succeeded'},
            },
            'events': [text('Done: the build passes.')],
          },
          {
            'expect': {'userContains': 'clean up'},
            'events': [
              call('x', 'task.run', {
                'argv': ['rm', '-rf', 'src'],
              }),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'x', 'isError': true, 'contains': 'denied'},
            },
            'events': [text("Understood, I won't delete anything.")],
          },
        ],
      }),
    );
    final script2 = '${data.path}/script-2.json';
    File(script2).writeAsStringSync(
      jsonEncode({
        'turns': [
          {
            'expect': {'historyContains': 'the build passes', 'userContains': 'still there'},
            'events': [text('Yes: this conversation resumed after the restart.')],
          },
          {
            'expect': {
              'userContains': 'use the server',
              'toolsInclude': ['mcp.fake.echo', 'mcp.fake.fail'],
            },
            'events': [
              call('m1', 'mcp.fake.echo', {'text': 'hi'}),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'm1', 'contains': 'echo: hi', 'isError': false},
            },
            'events': [call('m2', 'mcp.fake.fail', {})],
          },
          {
            'expect': {
              'toolResult': {'callId': 'm2', 'contains': 'it failed', 'isError': true},
            },
            'events': [text('The server echoed; its failing tool failed.')],
          },
        ],
      }),
    );
    final creds = '${data.path}/credentials.json';

    var app = await boot(
      tester,
      data.path,
      env: {'AXIOM_TEST_SCRIPTED_MODEL': script1, 'AXIOM_TEST_CREDENTIALS_FILE': creds},
    );
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    // The default policy (reads allowed, the rest asks) plus one rule: `cargo build` runs without
    // asking, as a person would set it.
    await app.client.callRaw('agent/setPolicy', {
      'default': 'ask',
      'rules': [
        {'effect': 'read', 'decision': 'allow'},
        {
          'effect': 'execute',
          'argvPrefix': ['cargo', 'build'],
          'decision': 'allow',
        },
      ],
    });
    await key(tester, LogicalKeyboardKey.keyL, mod: true);
    await until(
      tester,
      () =>
          find.byWidgetPredicate((w) => w is ChatView && w.dock).evaluate().isNotEmpty &&
          app.agent.scope == Scope.workspace &&
          app.agent.model != null,
      what: 'the dock',
    );

    // 1. Fix the build through tools only; the write is approved in the UI.
    await _send(tester, 'Please fix the build');
    final asked1 = await _answer(tester, app, 'Allow once');
    expect(main.readAsStringSync(), startsWith('fn add(a: i32, b: i32) -> i32 {'));
    expect(_tools(app), ['fs.read:ok', 'build.run:ok', 'fs.applyPatch:ok', 'build.run:ok']);
    expect(asked1, ['fs.applyPatch']);
    final builds = [
      for (final i in app.agent.items())
        if (i is ToolItem && i.tool == 'build.run') i.summary,
    ];
    expect(builds[0], startsWith('failed · 1 errors'));
    expect(builds[1], startsWith('succeeded · 0 errors'));

    // 2. The turn's checkpoint comes before its first non-read tool runs, so before every write.
    final order = [
      for (final i in app.agent.items())
        if (i is ToolItem) i.tool else if (i is CheckpointItem) 'checkpoint:${i.label}',
    ];
    expect(order, [
      'fs.read',
      'build.run',
      'checkpoint:before build.run',
      'fs.applyPatch',
      'build.run',
    ]);

    // 3. A denied operation does not run.
    await _send(tester, 'Now clean up');
    final asked2 = await _answer(tester, app, 'Deny');
    expect(asked2, ['task.run rm -rf src']);
    expect(main.existsSync(), isTrue);
    expect(app.agent.items().whereType<CheckpointItem>().length, 1);

    // 4. Restore puts the original file back.
    await tester.tap(named('Restore').first);
    await until(
      tester,
      () => app.agent.items().whereType<NoticeItem>().any((n) => n.message.startsWith('Restored')),
      what: 'the restore',
    );
    expect(main.readAsStringSync(), _original);
    final sid = app.agent.activeId!;

    // 5. Quit and start again: the dock is back, the session replays and continues.
    await quit(tester, app);
    app = await boot(
      tester,
      data.path,
      env: {'AXIOM_TEST_SCRIPTED_MODEL': script2, 'AXIOM_TEST_CREDENTIALS_FILE': creds},
    );
    await until(tester, () => app.workspace.isOpen, what: 'the reopened folder');
    await until(
      tester,
      () => find.byWidgetPredicate((w) => w is ChatView && w.dock).evaluate().isNotEmpty,
      what: 'the restored dock',
    );
    await until(
      tester,
      () => app.agent.sessions.any((s) => s.sessionId == sid),
      what: 'the session listed',
    );
    await app.agent.open(sid);
    await until(tester, () => _tools(app).length >= 5, what: 'the replayed history');
    await _send(tester, 'Is the fix still there in your memory?');
    await until(
      tester,
      () => app.agent.items().whereType<AssistantItem>().any(
        (a) => a.text.contains('resumed after the restart'),
      ),
      timeout: const Duration(seconds: 30),
      what: 'the resumed answer',
    );
    await until(tester, () => !app.agent.active.running, what: 'its end');

    // 6. An MCP server configured in settings: its tools are in the turn; each call asks.
    await app.client.call(
      Methods.settingsSet,
      SetParams(
        values_: {
          'agent.mcpServers': [
            {
              'name': 'fake',
              'argv': [_helper('fake-mcp')],
            },
          ],
        },
      ),
    );
    app.agent.newChat(Scope.workspace);
    await tester.pump();
    await _send(tester, 'Please use the server');
    final asked3 = await _answer(tester, app, 'Allow once');
    expect(_tools(app), ['mcp.fake.echo:ok', 'mcp.fake.fail:error']);
    expect(asked3.map((a) => a.split(' ').first).toList(), ['mcp.fake.echo', 'mcp.fake.fail']);

    // 7. An ACP agent in the folder: chosen like a model; its write asks and happens after Allow.
    final err = await app.agent.configure(
      ProviderConfig(
        id: 'fake-agent',
        kind: ProviderKind.acp,
        name: 'Fake Agent',
        command: [_helper('fake-acp')],
      ),
    );
    expect(err, '');
    app.agent.newChat(Scope.workspace);
    app.agent.setModel((providerId: 'fake-agent', model: 'default'));
    await tester.pump();
    await _send(tester, 'write the notes');
    final asked4 = await _answer(tester, app, 'Allow once');
    expect(asked4.length, 1);
    expect(File('${ws.path}/notes.md').readAsStringSync(), 'hello\n');
    expect(
      app.agent.items().whereType<AssistantItem>().any((a) => a.text.contains('wrote notes.md')),
      isTrue,
    );

    await quit(tester, app);
    semantics.dispose();
  });
}
