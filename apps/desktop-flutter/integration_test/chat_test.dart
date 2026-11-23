// Chat (FL-CHAT-001), against the real daemon, as the Tauri suites chat.mjs, dock.mjs, models.mjs
// and agent.mjs's ACP part:
// 1. No folder, the scripted model: Chat opens from the rail under Files, Enter sends, streamed
//    Markdown with a table and a highlighted code block, the title from the first message, ↑
//    edits (the branch is replaced), Regenerate, an image reaches the model and shows, F2
//    renames, Delete (with the question) removes it from the daemon too.
// 2. A folder, the scripted model: Mod+L opens the dock beside the editor in the folder's scope;
//    an edit asks (the card with the diff, the rail's badge), Allow once applies it, Esc denies
//    the next; Restore puts the file back; a file the person changed after the agent is a
//    conflict that restores nothing; Mod+L closes the dock.
// 3. No folder, no model: the first-run setup; an OpenAI-compatible server (a fake one on
//    loopback) added with a key that never shows; a conversation with it; Settings → Models
//    lists it with its key stored; removing it brings the setup back; Claude Code installed from
//    the setup (a fake npm); the fake ACP agent (tests/helpers/fake-acp) chosen in the picker
//    answers without a folder, in a scratch folder, and its write waits for Allow.
// Credentials go to a test file (AXIOM_TEST_CREDENTIALS_FILE), never the keychain.
// Run: apps/desktop-flutter/scripts/e2e.sh chat_test.dart

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/chat/chat_page.dart';
import 'package:axiom_desktop/chat/markdown_view.dart';
import 'package:axiom_desktop/chat/models.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/agent.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/text_input.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

const _png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

String _repo() => Directory.current.parent.parent.path;

String _fakeAcp() {
  final p = '${_repo()}/target/debug/fake-acp';
  if (!File(p).existsSync()) {
    throw StateError('$p is missing (scripts/e2e.sh builds it)');
  }
  return p;
}

List<Item> _items(AppState app) => app.agent.items();

String _assistant(AppState app) =>
    _items(app).whereType<AssistantItem>().map((i) => i.text).join('\n');

Future<void> _send(WidgetTester tester, String text) async {
  await typeFocused(tester, text);
  await key(tester, LogicalKeyboardKey.enter);
}

