// The transcript built from a session's events (the Tauri store's tests, ported), and the store
// across a daemon's loss and restart, against the real daemon with its scripted model (AUD-025,
// AUD-092; run `cargo build -p axiomd` first).

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/agent.dart';
import 'package:axiom_desktop/state/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

import 'documents_safety_test.dart' show daemon, until;

AgentEvent ev(int seq, int turn, EventBody b) =>
    AgentEvent(sessionId: 's', seq: seq, turn: turn, tsMs: seq, event: b);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('user, assistant, a tool call and its result, a checkpoint and a notice', () {
    final items = transcript([
      ev(1, 1, const EventBodyUser(text: 'hi')),
      ev(
        2,
        1,
        const EventBodyCheckpoint(
          checkpoint: CheckpointInfo(number: 1, label: 'Before turn 1', createdMs: 5, turn: 1),
        ),
      ),
      ev(
        3,
        1,
        const EventBodyToolCall(
          callId: 'c1',
          tool: 'workspace.writeFile',
          effect: Effect2.writeWorkspace,
          targets: ['a.rs'],
        ),
      ),
      ev(4, 1, const EventBodyToolResult(callId: 'c1', isError: false, summary: 'Wrote a.rs')),
      ev(5, 1, const EventBodyAssistantText(text: 'Done.')),
      ev(6, 1, const EventBodyError(kind: 'notice', message: 'A notice')),
      ev(7, 1, const EventBodyTurnEnd(reason: TurnEndReason.endTurn)),
    ]);
    expect(items.map((i) => i.runtimeType.toString()), [
      'UserItem',
      'CheckpointItem',
      'ToolItem',
      'AssistantItem',
      'NoticeItem',
    ]);
    final t = items[2] as ToolItem;
    expect((t.state, t.summary), (ToolState.ok, 'Wrote a.rs'));
    expect(t.targets, ['a.rs']);
    expect((items[4] as NoticeItem).notice, isTrue);
  });

  test('a rewind drops the turns from its turn on (edit, regenerate)', () {
    final items = transcript([
      ev(1, 1, const EventBodyUser(text: 'one')),
      ev(2, 1, const EventBodyAssistantText(text: 'a1')),
      ev(3, 2, const EventBodyUser(text: 'two')),
      ev(4, 2, const EventBodyAssistantText(text: 'a2')),
      ev(5, 2, const EventBodyRewind(fromTurn: 2)),
      ev(6, 2, const EventBodyUser(text: 'two, edited')),
    ]);
    expect(
      [
        for (final i in items)
          if (i is UserItem) i.text,
      ],
      ['one', 'two, edited'],
    );
    expect(lastUser(items)!.text, 'two, edited');
  });

  test('an interrupted tool call shows as an error', () {
    final items = transcript([
      ev(1, 1, const EventBodyToolCall(callId: 'c', tool: 'shell.run', effect: Effect2.execute)),
      ev(2, 1, const EventBodyInterrupted(callId: 'c')),
    ]);
    expect((items.single as ToolItem).state, ToolState.error);
  });
  test('an interrupted turn is marked where it stopped, with what was streamed of it', () {
    final events = [
      ev(1, 1, const EventBodyUser(text: 'hi')),
      ev(2, 1, const EventBodyAssistantText(text: 'Part one.')),
      ev(3, 1, const EventBodyTurnEnd(reason: TurnEndReason.error)), // the daemon's repair
      ev(4, 2, const EventBodyUser(text: 'again')),
    ];
    final items = transcript(events, interrupted: (seq: 2, turn: 1, text: 'and part'));
    expect(items.map((i) => i.runtimeType.toString()), [
      'UserItem',
      'AssistantItem',
      'AssistantItem',
      'NoticeItem',
      'UserItem',
    ]);
    expect((items[2] as AssistantItem).text, 'and part');
    final n = items[3] as NoticeItem;
    expect((n.message, n.notice), (interruptedMessage, false));
    // A rewind of that turn takes the mark with it.
    final rewound = transcript(
      [...events, ev(5, 1, const EventBodyRewind(fromTurn: 1))],
      interrupted: (seq: 2, turn: 1, text: ''),
    );
    expect(rewound.whereType<NoticeItem>(), isEmpty);
  });

  group('across a daemon loss (real daemon, scripted model)', () {
    late Directory data, folder;
    late String script;
    late DaemonClient client;
    late WorkspaceStore ws;
    late AgentStore agent;

    void writeScript(List<Map<String, Object?>> turns) =>
        File(script).writeAsStringSync(jsonEncode({'turns': turns}));

    setUp(() async {
      data = Directory.systemTemp.createTempSync('axiom-fl-agent-');
      folder = Directory.systemTemp.createTempSync('axiom-fl-agent-ws-');
      script = '${data.path}/script.json';
      writeScript([
        {
          'events': [
            {'type': 'text', 'text': 'partial words'},
            {'type': 'hang'},
          ],
        },
      ]);
      client = DaemonClient(
        DaemonConfig(
          program: daemon,
          args: ['--data-dir', '${data.path}/data'],
          environment: {
            'AXIOM_TEST_SCRIPTED_MODEL': script,
            'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json',
          },
        ),
      );
      expect(await client.start(), isA<DaemonReady>());
      ws = WorkspaceStore(client);
      agent = AgentStore(client, ws);
    });
    tearDown(() async {
      await client.shutdown();
      data.deleteSync(recursive: true);
      folder.deleteSync(recursive: true);
    });

    Future<void> kill() async {
      Process.killPid(client.pid!, ProcessSignal.sigkill);
      await until(() => client.state.value is DaemonFailed, 'the daemon gone');
    }

    test(
      'a turn cut off is marked interrupted, stops "writing", and Send works after a restart',
      () async {
        await agent.load();
        expect(agent.model?.providerId, 'scripted');
        expect(await agent.send('hello'), isTrue);
        await until(
          () => agent.active.events.any((e) => e.event is EventBodyUser),
          'the turn started',
        );
        expect(agent.active.running, isTrue);
        await Future<void>.delayed(
          const Duration(milliseconds: 500),
        ); // the text streams, then hangs
        await kill();
        expect(agent.active.running, isFalse);
        expect(agent.sessions.where((s) => s.running), isEmpty);
        List<String> shown() => [
          for (final i in agent.items())
            switch (i) {
              UserItem(:final text) => 'user:$text',
              AssistantItem(:final text) => 'assistant:$text',
              NoticeItem(:final message, :final notice) =>
                '${notice ? 'notice' : 'error'}:$message',
              _ => i.runtimeType.toString(),
            },
        ];
        expect(shown(), ['user:hello', 'assistant:partial words', 'error:$interruptedMessage']);

        // Back: the conversation is read again (the daemon ends the cut-off turn); the mark stays.
        final before = agent.active;
        writeScript([
          {
            'events': [
              {'type': 'text', 'text': 'second answer'},
            ],
          },
        ]);
        expect(await client.restart(), isA<DaemonReady>());
        await until(() => !identical(agent.active, before), 'read again');
        expect(agent.sendError, isEmpty);
        expect(agent.active.events.last.event, isA<EventBodyTurnEnd>());
        expect(shown(), ['user:hello', 'assistant:partial words', 'error:$interruptedMessage']);
        expect(await agent.send('again'), isTrue);
        await until(
          () => !agent.active.running && agent.items().last is AssistantItem,
          'the second answer',
        );
        expect(shown(), [
          'user:hello',
          'assistant:partial words',
          'error:$interruptedMessage',
          'user:again',
          'assistant:second answer',
        ]);
      },
    );

    test(
      "a daemon restart keeps Chat in the folder's scope and reads its conversation again",
      () async {
        writeScript([
          {
            'events': [
              {'type': 'text', 'text': 'answer'},
            ],
          },
        ]);
        expect(await client.restart(), isA<DaemonReady>());
        await ws.openFolder(folder.path);
        agent.setScope(Scope.workspace);
        await until(() => agent.status == 'ready', 'loaded');
        expect(await agent.send('hello'), isTrue);
        await until(
          () => agent.active.events.any((e) => e.event is EventBodyTurnEnd),
          'the answer',
        );
        final id = agent.activeId;
        final before = agent.active;
        await kill();
        expect(agent.scope, Scope.workspace, reason: 'daemon loss switched Chat to General');
        expect(agent.activeId, id);
        // AppState.restartBackend: the daemon, then the folder that was open.
        expect(await client.restart(), isA<DaemonReady>());
        await ws.openFolder(ws.lostRoot);
        await until(() => !identical(agent.active, before), 'read again');
        expect(agent.scope, Scope.workspace);
        expect(agent.activeId, id);
        expect(agent.sendError, isEmpty, reason: 'read in the wrong scope');
        await until(() => agent.sessions.any((s) => s.sessionId == id), "the folder's sessions");
      },
    );

    test('switching folders keeps Chat in the folder scope (AUD-092)', () async {
      final other = Directory.systemTemp.createTempSync('axiom-fl-agent-ws2-');
      addTearDown(() => other.deleteSync(recursive: true));
      await ws.openFolder(folder.path);
      agent.setScope(Scope.workspace);
      await until(() => agent.status == 'ready', 'loaded');
      await ws.openFolder(other.path);
      expect(ws.isOpen, isTrue);
      expect(agent.scope, Scope.workspace);
    });
  });
}
