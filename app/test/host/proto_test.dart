import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final lines = File('../docs/protocol/fixtures.jsonl').readAsLinesSync();
  final head = json.decode(lines.first) as Json;
  final messages = [for (final l in lines.skip(1)) Message.decode(json.decode(l) as Json)];

  test('the client knows every type the host can send', () {
    expect(head['protocol'], protocol);
    expect((head['types']! as List<Object?>).toSet(), Body.types);
    final events = (messages[0] as Events).events;
    expect(events.map((e) => e.seq), [for (var i = 1; i <= Body.types.length; i++) i]);
    expect(events.where((e) => e.body is UnknownBody), isEmpty);
  });

  test('every field arrives', () {
    final b = {for (final e in (messages[0] as Events).events) e.body.runtimeType: e.body};
    final added = b[AgentAdded]! as AgentAdded;
    expect([added.name, added.parent, added.apart], ['Lead', 'a_0', true]);
    final from = added.from!;
    expect([from.agent, from.item, from.kept], ['a_0', 'i_0', true]);
    final x = added.execution;
    expect(
      [x.harness, x.provider, x.model, x.connection, x.effort, x.environment],
      ['axiom', 'anthropic', 'm', 'c_2', 'high', 'e'],
    );
    expect((b[AgentStateChanged]! as AgentStateChanged).state, AgentPhase.asking);
    final ended = b[RunEnded]! as RunEnded;
    expect([ended.outcome, ended.error], [Outcome.failed, 'why']);
    final tool = b[ItemStarted]! as ItemStarted;
    expect([tool.kind, tool.name, (tool.args! as Json)['path']], [ItemKind.tool, 'read', 'x']);
    expect((b[ItemDelta]! as ItemDelta).text, 'tö\n"k"');
    final done = b[ItemEnded]! as ItemEnded;
    expect([done.outcome, (done.result! as Json)['lines']], [Outcome.interrupted, 3]);
    expect((b[RequestOpened]! as RequestOpened).answerable, Answerable.live);
    final u = b[UsageReported]! as UsageReported;
    expect(
      [u.usage.input, u.usage.cacheRead, u.usage.cacheWrite, u.usage.output, u.usage.reasoning],
      [1, 2, 3, 4, 5],
    );
    expect([u.usage.context, u.provenance], [.5, Known.estimated]);
    expect((messages[1] as Resync).last, 9);
    final s = SessionInfo.decode(
      ((messages[2] as Answered).result! as List<Object?>).first! as Json,
    );
    expect([s.id, s.lastSeq, s.running], ['s_1', 3, true]);
    expect((messages[3] as Failed).code, 'not_found');
    expect((messages[4] as Fault).words, 'cannot be read');
    expect((messages.last as FolderOpened).path, '/w');
    final status = (messages[5] as Answered).result! as Json;
    expect([status['damaged'], (status['faults']! as List<Object?>).length], ['why', 1]);
    final cut = (messages[6] as Events).events;
    final begun = cut[0].body as ItemStarted, over = cut[1].body as ItemEnded;
    expect([begun.args, begun.argsCut, tool.argsCut], [null, 300000, 0]);
    expect([over.result, over.resultCut, done.resultCut], [null, 400000, 0]);
  });

  test('the limits of the host arrive, with words for how much', () {
    final all = [
      for (final j in (messages[7] as Answered).result! as List<Object?>)
        HostLimit.decode(j! as Json),
    ];
    expect(
      [for (final l in all) (l.of, l.what, l.amount)],
      [
        ('A run', 'Steps', '200'),
        ('A run', 'Output tokens', 'No limit'),
        ('Files', 'One file', '1 KiB'),
        ('Files', 'One command', '10 minutes'),
      ],
    );
    for (final (most, unit, want) in [
      (0, 'bytes', '0'),
      (999, 'count', '999'),
      (1000, 'count', '1\u{a0}000'),
      (1234567, 'parsecs', '1\u{a0}234\u{a0}567'),
      (500, 'bytes', '500 B'),
      (1536, 'bytes', '1536 B'),
      (8 << 20, 'bytes', '8 MiB'),
      (16 << 30, 'bytes', '16 GiB'),
      (1, 'seconds', '1 second'),
      (90, 'seconds', '90 seconds'),
      (3600, 'seconds', '1 hour'),
      (4 * 3600, 'seconds', '4 hours'),
    ]) {
      final l = HostLimit.decode({'of': 'x', 'what': 'y', 'most': most, 'unit': unit});
      expect(l.amount, want);
    }
  });

  test('what is newer or malformed keeps its place', () {
    for (final j in [
      {'type': 'x.new', 'a': 1},
      {'type': 'item.delta', 'item': 'i'},
      {'type': 'agent.added', 'agent': 3},
    ]) {
      expect(Event.decode({'seq': 4, 'at': 1, 'body': j}).body, isA<UnknownBody>());
    }
    final odd = Body.decode({'type': 'agent.state', 'agent': 'a', 'state': 'dozing'});
    expect((odd as AgentStateChanged).state, AgentPhase.unknown);
  });

  test('lines survive any cutting', () {
    final text = [for (var i = 0; i < 50; i++) 'line $i é🙂 ${'x' * (i * 7)}'];
    final bytes = utf8.encode('${text.join('\n')}\n');
    final r = Random(3);
    for (var n = 0; n < 50; n++) {
      final s = LineSplitterBytes();
      final got = <String>[];
      var at = 0;
      while (at < bytes.length) {
        final k = min(1 + r.nextInt(90), bytes.length - at);
        got.addAll(s.add(Uint8List.sublistView(bytes, at, at + k)).map(utf8.decode));
        at += k;
      }
      expect(got, text);
    }
  });

  test('an endless line is refused', () {
    final s = LineSplitterBytes();
    expect(() {
      for (var i = 0; i < 200; i++) {
        s.add(Uint8List(65536)..fillRange(0, 65536, 65)).toList();
      }
    }, throwsFormatException);
  });

  group('session model', () {
    Event e(int seq, Body b) => Event(seq, seq, b);
    const x = Execution(harness: 'h');
    List<Event> start() => [
      e(1, const SessionCreated('T', null)),
      e(2, const AgentAdded('a', 'A', x, null)),
      e(3, const ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
    ];

    test('deltas then the end give the authoritative text once', () {
      final m = SessionModel('s')..apply(start());
      m.apply([e(4, const ItemDelta('i', 'one\n\n')), e(5, const ItemDelta('i', 'tw'))]);
      final item = m.agents['a']!.items.single;
      expect([item.doc.closed.single.text, item.doc.tail.single.text], ['one', 'tw']);
      m.apply([e(6, const ItemEnded('i', Outcome.completed, 'one\n\ntw', null))]);
      expect(item.doc.closed.map((b) => b.text), ['one', 'tw']);
      expect(item.open, isFalse);
    });

    test('folded history, without deltas, gives the same', () {
      final m = SessionModel('s')
        ..apply([...start(), e(6, const ItemEnded('i', Outcome.completed, 'one\n\ntw', null))]);
      expect(m.agents['a']!.items.single.doc.closed.map((b) => b.text), ['one', 'tw']);
    });

    test('a resync after missing deltas is repaired by the end', () {
      final m = SessionModel('s')..apply(start());
      m.apply([e(4, const ItemDelta('i', 'one'))]);
      m.apply([e(9, const ItemEnded('i', Outcome.completed, 'one and two', null))]);
      expect(m.agents['a']!.items.single.doc.closed.single.text, 'one and two');
    });

    test('repeated events change nothing', () {
      final m = SessionModel('s')..apply(start());
      final d = [e(4, const ItemDelta('i', 'x'))];
      m
        ..apply(d)
        ..apply(d)
        ..apply(start());
      expect(m.agents['a']!.items.single.doc.tail.single.text, 'x');
      expect(m.agents.length, 1);
    });

    test('the end is authoritative: shorter, or the same length with other words', () {
      for (final (streamed, text) in [('a\n\nb\n\nc\n\nd', 'a\n\nb'), ('aaaa', 'bbbb')]) {
        final m = SessionModel('s')..apply(start());
        final i = m.agents['a']!.items.single;
        m.apply([e(4, ItemDelta('i', streamed))]);
        final before = i.doc.revision;
        m.apply([e(5, ItemEnded('i', Outcome.completed, text, null))]);
        expect(i.doc.closed.map((b) => b.text).join('\n\n'), text);
        expect(i.doc.revision, greaterThan(before));
      }
    });

    test('an agent does not run after its run has ended', () {
      final m = SessionModel('s')
        ..apply([
          ...start(),
          e(4, const RunStarted('r', 'a')),
          e(5, const AgentStateChanged('a', AgentPhase.running)),
          e(6, const RunEnded('r', Outcome.failed, '429 from the provider')),
        ]);
      final a = m.agents['a']!;
      expect(
        [a.phase, a.ended, a.error],
        [AgentPhase.failed, Outcome.failed, '429 from the provider'],
      );
    });

    test('names cannot hide or reorder anything', () {
      final rlo = String.fromCharCode(0x202E);
      final m = SessionModel('s')
        ..apply([
          e(1, SessionCreated('a${rlo}b\nc', null)),
          e(2, AgentAdded('a', 'x${rlo}y', x, null)),
        ]);
      expect(m.title, 'a\u{FFFD}b c');
      expect(m.agents['a']!.name, 'x\u{FFFD}y');
    });

    test('a batch notifies each listener once', () {
      final m = SessionModel('s')..apply(start());
      var items = 0, sessions = 0;
      m.addListener(() => sessions++);
      m.agents['a']!.items.single.addListener(() => items++);
      m.apply([for (var i = 4; i < 104; i++) e(i, const ItemDelta('i', 'x '))]);
      expect([items, sessions], [1, 0]);
    });

    test('events for things never seen are counted, not fatal', () {
      final m = SessionModel('s')
        ..apply([
          e(1, const ItemStarted('i', 'r', 'ghost', ItemKind.prose, '', null)),
          e(2, const ItemDelta('i', 'x')),
          e(3, const ItemEnded('zz', Outcome.completed, '', null)),
          e(4, const RequestClosed('q', 'x')),
          e(5, const UnknownBody('new')),
        ]);
      expect([m.seq, m.unknown], [5, 2]);
    });
  });
}
