import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/context_gauge.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/patterns/models.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/start.dart';
import 'package:axiom/ui/controls.dart';
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

  Future<T> _do<T>(List<Object?> call, T answer) async {
    calls.add(call);
    if (failure case final f?) throw f;
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
  }) => _do([
    'add',
    session,
    name,
    x.connection,
    x.model,
    access,
    for (final i in instructions) i.digest,
  ], 'a_new');
  Map<String, List<Instructions>> words = const {};
  @override
  Future<List<Instructions>> look(String path) async =>
      words[path] ?? (throw const Failed(1, 'bad', 'x'));
  @override
  Future<void> open(String id) => _do(['open', id], null);
  @override
  Future<void> prompt(String session, String agent, String text) =>
      _do(['prompt', session, agent, text], null);
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
        RequestOpened('q3', 'a', 'asks to run a command', 'ls', '', Answerable.live, digest: 'x'),
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
    final (files, left) = readDiff(
      '--- a\n+++ a\n@@ -10,2 +10,3 @@ fn x\n same\n-old\n+new\n+more\n@@ -90 +91 @@\n-x\n+y\n',
    );
    expect(files.single.$1, 'a');
    expect(
      [for (final l in files.single.$2) (l.kind, l.number, l.text)],
      [
        (DiffKind.same, 10, 'same'),
        (DiffKind.remove, 11, 'old'),
        (DiffKind.add, 11, 'new'),
        (DiffKind.add, 12, 'more'),
        (DiffKind.remove, 90, 'x'),
        (DiffKind.add, 91, 'y'),
      ],
    );
    expect(left, 0);
    final gone = readDiff('--- b\n+++ (removed)\n@@ -1 +0,0 @@\n-x\n').$1.single;
    expect(gone.$1, 'b');
    final many = readDiff('--- a\n+++ a\n@@ -1 +1 @@\n${'+x\n' * 500}', most: 10);
    expect([many.$1.single.$2.length, many.$2], [10, 490]);
    for (final odd in [
      '',
      'nonsense',
      '+++ a',
      '@@',
      '--- a\n+++ a\n+x',
      '--- a\n+++ a\n@@ x @@\n+x',
    ]) {
      readDiff(odd);
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
}
