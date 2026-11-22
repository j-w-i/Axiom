// The transcript built from a session's events (the Tauri store's tests, ported).

import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/agent.dart';
import 'package:flutter_test/flutter_test.dart';

AgentEvent ev(int seq, int turn, EventBody b) =>
    AgentEvent(sessionId: 's', seq: seq, turn: turn, tsMs: seq, event: b);

void main() {
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
}
