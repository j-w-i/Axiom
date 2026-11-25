import 'dart:async';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/context_gauge.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/patterns/models.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/shell/drafts.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/start.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

/// Says what it was asked, and answers as it is told. The real host: test/host/e2e_test.dart.
class Asked extends HostStore {
  Asked() : super(paths: HostPaths('/nonexistent'), binary: null, spawn: false) {
    phase = HostPhase.connected;
  }
  final calls = <List<Object?>>[];
  Object? failure;
  List<ModelInfo> known = const [ModelInfo('claude-opus-5-5'), ModelInfo('gpt-5.6')];
  final _seen = <String, List<ModelInfo>>{};

  /// Only this fails, if it is not null.
  String? failsAt;

  /// What [failsAt] or everything waits for.
  Completer<void>? gate;

  Future<T> _do<T>(List<Object?> call, T answer) async {
    calls.add(call);
    final here = failsAt == null || failsAt == call.first;
    if (here) await gate?.future;
    if (failure case final f? when here) throw f;
    return answer;
  }

  @override
  Future<String> create(String title, {String? workspace}) =>
      _do(['create', title, workspace], 's_new');
  @override
  Future<String> addAgent(
    String session,
    String name,
    Execution x,
    Access access, {
    List<Instructions> instructions = const [],
    bool trust = false,
  }) => _do([
    'add',
    session,
    name,
    if (x.harness != 'axiom') (x.harness, trust) else x.connection,
    if (x.harness != 'axiom' && x.connection != null) x.connection,
    x.model,
    access,
    for (final i in instructions) i.digest,
  ], 'a_new');
  Map<String, List<Instructions>> words = const {};
  @override
  Future<List<Instructions>> look(String path) async =>
      words[path] ?? (throw const Failed(1, 'bad', 'x'));
  Changes now = const Changes();
  @override
  Future<Changes> changes(String session) => _do(['changes', session], now);
  @override
  Future<void> open(String id) => _do(['open', id], null);
  @override
  Future<void> prompt(String session, String agent, String text, {String? key}) {
    keys.add(key);
    return _do(['prompt', session, agent, text], null);
  }

  /// Of each prompt.
  final keys = <String?>[];
  @override
  Future<void> answer(String session, RequestOpened r, Answer a) =>
      _do(['answer', session, r.request, r.digest, a], null);
  @override
  Future<void> cancel(String session, String run) => _do(['cancel', session, run], null);
  @override
  Future<void> setAgent(String session, String agent, {Execution? execution, Access? access}) =>
      _do(['set', session, agent, execution?.model, execution?.connection, access], null);
  @override
  Future<String> connect(String name, String provider, {String? url, String? secret}) async {
    final id = await _do(['connect', name, provider, url, secret], 'c_new');
    connections = [
      ...connections,
      Connection(id, name, provider, url: url, secret: secret != null),
    ];
    notifyListeners();
    return id;
  }

  @override
  Future<void> disconnect(String id) async {
    await _do(['disconnect', id], null);
    connections = connections.where((c) => c.id != id).toList();
    notifyListeners();
  }

  @override
  List<ModelInfo>? modelsOf(String connection) => _seen[connection];
  @override
  Future<List<ModelInfo>> models(String connection) async {
    final r = await _do(['models', connection], known);
    _seen[connection] = r;
    notifyListeners();
    return r;
  }
}

const key = Connection('c1', 'Mine', 'anthropic', secret: true);
const x = Execution(
  harness: 'axiom',
  provider: 'anthropic',
  model: 'claude-opus-5-5',
  connection: 'c1',
);