Future<void> _idle(WidgetTester tester, AppState app, {String what = 'the answer'}) => until(
  tester,
  () => app.agent.activeId != null && !app.agent.active.running,
  timeout: const Duration(seconds: 30),
  what: what,
);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('chat without a folder (scripted model)', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-chat-data-');
    addTearDown(() => data.deleteSync(recursive: true));
    final script = '${data.path}/script.json';
    File(script).writeAsStringSync(
      jsonEncode({
        'turns': [
          {
            'expect': {'userContains': 'hello'},
            'events': [
              {
                'type': 'text',
                'text':
                    'Here is a table:\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n'
                    '```rust\nfn main() { let x = 42; }\n```\n',
              },
            ],
          },
          {
            'expect': {'userContains': 'edited question'},
            'events': [
              {'type': 'text', 'text': 'Edited answer.'},
            ],
          },
          {
            'expect': {'userContains': 'edited question'},
            'events': [
              {'type': 'text', 'text': 'Regenerated answer.'},
            ],
          },
          {
            'expect': {'userImages': 1},
            'events': [
              {'type': 'text', 'text': 'I see one image.'},
            ],
          },
        ],
      }),
    );
    final app = await boot(
      tester,
      data.path,
      env: {
        'AXIOM_TEST_SCRIPTED_MODEL': script,
        'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json',
      },
    );

    // 1. Chat is on the rail under Files; it opens with no folder and a model is chosen.
    Finder railItem(String label) =>
        find.byWidgetPredicate((w) => w is AxIconButton && w.label == label);
    expect(
      tester.getTopLeft(railItem('Chat')).dy,
      greaterThan(tester.getTopLeft(railItem('Files')).dy),
      reason: 'Chat is under Files',
    );
    await tester.tap(named('Chat'));
    await until(tester, () => app.agent.status == 'ready', what: 'the providers');
    expect(app.agent.model?.providerId, 'scripted');
    expect(find.byType(ChatView), findsOneWidget);
    final view = tester.state<ChatViewState>(find.byType(ChatView));
    await until(tester, () => view.composerFocus.hasFocus, what: 'the composer focused');

    // 2. Enter sends; Markdown streams in with a table and a highlighted code block.
    await _send(tester, 'hello there');
    await _idle(tester, app);
    expect(_assistant(app), contains('Here is a table'));
    expect(find.byType(Table), findsOneWidget);
    final code = tester.widget<CodeBlock>(find.byType(CodeBlock));
    expect(code.code, 'fn main() { let x = 42; }');
    expect(code.language, 'rust');
    final rich = tester.widget<RichText>(
      find.descendant(of: find.byType(CodeBlock), matching: find.byType(RichText)).first,
    );
    var coloured = 0;
    rich.text.visitChildren((s) {
      if (s is TextSpan && s.style?.color != null && (s.text ?? '').trim().isNotEmpty) coloured++;
      return true;
    });
    expect(coloured, greaterThan(1), reason: 'the code block is coloured by token');
    await until(
      tester,
      () => app.agent.sessions.any((s) => s.title == 'hello there'),
      what: 'the title from the first message',
    );

    // 3. ↑ in the empty composer edits the message: the branch is replaced.
    await key(tester, LogicalKeyboardKey.arrowUp);
    expect(view.composer.text, 'hello there');
    await _send(tester, 'edited question');
    await until(tester, () => _assistant(app).contains('Edited answer.'), what: 'the edit');
    await _idle(tester, app);
    expect(_items(app).whereType<UserItem>().map((u) => u.text).toList(), ['edited question']);
    expect(_items(app).whereType<AssistantItem>().length, 1);

    // 4. Regenerate replaces the last answer.
    await tester.tap(named('Regenerate'));
    await until(tester, () => _assistant(app).contains('Regenerated'), what: 'the regeneration');
    await _idle(tester, app);
    expect(_items(app).whereType<AssistantItem>().map((a) => a.text).toList(), [
      'Regenerated answer.',
    ]);

    // 5. An attached image reaches the model (the script expects it) and shows as a thumbnail.
    view.addImage(base64Decode(_png), 'dot.png');
    await tester.pump();
    expect(named('Attached image 1'), findsOneWidget);
    await _send(tester, 'what is this');
    await until(tester, () => _assistant(app).contains('I see one image.'), what: 'the image');
    await _idle(tester, app);
    await until(
      tester,
      () => find.byType(Thumbnail).evaluate().isNotEmpty,
      what: 'the thumbnail in the transcript',
    );

    // 6. F2 renames the focused conversation; Delete asks, then removes it from the daemon.
    final id = app.agent.activeId!;
    final row = find.descendant(of: find.byType(ChatSide), matching: find.text('hello there'));
    await tester.tap(row);
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.f2);
    await until(tester, () => named('Title').evaluate().isNotEmpty, what: 'the rename dialog');
    await typeFocused(tester, 'Renamed chat');
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => app.agent.sessions.any((s) => s.sessionId == id && s.title == 'Renamed chat'),
      what: 'the rename',
    );
    await tester.tap(
      find.descendant(of: find.byType(ChatSide), matching: find.text('Renamed chat')),
    );
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.delete);
    await until(tester, () => named('Delete').evaluate().isNotEmpty, what: 'the delete question');
    await tester.tap(named('Delete'));
    await until(tester, () => app.agent.sessions.isEmpty, what: 'the deletion');
    final listed = await app.client.call(Methods.agentSessionList, const SessionListParams());
    expect(listed.sessions.where((s) => s.sessionId == id), isEmpty);

    await quit(tester, app);
    semantics.dispose();
  });

  testWidgets('the dock beside the editor, approvals and checkpoints (scripted model)', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-dock-data-');
    final ws = tempDir('axiom-fl-dock-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(ws, 'src/lib.rs', 'fn a() {}\n');
    final lib = File('${ws.path}/src/lib.rs');
    Map<String, Object?> edit(String id, String search, String replace) => {
      'type': 'toolCall',
      'id': id,
      'name': 'fs.applyPatch',
      'input': {
        'path': 'src/lib.rs',
        'edits': [
          {'search': search, 'replace': replace},
        ],
      },
    };
    final script = '${data.path}/script.json';
    File(script).writeAsStringSync(
      jsonEncode({
        'turns': [
          {
            'expect': {'userContains': 'public'},
            'events': [
              {'type': 'text', 'text': 'Making it public.'},
              edit('c1', 'fn a() {}', 'pub fn a() {}'),
            ],
          },
          {
            'expect': {
              'toolResult': {'callId': 'c1', 'isError': false},
            },
            'events': [edit('c2', 'pub fn a() {}', 'pub fn a2() {}')],
          },
          {
            'expect': {
              'toolResult': {'callId': 'c2', 'isError': true, 'contains': 'denied'},
            },
            'events': [
              {'type': 'text', 'text': 'Understood, I left it.'},
            ],
          },
          {
            'expect': {'userContains': 'rename'},
            'events': [edit('c3', 'fn a() {}', 'fn z() {}')],
          },
          {
            'expect': {
              'toolResult': {'callId': 'c3', 'isError': false},
            },
            'events': [
              {'type': 'text', 'text': 'Renamed.'},
            ],
          },
        ],
      }),
    );
    final app = await boot(
      tester,
      data.path,
      env: {
        'AXIOM_TEST_SCRIPTED_MODEL': script,
        'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json',
      },
    );
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    await app.documents.open('src/lib.rs');
    await tester.pump(const Duration(milliseconds: 200));

    // 1. Mod+L opens the dock beside the editor, in the folder's scope.
    await key(tester, LogicalKeyboardKey.keyL, mod: true);
    final dock = find.byWidgetPredicate((w) => w is ChatView && w.dock);
    await until(tester, () => dock.evaluate().isNotEmpty, what: 'the dock');
    expect(named('AI dock'), findsOneWidget);
    await until(tester, () => app.agent.status == 'ready', what: 'the providers');
    expect(app.agent.scope, Scope.workspace);
    final view = tester.state<ChatViewState>(dock);
    await until(tester, () => view.composerFocus.hasFocus, what: 'the dock composer focused');

    // 2. An edit asks: the card with the diff, focused; the rail shows it waiting.
    await _send(tester, 'make it public');
    await until(tester, () => find.byType(ApprovalCard).evaluate().isNotEmpty, what: 'a card');
    expect(find.byType(DiffBlock), findsOneWidget);
    expect(named('Chat, 1 waiting for you'), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 100));
    // Enter on the focused Allow once.
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => lib.readAsStringSync() == 'pub fn a() {}\n',
      what: 'the first edit applied',
    );
    // 3. The next edit asks: Esc denies it.
    await until(
      tester,
      () => find.byType(ApprovalCard).evaluate().isNotEmpty && app.agent.approvals.isNotEmpty,
      what: 'the second card',
    );
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.escape);
    await until(tester, () => _assistant(app).contains('Understood'), what: 'the denial');
    await _idle(tester, app);
    expect(lib.readAsStringSync(), 'pub fn a() {}\n');
    expect(named('Chat'), findsOneWidget, reason: 'nothing waits any more');

    // 4. Restore puts the file back and says so.
    await tester.tap(named('Restore').first);
    await until(
      tester,
      () => _items(app).whereType<NoticeItem>().any((n) => n.message.startsWith('Restored')),
      what: 'the restore note',
    );
    expect(lib.readAsStringSync(), 'fn a() {}\n');

    // 5. The agent changes it again, then the person does: Restore is a conflict, nothing is
    // restored, the person's text stays.
    await _send(tester, 'rename it');
    await until(tester, () => app.agent.approvals.isNotEmpty, what: 'the rename card');
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(named('Allow once'));
    await until(tester, () => _assistant(app).contains('Renamed.'), what: 'the rename');
    await _idle(tester, app);
    expect(lib.readAsStringSync(), 'fn z() {}\n');
    lib.writeAsStringSync('fn mine() {}\n');
    await tester.tap(named('Restore').last);
    await until(tester, () => app.agent.restoreConflict != null, what: 'the conflict');
    expect(find.textContaining('src/lib.rs — edited after the agent'), findsOneWidget);
    await tester.tap(named('Cancel'));
    await tester.pump(const Duration(milliseconds: 100));
    expect(app.agent.restoreConflict, isNull);
    expect(lib.readAsStringSync(), 'fn mine() {}\n');

    // 6. Mod+L closes the dock.
    await key(tester, LogicalKeyboardKey.keyL, mod: true);
    await until(tester, () => dock.evaluate().isEmpty, what: 'the dock closed');

    await quit(tester, app);
    semantics.dispose();
  });

  testWidgets('first-run setup, providers, installing an agent, an agent without a folder', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-models-data-');
    final bin = tempDir('axiom-fl-models-bin-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      bin.deleteSync(recursive: true);
    });
    const secret = 'sk-e2e-not-a-real-key-4242';

    // A fake OpenAI-compatible server on loopback.
    final auth = <String>[];
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    server.listen((req) async {
      await req.drain<void>();
      auth.add(req.headers.value('authorization') ?? '');
      final res = req.response;
      if (req.method == 'GET' && req.uri.path == '/v1/models') {
        res.headers.contentType = ContentType.json;
        res.write(
          jsonEncode({
            'object': 'list',
            'data': [
              {'id': 'fake-gpt', 'object': 'model'},
            ],
          }),
        );
      } else if (req.method == 'POST' && req.uri.path == '/v1/chat/completions') {
        res.headers.contentType = ContentType('text', 'event-stream');
        String chunk(Map<String, Object?> delta, [String? finish]) =>
            'data: ${jsonEncode({
              'id': 'c1',
              'object': 'chat.completion.chunk',
              'choices': [
                {'index': 0, 'delta': delta, 'finish_reason': finish},
              ],
            })}\n\n';
        res.write(chunk({'role': 'assistant', 'content': ''}));
        for (final w in ['Hello ', 'from ', 'the ', 'fake ', 'server.']) {
          res.write(chunk({'content': w}));
          await res.flush();
        }
        res.write(chunk({}, 'stop'));
        res.write('data: [DONE]\n\n');
      } else {
        res.statusCode = 404;
      }
      await res.close();
    });
    final base = 'http://127.0.0.1:${server.port}/v1';

    // A fake npm: `install --no-fund --no-audit --prefix <dir> <package>` puts the agent's
    // program in <dir>/node_modules/.bin (as the daemon test does).
    final npm = File('${bin.path}/npm')
      ..writeAsStringSync(
        '#!/bin/sh\nwhile [ "\$1" != "--prefix" ]; do shift; done\nprefix="\$2"; pkg="\$3"\n'
        // Codex's install hangs (for Cancel).
        'case "\$pkg" in *codex*) exec sleep 60;; esac\n'
        'echo "\$pkg" > "\$prefix/installed.txt"\nmkdir -p "\$prefix/node_modules/.bin"\n'
        "printf '#!/bin/sh\\n' > \"\$prefix/node_modules/.bin/claude-agent-acp\"\n"
        'chmod +x "\$prefix/node_modules/.bin/claude-agent-acp"\n',
      );
    expect((await Process.run('chmod', ['+x', npm.path])).exitCode, 0);

    final app = await boot(
      tester,
      data.path,
      env: {
        'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json',
        // No real agent programs: the fake npm, and the system tools.
        'PATH': '${bin.path}:/usr/bin:/bin',
        'FAKE_ACP_LOG': '${data.path}/fake-acp.log',
      },
    );

    // 1. Nothing configured: Chat shows the first-run setup.
    await tester.tap(named('Chat'));
    await until(tester, () => app.agent.status == 'ready', what: 'the providers');
    expect(find.byType(ModelSetup), findsOneWidget);
    expect(app.agent.providers.where((p) => p.usable), isEmpty);

    // 2. An OpenAI-compatible server with a key: added and chosen; the key leaves the field.
    // The setup page scrolls, and what is above the tabs (the agents found) differs by
    // computer: each control is brought into view before it is clicked.
    await tester.ensureVisible(named('OpenAI-compatible'));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(named('OpenAI-compatible'));
    await tester.pump(const Duration(milliseconds: 100));
    Finder field(String label) => find.descendant(
      of: find.byWidgetPredicate((w) => w is AxTextInput && w.label == label),
      matching: find.byType(EditableText),
    );
    // Clicked, then typed (the text goes to the field the click focused).
    Future<void> typeInto(Finder f, String text) async {
      await tester.ensureVisible(f);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.tap(f);
      await tester.pump(const Duration(milliseconds: 50));
      final focused = tester.state<EditableTextState>(f).widget.focusNode.hasFocus;
      expect(focused, isTrue, reason: 'the click focused the field');
      await typeFocused(tester, text);
    }

    await typeInto(field('Server address'), base);
    await typeInto(field('API key'), secret);
    expect(
      (field('Server address').evaluate().single.widget as EditableText).controller.text,
      base,
    );

    // The setup page scrolls: the button may be below the fold.
    await tester.ensureVisible(named('Add provider'));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(named('Add provider'));
    await until(
      tester,
      () => app.agent.providers.any((p) => p.baseUrl == base && p.usable),
      what: 'the provider added',
      detail: () =>
          '${[for (final p in app.agent.providers) '${p.providerId} ${p.baseUrl} ${p.error}']}; '
          'the form says: ${find.descendant(of: find.byType(AddProvider), matching: find.byType(RichText)).evaluate().map((e) => (e.widget as RichText).text.toPlainText()).join(' | ')}',
    );
    final added = app.agent.providers.firstWhere((p) => p.baseUrl == base);
    expect(app.agent.model?.providerId, added.providerId);
    expect(added.hasKey, isTrue);
    bool secretShown() => find
        .byWidgetPredicate(
          (w) =>
              (w is EditableText && w.controller.text.contains(secret)) ||
              (w is RichText && w.text.toPlainText().contains(secret)),
        )
        .evaluate()
        .isNotEmpty;
    expect(secretShown(), isFalse, reason: 'the key is nowhere in the UI');

    // 3. A conversation with it streams its answer; the key went to the server as a bearer.
    final view = tester.state<ChatViewState>(find.byType(ChatView));
    view.composerFocus.requestFocus();
    await tester.pump();
    await _send(tester, 'hi');
    await until(tester, () => _assistant(app).contains('Hello from the fake server.'), what: 'it');
    await _idle(tester, app);
    expect(auth, contains('Bearer $secret'));

    // 4. Settings → Models lists it with its key stored and its model.
    app.openSettings!('models');
    await until(
      tester,
      () => named('Remove ${added.name}').evaluate().isNotEmpty,
      what: 'Models, with the provider',
    );
    expect(find.textContaining('Key stored · 1 model'), findsOneWidget);

    // 5. Removing it brings the setup back (in a new chat).
    await tester.tap(named('Remove ${added.name}'));
    await until(
      tester,
      () => app.agent.providers.every((p) => p.providerId != added.providerId),
      what: 'the removal',
    );
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 300));
    app.agent.newChat();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byType(ModelSetup), findsOneWidget);
    expect(secretShown(), isFalse);

    // 6. Claude Code is installed from the setup into the data directory.
    expect(named('Install Claude Code'), findsOneWidget);
    await tester.tap(named('Install Claude Code'));
    await until(
      tester,
      () => app.agent.providers.any((p) => p.providerId == 'claude-code' && p.usable),
      timeout: const Duration(seconds: 60),
      what: 'Claude Code installed',
    );
    expect(
      File('${data.path}/agents/installed.txt').readAsStringSync().trim(),
      '@agentclientprotocol/claude-agent-acp',
    );
    expect(app.agent.model?.providerId, 'claude-code');

    // 6b. In Settings → Models, an install can be cancelled: npm is stopped, nothing installed.
    app.openSettings!('models');
    await until(tester, () => named('Install Codex').evaluate().isNotEmpty, what: 'Models');
    await tester.tap(named('Install Codex'));
    await until(
      tester,
      () => named('Cancel installing Codex').evaluate().isNotEmpty,
      what: 'the install running',
      detail: () =>
          'installing ${app.agent.installing}; buttons '
          '${find.byWidgetPredicate((w) => w is AxButton).evaluate().map((e) => (e.widget as AxButton).label).toList()}',
    );
    final started = DateTime.now();
    await tester.tap(named('Cancel installing Codex'));
    await until(
      tester,
      () => !app.agent.installing.contains('codex'),
      timeout: const Duration(seconds: 10),
      what: 'the install cancelled',
    );
    expect(DateTime.now().difference(started).inSeconds, lessThan(10));
    expect(app.agent.providers.firstWhere((p) => p.providerId == 'codex').usable, isFalse);
    expect(find.textContaining('Codex was not installed: Cancelled.'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 300));

    // 7. The fake ACP agent, chosen in the picker, answers with no folder open: it runs in a
    // scratch folder under the data directory, and its write waits for Allow.
    final err = await app.agent.configure(
      ProviderConfig(
        id: 'fake-agent',
        kind: ProviderKind.acp,
        name: 'Fake Agent',
        command: [_fakeAcp()],
        env: {'FAKE_ACP_LOG': '${data.path}/fake-acp.log'},
      ),
    );
    expect(err, '');
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(
      find.descendant(of: find.byType(ModelPicker), matching: find.byType(RichText)).first,
    );
    await tester.pump(const Duration(milliseconds: 300));
    await tester.tap(find.text('Fake Agent').last);
    await tester.pump(const Duration(milliseconds: 300));
    expect(app.agent.model?.providerId, 'fake-agent');
    view.composerFocus.requestFocus();
    await tester.pump();
    await _send(tester, 'write the notes');
    await until(tester, () => app.agent.approvals.isNotEmpty, what: 'the agent asking');
    await until(tester, () => find.byType(ApprovalCard).evaluate().isNotEmpty, what: 'the card');
    expect(
      find.textContaining('Fake Agent is an external agent'),
      findsOneWidget,
      reason: 'the card says the agent is external',
    );
    final id = app.agent.activeId!;
    final notes = File('${data.path}/scratch/$id/notes.md');
    expect(notes.existsSync(), isFalse, reason: 'nothing is written before Allow');
    await tester.tap(named('Allow once'));
    await until(tester, () => _assistant(app).contains('wrote notes.md'), what: 'the write');
    await _idle(tester, app);
    expect(notes.readAsStringSync(), 'hello\n');

    await quit(tester, app);
    semantics.dispose();
  });
}