SessionModel working({Access? access = Access.ask, bool asks = false}) {
  var n = 0;
  Event e(Body b) => Event(++n, n, b);
  return SessionModel('s')..apply([
    e(const SessionCreated('Fix the cache', '/w')),
    e(AgentAdded('a', 'Lead', x, null, access: access)),
    e(const RunStarted('r', 'a')),
    e(const AgentStateChanged('a', AgentPhase.running)),
    e(const ItemStarted('p', 'r', 'a', ItemKind.prompt, '', null)),
    e(const ItemEnded('p', Outcome.completed, 'Make the cache forget.', null)),
    e(const ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
    e(const ItemEnded('i', Outcome.completed, 'I will run the tests first.', null)),
    if (asks) ...[
      e(const ItemStarted('t', 'r', 'a', ItemKind.tool, 'shell', {'command': 'cargo test'})),
      for (final (q, s) in [('q1', 'cargo test'), ('q2', 'cargo bench')])
        e(
          RequestOpened(
            q,
            'a',
            'asks to run a command',
            s,
            'in .; writes the workspace; no network',
            Answerable.live,
            digest: 'd-$q',
            item: 't',
            session: true,
          ),
        ),
      e(const AgentStateChanged('a', AgentPhase.asking)),
    ],
  ]);
}

Future<void> settled(WidgetTester t) async {
  await t.pump(const Duration(milliseconds: 100));
  await t.pump(const Duration(milliseconds: 700));
}

void main() {
  Future<Asked> shell(WidgetTester t, {SessionModel? session, bool accounts = true}) async {
    final s = Asked()
      ..current = session
      ..connections = accounts ? const [key, Connection('c2', 'Here', 'lm-studio')] : const [];
    addTearDown(s.dispose);
    await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
    return s;
  }

  testWidgets('work is begun: what, where, who, and how much it may do', (t) async {
    final s = await shell(t);
    await t.tap(find.bySemanticsLabel('New session').first);
    await settled(t);
    expect(s.calls, [
      ['models', 'c1'],
    ]);
    AxButton start() => t.widget<AxButton>(find.widgetWithText(AxButton, 'Start'));
    expect(start().onPressed, isNull, reason: 'nothing is filled in');
    Future<void> write(String label, String text) => t.enterText(
      find.descendant(
        of: find.byWidgetPredicate((w) => w is AxField && w.label == label),
        matching: find.byType(EditableText),
      ),
      text,
    );
    await write('What should be done', 'Make the cache forget.\nAll of it.');
    await write('Folder', '/home/me/project');
    await settled(t);
    expect(start().onPressed, isNull, reason: 'no model');
    await t.tap(find.bySemanticsLabel('Model: none'));
    await settled(t);
    await t.tap(find.bySemanticsLabel('gpt-5.6').last);
    await settled(t);
    await t.ensureVisible(find.text('Start'));
    await snap(t, 'start');
    await t.tap(find.text('Start'));
    await settled(t);
    expect(s.calls.skip(1), [
      ['create', 'Make the cache forget. All of it.', '/home/me/project'],
      ['add', 's_new', 'Lead', 'c1', 'gpt-5.6', Access.ask],
      ['open', 's_new'],
      ['prompt', 's_new', 'a_new', 'Make the cache forget.\nAll of it.'],
    ]);
    expect(find.byType(StartPage), findsNothing);
  });

  testWidgets('a project\'s words for agents are followed only if a person says so', (t) async {
    final s = await shell(t);
    s.words = const {
      '/p': [Instructions('AGENTS.md', 'd1', 'Use tabs.\nAlways.')],
      '/q': [Instructions('AGENTS.md', 'd2', 'Other.')],
    };
    await t.tap(find.bySemanticsLabel('New session').first);
    await settled(t);
    Finder field(String l) => find.descendant(
      of: find.byWidgetPredicate((w) => w is AxField && w.label == l),
      matching: find.byType(EditableText),
    );
    Future<void> folder(String p) async {
      await t.enterText(field('Folder'), p);
      await settled(t);
    }

    await folder('/none');
    expect(find.textContaining('Follow'), findsNothing);
    await folder('/p');
    expect(find.textContaining('Use tabs'), findsNothing);
    await t.tap(find.text('Follow AGENTS.md'));
    await settled(t);
    expect(find.textContaining('Use tabs'), findsOneWidget, reason: 'what is followed is shown');
    await snap(t, 'instructions');
    // Another folder's are other words.
    await folder('/q');
    expect(t.widget<AxCheck>(find.byType(AxCheck)).value, isFalse);
    await folder('/p');
    await t.tap(find.text('Follow AGENTS.md'));
    await settled(t);
    await t.enterText(field('What should be done'), 'x');
    await t.ensureVisible(find.bySemanticsLabel('Model: none'));
    await t.tap(find.bySemanticsLabel('Model: none'));
    await settled(t);
    await t.tap(find.bySemanticsLabel('gpt-5.6').last);
    await settled(t);
    await t.ensureVisible(find.text('Start'));
    await t.tap(find.text('Start'));
    await settled(t);
    expect(s.calls.firstWhere((c) => c.first == 'add'), [
      'add',
      's_new',
      'Lead',
      'c1',
      'gpt-5.6',
      Access.ask,
      'd1',
    ]);
  });

  testWidgets('a folder is chosen again, and ~ is home', (t) async {
    Json info(String id, String? w) => {
      'id': id,
      'title': 'T',
      'workspace': w,
      'created': 1,
      'updated': 1,
      'last_seq': 1,
      'running': false,
    };
    final s = Asked()
      ..connections = const [key]
      ..sessions = [
        for (final (i, w) in ['/a/one', null, '/a/one', '/b/two'].indexed)
          SessionInfo.decode(info('$i', w)),
      ];
    addTearDown(s.dispose);
    expect(recentFolders(s.sessions), ['/a/one', '/b/two']);
    expect(recentFolders(s.sessions, most: 1), ['/a/one']);
    await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
    await t.tap(find.bySemanticsLabel('New session').first);
    await settled(t);
    await t.tap(find.bySemanticsLabel(RegExp('^Recent folders')));
    await settled(t);
    await t.tap(find.bySemanticsLabel(RegExp('^two')).last);
    await settled(t);
    expect(find.text('/b/two'), findsOneWidget);
    expect(wholePath('/x/~/y'), '/x/~/y');
    expect(wholePath('~x'), '~x');
    expect(wholePath('~/x'), isNot(startsWith('~')));
    expect(wholePath('~/x'), endsWith('/x'));
  });

  testWidgets('what fails is said, keeps what was written, and is not made twice', (t) async {
    final s = await shell(t);
    await t.tap(find.bySemanticsLabel('New session').first);
    await settled(t);
    await t.tap(find.bySemanticsLabel('Model: none'));
    await settled(t);
    await t.tap(find.bySemanticsLabel('Another, by name').last);
    await settled(t);
    for (final (l, v) in [('What should be done', 'x'), ('Folder', '/w'), ('Model name', 'm')]) {
      await t.enterText(
        find.descendant(
          of: find.byWidgetPredicate((w) => w is AxField && w.label == l),
          matching: find.byType(EditableText),
        ),
        v,
      );
    }
    await settled(t);
    s.failure = const Failed(1, 'bad_request', '/w: not a folder');
    await t.ensureVisible(find.text('Start'));
    await t.tap(find.text('Start'));
    await settled(t);
    expect(find.textContaining('not a folder'), findsOneWidget);
    expect(find.text('/w'), findsOneWidget);
    s
      ..failure = null
      ..calls.clear();
    await t.ensureVisible(find.text('Start'));
    await t.tap(find.text('Start'));
    await settled(t);
    expect(s.calls.map((c) => c.first), ['create', 'add', 'open', 'prompt']);
  });

  testWidgets('a question is answered once, with what it showed', (t) async {
    final s = await shell(t, session: working(asks: true));
    expect(find.byType(RequestBar), findsOneWidget);
    expect(find.text('1 more after this'), findsOneWidget);
    expect(find.text('cargo test'), findsWidgets);
    await settled(t);
    await snap(t, 'asked');
    await t.tap(find.text('Allow once'));
    await settled(t);
    expect(s.calls, [
      ['answer', 's', 'q1', 'd-q1', Answer.once],
    ]);
    // The next one is another question: a click meant for the first does nothing to it.
    expect(find.text('cargo bench'), findsWidgets);
    await t.tap(find.text('Deny'));
    await settled(t);
    expect(s.calls.last, ['answer', 's', 'q2', 'd-q2', Answer.deny]);
    expect(find.byType(RequestBar), findsNothing);
    // The host did not take it: it is asked again.
    s.failure = const Failed(1, 'bad_request', 'what was shown is not what is asked');
    s.current!.apply([
      const Event(
        90,
        90,
        RequestOpened(
          'q3',
          'a',
          'asks to run a command',
          'ls',
          '',
          Answerable.live,
          digest: 'x',
          session: true,
        ),
      ),
    ]);
    await settled(t);
    await t.tap(find.text('Allow this for the session'));
    await settled(t);
    expect(s.calls.last, ['answer', 's', 'q3', 'x', Answer.session]);
    expect(find.byType(RequestBar), findsOneWidget);
    expect(find.textContaining('not what is asked'), findsOneWidget);
  });

  testWidgets('an agent is spoken to and stopped', (t) async {
    final s = await shell(t, session: working());
    final field = find.byType(EditableText);
    await t.tap(field);
    await t.enterText(field, 'Also the index.');
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    expect(s.calls, [
      ['prompt', 's', 'a', 'Also the index.'],
    ]);
    expect(find.text('Also the index.'), findsNothing);
    s.failure = const LinkClosed('the host is gone');
    await t.enterText(field, 'Not lost.');
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    expect(find.text('Not lost.'), findsOneWidget, reason: 'it was not taken');
    expect(find.text('the host is gone'), findsOneWidget);
    s.failure = null;
    await t.tap(find.text('Stop'));
    await settled(t);
    expect(s.calls.last, ['cancel', 's', 'r']);
    s.current!.apply([
      const Event(50, 50, RunEnded('r', Outcome.failed, 'the provider declined')),
      const Event(51, 51, AgentStateChanged('a', AgentPhase.failed)),
    ]);
    await settled(t);
    expect(find.text('Stop'), findsNothing);
    expect(find.textContaining('declined'), findsOneWidget);
  });

  testWidgets('what it runs on is changed, with its cost said first', (t) async {
    final s = await shell(t, session: working());
    await t.runAsync(() => s.models('c1'));
    s.calls.clear();
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    await t.tap(find.bySemanticsLabel('Model: claude-opus-5-5'));
    await settled(t);
    await t.tap(find.bySemanticsLabel('gpt-5.6').last);
    await settled(t);
    expect(s.calls, isEmpty);
    expect(find.textContaining('has to be read again'), findsOneWidget);
    await t.tap(find.text('Change to gpt-5.6'));
    await settled(t);
    expect(s.calls, [
      ['set', 's', 'a', 'gpt-5.6', 'c1', null],
    ]);
    await t.tap(find.bySemanticsLabel('Access: Ask first'));
    await settled(t);
    await t.tap(find.bySemanticsLabel(RegExp('Work in the folder')).last);
    await settled(t);
    expect(s.calls.last, ['set', 's', 'a', null, null, Access.work]);
  });

  testWidgets('what the host says of an agent is what is shown', (t) async {
    final s = await shell(t, session: working());
    expect(find.byType(ContextGauge), findsNothing, reason: 'nothing is known');
    s.current!.apply([
      Event(
        60,
        60,
        UsageReported('a', Usage.decode({'input': 10, 'context': .42}), Known.reported),
      ),
      const Event(
        61,
        61,
        AgentChanged(
          'a',
          Execution(
            harness: 'axiom',
            provider: 'anthropic',
            model: 'claude-haiku-5-5',
            connection: 'c1',
          ),
          Access.work,
        ),
      ),
    ]);
    await settled(t);
    expect(find.byType(ContextGauge), findsOneWidget);
    expect(find.textContaining('claude-haiku-5-5', findRichText: true), findsWidgets);
    expect(s.current!.agents['a']!.access, Access.work);
  });

  testWidgets('what an agent changed is shown as a diff', (t) async {
    final s = await shell(t, session: working());
    const diff =
        '--- a.txt\n+++ a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+2\n three\n'
        '--- (new)\n+++ b.txt\n@@ -0,0 +1 @@\n+new \u202Eline\n';
    s.current!.apply([
      const Event(70, 70, ItemStarted('w', 'r', 'a', ItemKind.tool, 'patch', {'patch': '…'})),
      const Event(71, 71, ItemDelta('w', 'changed a.txt, b.txt')),
      const Event(
        72,
        72,
        ItemEnded('w', Outcome.completed, 'changed a.txt, b.txt', {
          'ok': true,
          'diff': diff,
          'summary': '+2 −1',
        }),
      ),
    ]);
    await settled(t);
    expect(find.text('+2 −1'), findsOneWidget);
    expect(find.byType(DiffLines), findsNothing);
    await t.tap(find.text('Patch'));
    await settled(t);
    expect(find.byType(DiffLines), findsNWidgets(2));
    expect(find.text('b.txt'), findsOneWidget);
    expect(find.textContaining('two', findRichText: true), findsOneWidget);
    expect(find.textContaining(r'\u{202E}', findRichText: true), findsOneWidget);
    await snap(t, 'diff');
  });

  test('a diff is read line by line, and no more than there is room for', () {
    final f = readDiffs(
      '--- a\n+++ a\n@@ -10,2 +10,3 @@ fn x\n same\n-old\n+new\n+more\n@@ -90 +91 @@\n-x\n+y\n',
    ).single;
    expect((f.path, f.added, f.removed, f.left), ('a', 3, 2, 0));
    expect(
      [for (final l in f.lines) (l.kind, l.number, l.text)],
      [
        (DiffKind.same, 10, 'same'),
        (DiffKind.remove, 11, 'old'),
        (DiffKind.add, 11, 'new'),
        (DiffKind.add, 12, 'more'),
        (DiffKind.remove, 90, 'x'),
        (DiffKind.add, 91, 'y'),
      ],
    );
    expect(readDiffs('--- b\n+++ (removed)\n@@ -1 +0,0 @@\n-x\n').single.path, 'b');
    // A removed line that looks like the beginning of a file is a removed line.
    final dashes = readDiffs(
      '--- a\n+++ a\n@@ -1,2 +1,2 @@\n--- x\n+++ y\n same\n--- c d\t\n+++ c d\t\n@@ -1 +1 @@\n-1\n+2\n',
    );
    expect([for (final f in dashes) f.path], ['a', 'c d']);
    expect([for (final l in dashes.first.lines) l.text], ['-- x', '++ y', 'same']);
    final many = readDiffs('--- a\n+++ a\n@@ -0,0 +1,500 @@\n${'+x\n' * 500}', most: 10).single;
    expect((many.lines.length, many.left, many.added), (10, 490, 500));
    final all = readDiffs('--- a\n+++ a\n@@ -0,0 +1,9 @@\n${'+x\n' * 9}' * 3, atAll: 12);
    expect([for (final f in all) f.lines.length], [9, 3, 0]);
    for (final odd in [
      '',
      'nonsense',
      '+++ a',
      '@@',
      '--- a\n+++ a\n+x',
      '--- a\n+++ a\n@@ x @@\n+x',
    ]) {
      readDiffs(odd);
    }
  });

  testWidgets('a replayed agent is shown and not spoken to', (t) async {
    await shell(t, session: working(access: null, asks: true));
    expect(find.byType(EditableText), findsNothing);
    expect(find.byType(RequestBar), findsNothing);
    expect(find.text('Stop'), findsNothing);
    expect(find.textContaining('cargo bench'), findsOneWidget, reason: 'what was asked is told');
  });

  testWidgets('accounts are added and removed, and a key is not shown', (t) async {
    final s = await shell(t, accounts: false);
    await t.tap(find.bySemanticsLabel('Accounts and servers'));
    await settled(t);
    Finder field(String l) => find.descendant(
      of: find.byWidgetPredicate((w) => w is AxField && w.label == l),
      matching: find.byType(EditableText),
    );
    await t.tap(find.bySemanticsLabel(RegExp('^Add: ')));
    await settled(t);
    await t.tap(find.bySemanticsLabel('Anthropic API').last);
    await settled(t);
    await t.enterText(field('Name'), 'Work');
    await t.enterText(field('Key'), 'sk-secret');
    await settled(t);
    expect(t.widget<EditableText>(field('Key')).obscureText, isTrue);
    await snap(t, 'accounts');
    await t.tap(find.widgetWithText(AxButton, 'Add'));
    await settled(t);
    expect(s.calls, [
      ['connect', 'Work', 'anthropic', null, 'sk-secret'],
    ]);
    expect(t.widget<EditableText>(field('Key')).controller.text, isEmpty);
    expect(find.text('Work'), findsOneWidget);
    await t.tap(find.text('Remove'));
    await settled(t);
    expect(s.calls.length, 1, reason: 'asked first');
    await t.tap(find.text('Remove Work'));
    await settled(t);
    expect(s.calls.last, ['disconnect', 'c_new']);
    expect(find.text('Work'), findsNothing);
  });

  testWidgets('a provider that needs a key is not added without one', (t) async {
    final s = await shell(t, accounts: false);
    await t.tap(find.bySemanticsLabel('Accounts and servers'));
    await settled(t);
    Finder field(String l) => find.descendant(
      of: find.byWidgetPredicate((w) => w is AxField && w.label == l),
      matching: find.byType(EditableText),
    );
    for (final (kind, name, from) in const [
      ('openai', 'OpenAI API', 'the OpenAI platform'),
      ('gemini', 'Gemini API', 'Google AI Studio'),
      ('openrouter', 'OpenRouter', 'OpenRouter'),
    ]) {
      await t.tap(find.bySemanticsLabel(RegExp('^Add: ')));
      await settled(t);
      await t.tap(find.bySemanticsLabel(name).last);
      await settled(t);
      expect(find.text('An API key from $from. It goes to the system\'s store.'), findsOneWidget);
      expect(field('Address'), findsNothing, reason: 'a key goes to its provider and nowhere else');
      await t.enterText(field('Name'), 'Mine');
      await settled(t);
      await t.tap(find.widgetWithText(AxButton, 'Add'));
      await settled(t);
      expect(s.calls.where((c) => c.contains(kind)), isEmpty);
      await t.enterText(field('Key'), 'sk-secret');
      await settled(t);
      await t.tap(find.widgetWithText(AxButton, 'Add'));
      await settled(t);
      expect(s.calls.last, ['connect', 'Mine', kind, null, 'sk-secret']);
      expect(find.textContaining('$name · '), findsWidgets);
      expect(markOf(kind, null)?.label, name.split(' ').first);
    }
  });

  testWidgets('fits a small window at twice the text', (t) async {
    final s = Asked()
      ..current = working(asks: true)
      ..connections = const [key];
    addTearDown(s.dispose);
    await mount(t, Shell(s), size: const Size(760, 560), dpr: 1, textScale: 1.6);
    await settled(t);
    expect(t.takeException(), isNull);
    for (final l in ['New session', 'Accounts and servers']) {
      await t.tap(find.bySemanticsLabel(l).first);
      await settled(t);
      expect(t.takeException(), isNull, reason: l);
    }
  });

  Finder field(String l) => find.descendant(
    of: find.byWidgetPredicate((w) => w is AxField && w.label == l),
    matching: find.byType(EditableText),
  );
  Future<void> press(WidgetTester t, Finder f) async {
    await t.ensureVisible(f);
    await t.tap(f);
    await settled(t);
  }

  /// Everything but the start itself.
  Future<void> fill(WidgetTester t, String say) async {
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('What should be done'), say);
    await t.enterText(field('Folder'), '/w');
    await settled(t);
    await press(t, find.bySemanticsLabel('Model: none'));
    await press(t, find.bySemanticsLabel('gpt-5.6').last);
  }

  const harnesses = [
    HarnessInfo(
      'claude',
      'Claude Code',
      program: '/usr/local/bin/claude',
      access: [Access.read, Access.work],
      accounts: 'claude-code',
    ),
    HarnessInfo(
      'goose',
      'Goose',
      program: '/usr/local/bin/goose',
      needsTrust: true,
      access: [Access.ask],
    ),
    HarnessInfo('codex', 'Codex', problem: 'codex is not in any of the folders'),
    HarnessInfo('qwen', 'Qwen Code', problem: 'qwen is not in any of the folders'),
  ];

  testWidgets('a session can run with a program of the person\'s own', (t) async {
    final s = await shell(t, accounts: false);
    s.harnesses = harnesses;
    await press(t, find.bySemanticsLabel('New session').first);
    expect(find.text('Add an account or a server'), findsOneWidget);
    await press(t, find.bySemanticsLabel('Runs with: Axiom'));
    expect(find.text('not on this computer'), findsOneWidget, reason: 'Codex is not there');
    expect(find.bySemanticsLabel('Qwen Code'), findsNothing);
    await t.tap(find.text('Codex'), warnIfMissed: false);
    await settled(t);
    expect(find.bySemanticsLabel('Runs with: Axiom'), findsOneWidget, reason: 'nothing happens');
    await press(t, find.bySemanticsLabel('Claude Code').last);
    // No account is needed, and what it has no mode for is not offered.
    expect(find.text('Add an account or a server'), findsNothing);
    expect(find.bySemanticsLabel('Access: Only look'), findsOneWidget, reason: 'the least');
    await press(t, find.bySemanticsLabel('Access: Only look'));
    expect(find.text('Claude Code has no mode for it'), findsOneWidget);
    await press(t, find.bySemanticsLabel('Work in the folder').last);
    await t.enterText(field('What should be done'), 'Do it.');
    await t.enterText(field('Folder'), '/w');
    await settled(t);
    await snap(t, 'start-harness');
    await press(t, find.text('Start'));
    expect(s.calls, [
      ['create', 'Do it.', '/w'],
      ['add', 's_new', 'Lead', ('claude', false), null, Access.work],
      ['open', 's_new'],
      ['prompt', 's_new', 'a_new', 'Do it.'],
    ]);
  });

  const work = Connection(
    'c9',
    'Work',
    'claude-code',
    signIn: "CLAUDE_CONFIG_DIR='/data/accounts/c9' claude auth login",
  );

  testWidgets('a program can have several accounts', (t) async {
    final s = await shell(t);
    s.harnesses = harnesses;
    await press(t, find.bySemanticsLabel('Accounts and servers').first);
    await press(t, find.bySemanticsLabel('Add: LM Studio on this machine'));
    await press(t, find.bySemanticsLabel('Claude Code, another account').last);
    expect(field('Key'), findsNothing, reason: 'it signs in by itself');
    await t.enterText(field('Name'), 'Work');
    await settled(t);
    s.calls.clear();
    await press(t, find.widgetWithText(AxButton, 'Add'));
    expect(s.calls, [
      ['connect', 'Work', 'claude-code', null, null],
    ]);
    s.connections = [key, work];
    s.notifyListeners();
    await settled(t);
    expect(find.text(work.signIn!), findsOneWidget);
    String? copied;
    t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (m) async {
      if (m.method == 'Clipboard.setData') copied = (m.arguments as Map)['text'] as String;
      return null;
    });
    await press(t, find.text('Copy to sign in'));
    expect(copied, work.signIn);
    expect(find.text('Copied'), findsOneWidget);
    await snap(t, 'accounts-program');
    await press(t, find.widgetWithText(AxButton, 'Remove').last);
    expect(find.textContaining('does not sign it out'), findsOneWidget);
    await press(t, find.text('Keep'));
    // It is one to choose for its program, and not for Axiom's own loop.
    await press(t, find.bySemanticsLabel('New session').first);
    await press(t, find.bySemanticsLabel('Account: Mine'));
    expect(find.bySemanticsLabel('Work'), findsNothing);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    await press(t, find.bySemanticsLabel('Runs with: Axiom'));
    await press(t, find.bySemanticsLabel('Goose').last);
    expect(find.byType(AxSelect<String>), findsOneWidget, reason: 'it has one account');
    await press(t, find.bySemanticsLabel('Runs with: Goose'));
    await press(t, find.bySemanticsLabel('Claude Code').last);
    await press(t, find.bySemanticsLabel('Account: Its own sign-in'));
    await press(t, find.bySemanticsLabel('Work').last);
    await t.enterText(field('What should be done'), 'Do it.');
    await t.enterText(field('Folder'), '/w');
    await settled(t);
    s.calls.clear();
    await press(t, find.text('Start'));
    expect(s.calls.where((c) => c.first == 'add').single, [
      'add',
      's_new',
      'Lead',
      ('claude', false),
      'c9',
      null,
      Access.read,
    ]);
  });

  testWidgets('changing a program\'s account says what that means', (t) async {
    final s = await shell(t, session: working());
    s
      ..harnesses = harnesses
      ..connections = [key, work];
    const was = Execution(harness: 'claude', connection: 'c9');
    s.current!.apply([
      const Event(91, 91, AgentStateChanged('a', AgentPhase.done)),
      const Event(92, 92, AgentChanged('a', was, Access.read)),
    ]);
    s.notifyListeners();
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(s.calls, isEmpty, reason: 'it lists no models');
    await press(t, find.bySemanticsLabel('Account: Work'));
    await press(t, find.bySemanticsLabel('Its own sign-in').last);
    expect(find.textContaining('begins anew there'), findsOneWidget);
    await t.pump(AxMotion.settle);
    await press(t, find.text('Change to Its own sign-in'));
    expect(s.calls, [
      ['set', 's', 'a', null, null, null],
    ]);
  });

  testWidgets('what cannot ignore a folder\'s settings needs a yes to them', (t) async {
    final s = await shell(t);
    s.harnesses = harnesses;
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('What should be done'), 'Do it.');
    await t.enterText(field('Folder'), '/w');
    await press(t, find.bySemanticsLabel('Runs with: Axiom'));
    await press(t, find.bySemanticsLabel('Goose').last);
    AxButton start() => t.widget<AxButton>(find.widgetWithText(AxButton, 'Start'));
    expect(start().onPressed, isNull);
    await t.enterText(field('Model name'), 'gpt-5.6');
    await press(t, find.byType(AxCheck));
    expect(start().onPressed, isNotNull);
    // Another has not been said yes for.
    await press(t, find.bySemanticsLabel('Runs with: Goose'));
    await press(t, find.bySemanticsLabel('Claude Code').last);
    expect(t.widget<AxCheck>(find.byType(AxCheck)).value, isFalse);
    expect(
      t
          .widget<AxField>(find.byWidgetPredicate((w) => w is AxField && w.label == 'Model name'))
          .controller!
          .text,
      isEmpty,
    );
    await press(t, find.bySemanticsLabel('Runs with: Claude Code'));
    await press(t, find.bySemanticsLabel('Goose').last);
    await press(t, find.byType(AxCheck));
    await press(t, find.text('Start'));
    expect(s.calls.where((c) => c.first == 'add').single, [
      'add',
      's_new',
      'Lead',
      ('goose', true),
      null,
      Access.ask,
    ]);
  });

  testWidgets('another agent joins a session', (t) async {
    final s = await shell(t, session: working());
    s
      ..harnesses = harnesses
      ..words = const {
        '/w': [Instructions('AGENTS.md', 'd1', 'Use tabs.')],
      };
    await press(t, find.bySemanticsLabel('Add an agent to Fix the cache'));
    expect(find.text('New agent'), findsOneWidget);
    expect(field('Folder'), findsNothing, reason: 'it is the session\'s');
    expect(find.text('Follow AGENTS.md'), findsOneWidget);
    await t.enterText(field('Name'), 'Reviewer');
    await press(t, find.bySemanticsLabel('Runs with: Axiom'));
    await press(t, find.bySemanticsLabel('Claude Code').last);
    await snap(t, 'add-agent');
    s.calls.clear();
    // It need not be told anything yet. While it is set up the other goes on.
    await press(t, find.text('Add'));
    expect(s.calls, [
      ['add', 's', 'Reviewer', ('claude', false), null, Access.read],
    ]);
    expect(find.byType(StartPage), findsNothing);
    s.current!.apply([
      const Event(
        91,
        91,
        AgentAdded('a_new', 'Reviewer', Execution(harness: 'claude'), null, access: Access.read),
      ),
    ]);
    await settled(t);
    // It is the one that is shown.
    expect(find.text('Say something to Reviewer'), findsOneWidget);
    await snap(t, 'two-agents');
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(find.text('Its own model'), findsWidgets);
    expect(find.text('Its own sign-in'), findsWidgets);
    expect(s.calls.length, 1, reason: 'there is no account whose models could be asked for');
  });

  testWidgets('the window opens where its person was last, once', (t) async {
    final s = Asked()
      ..sessions = [
        for (final (id, title, at) in const [('s2', 'Newer', 9), ('s1', 'Older', 2)])
          SessionInfo.decode({
            'id': id,
            'title': title,
            'created': 1,
            'updated': at,
            'last_seq': 1,
            'running': false,
          }),
      ];
    addTearDown(s.dispose);
    await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
    await settled(t);
    expect(s.calls, [
      ['open', 's2'],
    ]);
    await press(t, find.text('Older'));
    expect(s.calls.last, ['open', 's1']);
    expect(s.calls.length, 2);
  });

  testWidgets('what has changed in the folder is shown, file by file', (t) async {
    final s = await shell(t, session: working());
    s.now = const Changes(
      repository: true,
      branch: 'trunk',
      files: [
        Changed('lib/cache.dart', 'changed'),
        Changed('new dir/é.txt', 'new'),
        Changed('gone.txt', 'removed'),
        Changed('logo.png', 'new'),
      ],
      diff:
          'diff --git lib/cache.dart lib/cache.dart\nindex 1..2 100644\n--- lib/cache.dart\n+++ lib/cache.dart\n'
          '@@ -10,3 +10,3 @@ class Cache {\n   final a;\n--- old\n+  final b;\n   final c;\n'
          'diff --git gone.txt gone.txt\ndeleted file mode 100644\n--- gone.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-was here\n'
          '--- (new)\n+++ new dir/é.txt\n@@ -0,0 +1 @@\n+fresh\n',
      whole: false,
    );
    await t.tap(find.text('Changes').last);
    await settled(t);
    expect(find.text('trunk'), findsOneWidget);
    expect(find.text('4 files'), findsOneWidget);
    for (final (file, what, add, remove) in [
      ('lib/cache.dart', 'changed', 1, 1),
      ('new dir/é.txt', 'new', 1, 0),
      ('gone.txt', 'removed', 0, 1),
      ('logo.png', 'new', 0, 0),
    ]) {
      expect(
        find.bySemanticsLabel('$file, $what, $add added, $remove removed'),
        findsOneWidget,
        reason: file,
      );
    }
    for (final l in ['  final b;', 'fresh', 'was here', '-- old']) {
      expect(find.textContaining(l, findRichText: true), findsWidgets, reason: l);
    }
    expect(find.text('There is more than can be shown here.'), findsOneWidget);
    await snap(t, 'changes');
    await t.tap(find.bySemanticsLabel('lib/cache.dart, changed, 1 added, 1 removed'));
    await settled(t);
    expect(find.textContaining('  final b;', findRichText: true), findsNothing);
    // When somebody has stopped working it is looked at again.
    s.calls.clear();
    s.now = const Changes(repository: true, branch: 'trunk');
    s.current!.apply([
      const Event(92, 92, RunEnded('r', Outcome.completed, null)),
      const Event(93, 93, AgentStateChanged('a', AgentPhase.done)),
    ]);
    await settled(t);
    expect(s.calls, [
      ['changes', 's'],
    ]);
    expect(find.text('Nothing has changed since the last commit.'), findsOneWidget);
    s.failure = const Failed(1, 'bad_request', 'there is no sandbox on this system');
    await t.tap(find.text('Look again'));
    await settled(t);
    expect(find.text('there is no sandbox on this system'), findsOneWidget);
    await t.tap(find.text('Conversation').last);
    await settled(t);
    expect(find.textContaining('I will run the tests'), findsOneWidget);
  });

  testWidgets('a double click answers one question, not the next as well', (t) async {
    final s = await shell(t, session: working(asks: true));
    await settled(t);
    final at = t.getCenter(find.text('Allow once'));
    for (final ms in [60, 96, 160, 200]) {
      await t.tapAt(at);
      await t.pump(Duration(milliseconds: ms));
    }
    expect(s.calls, [
      ['answer', 's', 'q1', 'd-q1', Answer.once],
    ]);
    // Nor does a key that is held down.
    await t.pump(AxMotion.settle);
    expect(find.text('cargo bench'), findsWidgets);
    for (var i = 0; i < 4; i++) {
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.pump(const Duration(milliseconds: 30));
    }
    expect(s.calls.length, 1);
  });

  testWidgets('what is being written to an agent is there when its person comes back', (t) async {
    final s = await shell(t, session: working());
    final words = find.byType(EditableText);
    await t.enterText(words, 'Half a thought');
    await press(t, find.bySemanticsLabel('Accounts and servers').first);
    expect(find.text('Half a thought'), findsNothing);
    await press(t, find.text('Done'));
    expect(find.text('Half a thought'), findsOneWidget);
    // Nor lost when it fails while the next is being written, or while one looks elsewhere.
    final gate = Completer<void>();
    s
      ..gate = gate
      ..failure = const LinkClosed('the host is gone');
    await t.tap(words);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    await t.enterText(words, 'and the next');
    await press(t, find.bySemanticsLabel('Accounts and servers').first);
    gate.complete();
    await settled(t);
    await press(t, find.text('Done'));
    expect(find.text('Half a thought\n\nand the next'), findsOneWidget);
    expect(find.text('the host is gone'), findsOneWidget);
    // Said again, it is the same; said again after it was heard, it is not.
    await t.tap(words);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    s.failure = null;
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    await t.enterText(words, 'Half a thought\n\nand the next');
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    expect(s.keys.length, 4);
    expect(s.keys[1], isNot(s.keys[0]));
    expect(s.keys[2], s.keys[1]);
    expect(s.keys[3], isNot(s.keys[2]));
    expect(s.keys.first, hasLength(32));
  });

  testWidgets('drafts of two agents are two, and not too many are kept', (t) async {
    final d = Drafts();
    d.of('s', 'a').text.text = 'one';
    d.of('s', 'b').text.text = 'two';
    expect([d.of('s', 'a').text.text, d.of('s', 'b').text.text], ['one', 'two']);
    for (var i = 0; i < 1000; i++) {
      d.of('s', '$i');
    }
    await t.pumpWidget(const SizedBox());
    expect([d.of('s', 'a').text.text, d.of('s', 'b').text.text], ['one', 'two']);
    for (var i = 0; i < 1000; i++) {
      d.of('t', '$i').text.text = 'x';
    }
    await t.pumpWidget(const SizedBox());
    expect(d.of('s', 'a').text.text, isEmpty);
    d.dispose();
  });

  testWidgets('adding an account on the way loses nothing of a new session', (t) async {
    final s = await shell(t, accounts: false);
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('What should be done'), 'precious words');
    await t.enterText(field('Folder'), '/w');
    await settled(t);
    await press(t, find.text('Add an account or a server'));
    s.connections = const [key];
    await press(t, find.text('Done'));
    if (find.byType(StartPage).evaluate().isEmpty) {
      await press(t, find.bySemanticsLabel('New session').first);
    }
    expect(find.text('precious words'), findsOneWidget);
    expect(find.text('/w'), findsOneWidget);
  });

  testWidgets('starting again makes nothing twice, and takes what was changed', (t) async {
    final s = await shell(t);
    await fill(t, 'Do it.');
    s
      ..calls.clear()
      ..failsAt = 'prompt'
      ..failure = const LinkClosed('no answer');
    await press(t, find.text('Start'));
    await press(t, find.text('Start'));
    expect(s.calls.map((c) => c.first), ['create', 'add', 'open', 'prompt', 'open', 'prompt']);
    expect(s.keys[0], s.keys[1]);
    s.calls.clear();
    await press(t, find.bySemanticsLabel('Access: Ask first'));
    await press(t, find.bySemanticsLabel('Work in the folder').last);
    s.failure = null;
    await press(t, find.text('Start'));
    expect(s.calls, [
      ['set', 's_new', 'a_new', 'gpt-5.6', 'c1', Access.work],
      ['open', 's_new'],
      ['prompt', 's_new', 'a_new', 'Do it.'],
    ]);
    // The next is another.
    s.calls.clear();
    await press(t, find.bySemanticsLabel('New session').first);
    expect(find.text('Do it.'), findsNothing);
    await t.enterText(field('What should be done'), 'Do it.');
    await settled(t);
    await press(t, find.text('Start'));
    expect(s.calls.map((c) => c.first), ['create', 'add', 'open', 'prompt']);
    expect(s.keys.last, isNot(s.keys[1]));
  });

  testWidgets('who leaves while it starts is not taken back, and is told', (t) async {
    final s = await shell(t);
    await fill(t, 'precious words');
    final gate = Completer<void>();
    s
      ..calls.clear()
      ..failsAt = 'add'
      ..gate = gate;
    await press(t, find.text('Start'));
    await press(t, find.bySemanticsLabel('Accounts and servers').first);
    gate.complete();
    await settled(t);
    expect(s.calls.map((c) => c.first), ['create', 'add', 'prompt'], reason: 'it is not opened');
    expect(find.byType(AccountsPage), findsOneWidget);
    // And when it fails:
    s
      ..failsAt = 'prompt'
      ..gate = Completer<void>()
      ..failure = const LinkClosed('refused it');
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('What should be done'), 'other words');
    await settled(t);
    await press(t, find.text('Start'));
    await press(t, find.bySemanticsLabel('Accounts and servers').first);
    s.gate!.complete();
    await settled(t);
    await press(t, find.bySemanticsLabel('New session').first);
    expect(find.text('other words'), findsOneWidget);
    expect(find.text('refused it'), findsOneWidget);
  });

  testWidgets('a model being written is not taken away when the list comes', (t) async {
    final s = await shell(t);
    final gate = Completer<void>();
    s
      ..failsAt = 'models'
      ..gate = gate;
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('Model name'), 'my-own');
    gate.complete();
    await settled(t);
    expect(find.text('my-own'), findsOneWidget);
    expect(find.bySemanticsLabel('Model: Another, by name'), findsOneWidget);
  });

  testWidgets('a provider that lists nothing: its model is written', (t) async {
    final s = await shell(t);
    s.known = const [];
    await press(t, find.bySemanticsLabel('New session').first);
    expect(field('Model name'), findsOneWidget);
    expect(find.byType(AxSelect<Access>), findsOneWidget);
  });

  testWidgets('models are asked for when somebody looks, once', (t) async {
    final s = await shell(t, session: working());
    expect(s.calls, isEmpty);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(s.calls, [
      ['models', 'c1'],
    ]);
    await t.tap(find.text('Model'));
    await settled(t);
    expect(find.text('gpt-5.6'), findsWidgets);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(s.calls.length, 1);
  });

  testWidgets('a stop that fails says so', (t) async {
    final s = await shell(t, session: working());
    s.failure = const LinkClosed('the host is gone');
    await t.tap(find.text('Stop'));
    await settled(t);
    expect(find.textContaining('It was not stopped: the host is gone'), findsOneWidget);
    s.failure = null;
    await t.tap(find.text('Stop'));
    await settled(t);
    expect(find.textContaining('It was not stopped'), findsNothing);
  });

  for (final (size, scale) in const [
    // No window is smaller (macos, windows, linux: the runners say so).
    (Size(760, 520), 1.0),
    (Size(760, 520), 2.0),
    (Size(900, 600), 1.5),
  ]) {
    testWidgets('fits ${size.width.round()} by ${size.height.round()} at $scale', (t) async {
      final s = Asked()
        ..current = working(asks: true)
        ..connections = const [key];
      addTearDown(s.dispose);
      await mount(
        t,
        MediaQuery(
          data: MediaQueryData(size: size, textScaler: TextScaler.linear(scale)),
          child: Shell(s),
        ),
        size: size,
        dpr: 1,
      );
      await settled(t);
      expect(t.takeException(), isNull);
      for (final l in ['New session', 'Accounts and servers']) {
        await t.tap(find.bySemanticsLabel(l).first);
        await settled(t);
        expect(t.takeException(), isNull, reason: l);
      }
    });
  }
}
