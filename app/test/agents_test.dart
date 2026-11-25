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
import 'package:axiom/shell/outside.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/start.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/marks.dart';
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
    bool apart = false,
  }) => _do([
    'add',
    session,
    name,
    if (x.harness != 'axiom') (x.harness, trust) else x.connection,
    ?x.environment,
    if (x.effort case final e?) 'effort $e',
    if (x.harness != 'axiom' && x.connection != null) x.connection,
    x.model,
    access,
    for (final i in instructions) i.digest,
    if (apart) 'apart',
  ], 'a_new');

  CopyChanges copy = const CopyChanges();
  @override
  Future<CopyChanges> copyChanges(String session, String agent) =>
      _do(['copy', session, agent], copy);
  Moved moved = const Moved([], []);
  @override
  Future<Moved> bringIn(String session, String agent, List<CopyFile> files) =>
      _do(['bring', for (final f in files) (f.path, f.digest)], moved);
  @override
  Future<Moved> putBack(String session, String agent, String back) =>
      _do(['back', session, agent, back], moved);
  (int, String?, List<(String, String)>) renewed = (0, null, const []);
  @override
  Future<(int, String?, List<(String, String)>)> renewCopy(String session, String agent) =>
      _do(['renew', session, agent], renewed);
  @override
  Future<void> removeCopy(String session, String agent, {required bool lose}) =>
      _do(['throw away', session, agent, lose], null);

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
  Future<void> answer(String session, RequestOpened r, Answer a, {String? words}) =>
      _do(['answer', session, r.request, r.digest, a, ?words], null);
  @override
  Future<void> cancel(String session, String run) => _do(['cancel', session, run], null);
  @override
  Future<void> setAgent(String session, String agent, {Execution? execution, Access? access}) =>
      _do([
        'set',
        session,
        agent,
        execution?.model,
        execution?.connection,
        ?execution?.environment,
        ?execution?.effort,
        access,
      ], null);
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
  Future<bool?> disconnect(String id) async {
    await _do(['disconnect', id], null);
    connections = connections.where((c) => c.id != id).toList();
    notifyListeners();
    return ended;
  }

  /// What the host says of signing out.
  bool? ended;

  @override
  Future<Signin> beginSignin({String? again}) async {
    final n = await _do(['sign in', again], '${calls.length}');
    signin = Signin('n$n', 'https://auth.openai.com/api/accounts/authorize?state=x', again);
    notifyListeners();
    return signin!;
  }

  void toldOfSignin(Json j) {
    signin = signin!.told(j);
    notifyListeners();
  }

  @override
  Future<void> cancelSignin() async {
    await _do(['cancel sign-in', signin?.id], null);
    signin = null;
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

/// The box whose label begins so.
Finder check(String label) =>
    find.byWidgetPredicate((w) => w is AxCheck && w.label.startsWith(label));

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
    expect(t.widget<AxCheck>(check('Follow')).value, isFalse);
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

  group('a ChatGPT plan', () {
    const mine = Connection('c7', 'me@example.com', 'chatgpt', secret: true, lasts: true);
    final opened = <String>[];
    var can = true;
    setUp(() {
      opened.clear();
      can = true;
      opens = (u) async {
        opened.add('$u');
        return can;
      };
    });
    tearDown(() => opens = (u) => throw StateError('a test would have opened $u'));

    Future<Asked> page(WidgetTester t) async {
      final s = await shell(t, accounts: false);
      await t.tap(find.bySemanticsLabel('Accounts and servers'));
      await settled(t);
      return s;
    }

    Future<void> choose(WidgetTester t) async {
      await t.tap(find.bySemanticsLabel(RegExp('^Add: ')));
      await settled(t);
      await t.tap(find.bySemanticsLabel('ChatGPT plan').last);
      await settled(t);
    }

    testWidgets('is signed in to in the person\'s own browser, when they say so', (t) async {
      final s = await page(t);
      await choose(t);
      expect(find.byType(AxField), findsNothing, reason: 'no name and no key');
      expect(find.widgetWithText(AxButton, 'Add'), findsNothing);
      expect(opened, isEmpty);
      await snap(t, 'accounts-plan');
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      expect(s.calls, [
        ['sign in', null],
      ]);
      expect(opened, ['https://auth.openai.com/api/accounts/authorize?state=x']);
      expect(find.text('Go on in your browser'), findsOneWidget);
      final again = t.widget<AxButton>(find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      expect(again.onPressed, isNull, reason: 'one at a time');
      await snap(t, 'accounts-plan-waits');
      await press(t, find.widgetWithText(AxButton, 'Open it again'));
      expect((opened.length, s.calls.length), (2, 1));
      String? copied;
      t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (m) async {
        if (m.method == 'Clipboard.setData') copied = (m.arguments as Map)['text'] as String;
        return null;
      });
      await press(t, find.widgetWithText(AxButton, 'Copy the address'));
      expect(copied, opened.first);
      expect(find.text('Copied'), findsOneWidget);

      s.toldOfSignin({'state': 'finishing'});
      await settled(t);
      expect(find.text('Signing in…'), findsOneWidget);
      s.connections = [mine];
      s.toldOfSignin({'state': 'done', 'connection': 'c7'});
      await settled(t);
      expect(find.text('You\'re using your ChatGPT plan'), findsOneWidget);
      expect(find.text('ChatGPT plan · signed in'), findsOneWidget);
      await snap(t, 'accounts-plan-done');
      await press(t, find.widgetWithText(AxButton, 'Got it'));
      expect(find.text('You\'re using your ChatGPT plan'), findsNothing);
      expect(markOf('chatgpt', null), ServiceMark.openai);
    });

    testWidgets('a sign-in can be given up, fail, and be left for later', (t) async {
      final s = await page(t);
      await choose(t);
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      await press(t, find.widgetWithText(AxButton, 'Cancel'));
      expect(s.calls.last, ['cancel sign-in', 'n0']);
      expect(find.text('Go on in your browser'), findsNothing);
      // What cannot be opened from here is opened by hand.
      can = false;
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      expect(find.text('Open the address in your browser'), findsOneWidget);
      s.toldOfSignin({'state': 'failed', 'problem': 'the sign-in was not allowed'});
      await settled(t);
      expect(find.text('the sign-in was not allowed'), findsOneWidget);
      await press(t, find.widgetWithText(AxButton, 'Close'));
      expect(find.text('You are not signed in'), findsNothing);
      // Another page, and back: it is on still.
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      await press(t, find.bySemanticsLabel('New session').first);
      await press(t, find.bySemanticsLabel('Accounts and servers'));
      s.toldOfSignin({'state': 'done', 'connection': 'c7'});
      await settled(t);
      expect(find.text('You\'re using your ChatGPT plan'), findsOneWidget);
      // The host does not begin one.
      await press(t, find.widgetWithText(AxButton, 'Got it'));
      await choose(t);
      s.failure = const Failed(1, 'busy', 'other sign-ins are waiting');
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      expect(find.text('other sign-ins are waiting'), findsOneWidget);
    });

    testWidgets('is signed in to again, and out of', (t) async {
      final s = await page(t);
      s
        ..connections = const [
          Connection('c7', 'me', 'chatgpt', secret: true, problem: 'it has ended; sign in again'),
        ]
        ..notifyListeners();
      await settled(t);
      expect(find.text('ChatGPT plan · signed in until the host stops'), findsOneWidget);
      expect(find.text('it has ended; sign in again'), findsOneWidget);
      await snap(t, 'accounts-plan-ended');
      await press(t, find.widgetWithText(AxButton, 'Sign in again'));
      expect(s.calls.last, ['sign in', 'c7']);
      // Also with another provider chosen below.
      expect(find.text('Go on in your browser'), findsOneWidget);
      s.toldOfSignin({'state': 'done', 'connection': 'c7'});
      await settled(t);
      expect(find.text('You are signed in again'), findsOneWidget);
      await press(t, find.widgetWithText(AxButton, 'Got it'));
      s.ended = false;
      await press(t, find.text('Remove'));
      await press(t, find.text('Remove me'));
      expect(find.textContaining('OpenAI did not confirm'), findsOneWidget);
    });

    testWidgets('what it is used for says so, and where its limit is', (t) async {
      final s = await shell(t, session: working());
      expect(find.text('Using ChatGPT plan'), findsNothing);
      s.connections = [key, mine];
      s.current!.apply([
        const Event(91, 91, AgentStateChanged('a', AgentPhase.done)),
        const Event(
          92,
          92,
          AgentChanged(
            'a',
            Execution(harness: 'axiom', provider: 'chatgpt', model: 'gpt-5.6', connection: 'c7'),
            Access.ask,
          ),
        ),
      ]);
      s.notifyListeners();
      await settled(t);
      expect(find.text('Using ChatGPT plan'), findsOneWidget);
      await snap(t, 'plan-in-use');
      await press(t, find.text('Manage usage'));
      expect(opened, ['https://chatgpt.com/settings/usage']);
      can = false;
      await press(t, find.text('Manage usage'));
      expect(find.textContaining('https://chatgpt.com/settings/usage'), findsOneWidget);
      can = true;
      await press(t, find.text('Manage usage'));
      expect(find.textContaining('https://chatgpt.com'), findsNothing);
      // At the limit it is the first thing to do.
      s.current!.apply([
        const Event(93, 93, RunStarted('r2', 'a')),
        const Event(
          94,
          94,
          RunEnded('r2', Outcome.failed, 'billing (429): You have reached your usage limit.'),
        ),
      ]);
      s.notifyListeners();
      await settled(t);
      final manage = find.widgetWithText(AxButton, 'Manage usage');
      await snap(t, 'plan-used-up');
      expect(manage, findsOneWidget);
      expect(t.widget<AxButton>(manage).tone, AxTone.primary);
    });

    test('only OpenAI\'s own pages are opened', () async {
      expect(await browse(planUsage), isTrue);
      for (final bad in [
        'http://auth.openai.com/x',
        'https://auth.openai.com.evil.example/x',
        'https://evil.example/auth.openai.com',
        'https://auth.openai.com@evil.example/',
        'https://u:p@auth.openai.com/',
        'https://auth.openai.com:444/',
        'file:///etc/passwd',
        'javascript:alert(1)',
        'auth.openai.com',
        ':',
      ]) {
        expect(await browse(bad), isFalse, reason: bad);
      }
      expect(opened, [planUsage]);
      opens = (u) => throw PlatformException(code: 'x');
      expect(await browse(planUsage), isFalse);
    });
  });

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

  const handy = [
    HarnessInfo(
      'claude',
      'Claude Code',
      program: '/usr/local/bin/claude',
      access: [Access.read, Access.work],
      hands: true,
    ),
  ];

  testWidgets('a program can work with Axiom\'s tools in place of its own', (t) async {
    final s = await shell(t);
    s.harnesses = handy;
    await press(t, find.bySemanticsLabel('New session').first);
    await press(t, find.bySemanticsLabel('Runs with: Axiom'));
    await press(t, find.bySemanticsLabel('Claude Code').last);
    // Contained by Axiom unless its person says otherwise, and with every access.
    expect(find.bySemanticsLabel('Tools: Axiom\'s'), findsOneWidget);
    expect(find.bySemanticsLabel('Access: Ask first'), findsOneWidget);
    expect(check('Let the folder'), findsNothing, reason: 'the folder\'s settings have no say');
    await snap(t, 'start-hands');
    await press(t, find.bySemanticsLabel('Tools: Axiom\'s'));
    await press(t, find.bySemanticsLabel('Its own').last);
    expect(find.bySemanticsLabel('Access: Only look'), findsOneWidget, reason: 'it cannot ask');
    expect(check('Let the folder'), findsOneWidget);
    await press(t, find.bySemanticsLabel('Tools: Its own'));
    await press(t, find.bySemanticsLabel('Axiom\'s').last);
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
      'axiom',
      null,
      Access.read,
    ]);
  });

  testWidgets('giving a program its own tools says what that means', (t) async {
    final s = await shell(t, session: working());
    s.harnesses = handy;
    void set(Access access) {
      const x = Execution(harness: 'claude', environment: 'axiom');
      s.current!.apply([
        Event(s.current!.seq + 1, 9, const AgentStateChanged('a', AgentPhase.done)),
        Event(s.current!.seq + 2, 9, AgentChanged('a', x, access)),
      ]);
      s.notifyListeners();
    }

    set(Access.ask);
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    await press(t, find.bySemanticsLabel('Tools: Axiom\'s'));
    expect(find.text('it has no mode for this access'), findsOneWidget);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    set(Access.work);
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    await press(t, find.bySemanticsLabel('Tools: Axiom\'s'));
    await press(t, find.bySemanticsLabel('Its own').last);
    expect(find.textContaining('outside Axiom\'s sandbox and without'), findsOneWidget);
    await t.pump(AxMotion.settle);
    await press(t, find.text('Change to Its own'));
    expect(s.calls, [
      ['set', 's', 'a', null, null, null],
    ]);
  });

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
    await press(t, check('Let the folder'));
    expect(start().onPressed, isNotNull);
    // Another has not been said yes for.
    await press(t, find.bySemanticsLabel('Runs with: Goose'));
    await press(t, find.bySemanticsLabel('Claude Code').last);
    expect(t.widget<AxCheck>(check('Let the folder')).value, isFalse);
    expect(
      t
          .widget<AxField>(find.byWidgetPredicate((w) => w is AxField && w.label == 'Model name'))
          .controller!
          .text,
      isEmpty,
    );
    await press(t, find.bySemanticsLabel('Runs with: Claude Code'));
    await press(t, find.bySemanticsLabel('Goose').last);
    await press(t, check('Let the folder'));
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

  testWidgets('a no can say why, and is never a yes by that', (t) async {
    final s = await shell(t, session: working(asks: true));
    await settled(t);
    await press(t, find.text('Say why not'));
    expect(find.text('Allow once'), findsNothing);
    await t.enterText(field('Why not'), 'use the Makefile');
    await snap(t, 'why-not');
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    expect(find.text('Allow once'), findsOneWidget);
    await press(t, find.text('Say why not'));
    expect(s.calls, isEmpty);
    await t.enterText(field('Why not'), 'use the Makefile');
    await press(t, find.text('Deny'));
    expect(s.calls, [
      ['answer', 's', 'q1', 'd-q1', Answer.deny, 'use the Makefile'],
    ]);
    // The next question is another: nothing of it has been said.
    expect(field('Why not'), findsNothing);
    await press(t, find.text('Say why not'));
    expect(t.widget<EditableText>(field('Why not')).controller.text, isEmpty);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    expect(s.calls.last, ['answer', 's', 'q2', 'd-q2', Answer.deny]);
  });

  testWidgets('how hard it tries is chosen where that can be said', (t) async {
    final s = await shell(t, session: working());
    s.known = const [ModelInfo('claude-opus-5-5', effort: true), ModelInfo('plain')];
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    await press(t, find.bySemanticsLabel('Effort: Usual effort'));
    await press(t, find.bySemanticsLabel('High effort').last);
    expect(s.calls.last, ['set', 's', 'a', 'claude-opus-5-5', 'c1', 'high', null]);
    // A new one, and with a model that takes none.
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('What should be done'), 'Do it.');
    await t.enterText(field('Folder'), '/w');
    expect(find.bySemanticsLabel(RegExp('^Effort')), findsNothing);
    await press(t, find.bySemanticsLabel('Model: none'));
    await press(t, find.bySemanticsLabel('claude-opus-5-5').last);
    await press(t, find.bySemanticsLabel('Effort: Usual effort'));
    await press(t, find.bySemanticsLabel('Low effort').last);
    await press(t, find.text('Start'));
    expect(s.calls.firstWhere((c) => c.first == 'add'), contains('effort low'));
    await press(t, find.bySemanticsLabel('New session').first);
    await press(t, find.bySemanticsLabel(RegExp('^Model: ')));
    await press(t, find.bySemanticsLabel('plain').last);
    expect(find.bySemanticsLabel(RegExp('^Effort')), findsNothing);
    await t.enterText(field('What should be done'), 'Do more.');
    await settled(t);
    s.calls.clear();
    await press(t, find.text('Start'));
    expect(
      s.calls.firstWhere((c) => c.first == 'add' || c.first == 'set').join(' '),
      isNot(contains('low')),
    );
  });

  testWidgets('what is said about a line goes into the message, and is not sent', (t) async {
    final s = await shell(t, session: working());
    Changes now(String line) => Changes(
      repository: true,
      files: const [Changed('lib/cache.dart', 'changed')],
      diff: '--- lib/cache.dart\n+++ lib/cache.dart\n@@ -4,2 +4,2 @@\n a\n-old\n+$line\n',
    );
    s.now = now('fresh');
    await t.tap(find.text('Changes').last);
    await settled(t);
    await press(t, find.textContaining('fresh', findRichText: true));
    expect(
      find.byWidgetPredicate((w) => w is AxField && w.label == 'About line 5'),
      findsOneWidget,
    );
    await t.enterText(field('About line 5'), 'why not const?');
    await snap(t, 'remark');
    // Looking again loses nothing; also when the line is not there any more.
    await press(t, find.text('Look again'));
    expect(t.widget<EditableText>(field('About line 5')).controller.text, 'why not const?');
    s.now = now('other');
    await press(t, find.text('Look again'));
    expect(t.widget<EditableText>(field('About line 5')).controller.text, 'why not const?');
    await press(t, find.text('Add to your message'));
    expect(find.text('In your message'), findsOneWidget);
    expect(s.calls.where((c) => c.first == 'prompt'), isEmpty);
    // By the keyboard, about the file.
    await press(t, find.bySemanticsLabel('Say something about lib/cache.dart'));
    await t.enterText(field('About the file'), 'and test it');
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    await t.tap(find.text('Conversation').last);
    await settled(t);
    final said = t
        .widgetList<EditableText>(find.byType(EditableText))
        .map((e) => e.controller.text)
        .firstWhere((x) => x.isNotEmpty);
    expect(
      said,
      'On lib/cache.dart, line 5:\n    + fresh\nwhy not const?\n\n'
      'On lib/cache.dart:\nand test it',
    );
  });

  group('what needs its person', () {
    const here = Waits(
      's',
      'Fix the cache',
      'a',
      'Lead',
      'q1',
      'asks to run a command',
      'cargo test',
      1,
    );
    const there = Waits('s2', 'Other work', 'b', 'Ada', 'q2', 'asks to write a file', 'a.txt', 2);

    testWidgets('is one list over all sessions, and leads to where it is asked', (t) async {
      final s = await shell(t, session: working(asks: true));
      expect(find.textContaining('Needs you'), findsNothing);
      s
        ..needs = const [here, there]
        ..notifyListeners();
      await settled(t);
      expect(find.text('Needs you · 2'), findsOneWidget);
      expect(find.text('Ada asks to write a file'), findsOneWidget);
      expect(
        find.bySemanticsLabel('Ada asks to write a file: a.txt, in Other work'),
        findsOneWidget,
      );
      await snap(t, 'needs-you');
      // From another page and another tab, in the session that is open.
      s.current!.apply([const Event(91, 91, AgentAdded('z', 'Zoe', x, null, access: Access.ask))]);
      await settled(t);
      await t.tap(find.bySemanticsLabel(RegExp('^Zoe')).first);
      await settled(t);
      await t.tap(find.text('Changes').last);
      await settled(t);
      await t.tap(find.bySemanticsLabel('Accounts and servers'));
      await settled(t);
      s.calls.clear();
      await press(t, find.text('Lead asks to run a command'));
      expect(s.calls, isEmpty, reason: 'it is open already');
      expect(find.byType(AccountsPage), findsNothing);
      expect(find.textContaining('to Lead'), findsWidgets);
      expect(find.byType(RequestBar), findsOneWidget);
      // In another session: that is opened, and its agent is the one that is shown.
      await press(t, find.text('Ada asks to write a file'));
      expect(s.calls, [
        ['open', 's2'],
      ]);
      s
        ..current = (SessionModel('s2')
          ..apply(const [
            Event(1, 1, SessionCreated('Other work', '/o')),
            Event(2, 2, AgentAdded('c', 'Cleo', x, null, access: Access.ask)),
            Event(3, 3, AgentAdded('b', 'Ada', x, null, access: Access.ask)),
          ]))
        ..needs = const [here]
        ..notifyListeners();
      await settled(t);
      expect(find.text('Say something to Ada'), findsOneWidget);
      expect(find.text('Needs you'), findsOneWidget);
      s
        ..needs = const []
        ..notifyListeners();
      await settled(t);
      expect(find.textContaining('Needs you'), findsNothing);
    });

    testWidgets('a session that is opened by hand shows its first agent', (t) async {
      final s = await shell(t, session: working());
      s
        ..sessions = const []
        ..needs = const [there]
        ..notifyListeners();
      await settled(t);
      await press(t, find.text('Ada asks to write a file'));
      // It was not opened: what was wanted then is not shown some other time.
      s
        ..current = (SessionModel('s3')
          ..apply(const [
            Event(1, 1, SessionCreated('Third', '/o')),
            Event(2, 2, AgentAdded('c', 'Cleo', x, null, access: Access.ask)),
            Event(3, 3, AgentAdded('b', 'Ada', x, null, access: Access.ask)),
          ]))
        ..notifyListeners();
      await settled(t);
      expect(find.text('Say something to Cleo'), findsOneWidget);
    });
  });

  group('an agent that works in a copy', () {
    const some = '--- x\n+++ x\n@@ -1 +1 @@\n-old\n+fresh\n';
    const files = [
      CopyFile(
        'lib/cache.dart',
        'changed',
        digest: 'd1',
        there: 'f1',
        diff: '--- lib/cache.dart\n+++ lib/cache.dart\n@@ -1 +1 @@\n-old\n+fresh\n',
      ),
      CopyFile('Makefile', 'changed', digest: 'd2', there: 'f2', runs: true, diff: some),
      CopyFile('both.txt', 'changed', digest: 'd3', clash: true, others: ['Ben']),
      CopyFile('.envrc', 'new', digest: 'd4', never: 'a shell runs what is written there'),
      CopyFile(
        'gone.txt',
        'removed',
        there: 'f5',
        diff: '--- gone.txt\n+++ (removed)\n@@ -1 +0,0 @@\n-was here\n',
      ),
      CopyFile('logo.png', 'changed', digest: 'd6', there: 'f6'),
      CopyFile('told.txt', 'changed', digest: 'd7', there: 'f7', unsettled: true, diff: some),
    ];
    const all = CopyChanges(made: true, files: files, ignored: 2, odd: 1);
    Future<Asked> shown(WidgetTester t, {CopyChanges copy = all}) async {
      final s = await shell(t, session: working());
      s.copy = copy;
      s.current!.apply([
        const Event(90, 90, AgentStateChanged('a', AgentPhase.done)),
        const Event(91, 91, AgentAdded('b', 'Ada', x, null, access: Access.work, apart: true)),
      ]);
      await settled(t);
      await t.tap(find.bySemanticsLabel(RegExp('^Ada')).first);
      await settled(t);
      await t.tap(find.text('Changes').last);
      await settled(t);
      return s;
    }

    AxCheck box(WidgetTester t, String f) =>
        t.widget<AxCheck>(find.byWidgetPredicate((w) => w is AxCheck && w.label == 'Bring in $f'));

    testWidgets('its person sees what it changed and brings in what they choose', (t) async {
      final s = await shown(t);
      expect(s.calls, [
        ['copy', 's', 'b'],
      ]);
      expect(find.text('7 files in its own copy'), findsOneWidget);
      for (final (f, on, can) in [
        ('lib/cache.dart', true, true),
        ('Makefile', false, true),
        ('both.txt', false, false),
        ('.envrc', false, false),
        ('gone.txt', true, true),
        ('logo.png', false, true),
        ('told.txt', false, true),
      ]) {
        expect((box(t, f).value, box(t, f).onChanged != null), (on, can), reason: f);
      }
      for (final words in [
        'Never brought in: a shell runs what is written there.',
        'The folder\'s own has changed too. Bring the copy up to date first.',
        'It runs when someone builds, tests or installs.',
        'Also changed by Ben.',
        'It cannot be shown: too large, or not text.',
        'Not listed: 2 that git ignores and only the copy has. 1 could not be copied or listed.',
      ]) {
        expect(find.text(words), findsOneWidget, reason: words);
      }
      expect(find.textContaining('Ada has not looked at yours yet'), findsOneWidget);
      expect(find.textContaining('was here', findRichText: true), findsWidgets);
      await snap(t, 'copy');
      await press(t, find.bySemanticsLabel('Bring in Makefile'));
      await press(t, find.bySemanticsLabel('Bring in gone.txt'));
      await press(t, find.bySemanticsLabel('Bring in both.txt'));
      s
        ..calls.clear()
        ..moved = const Moved(
          ['lib/cache.dart'],
          [('Makefile', 'it is not what was shown any more')],
        )
        ..copy = const CopyChanges(made: true, back: ('q1', 1));
      await press(t, find.text('Bring in 2 files'));
      expect(s.calls, [
        ['bring', ('lib/cache.dart', 'd1'), ('Makefile', 'd2')],
        ['copy', 's', 'b'],
      ]);
      expect(find.textContaining('Brought in 1 file.'), findsOneWidget);
      expect(find.textContaining('Makefile: it is not what was shown any more.'), findsOneWidget);
      expect(find.text('The folder has all of it.'), findsOneWidget);
      await snap(t, 'copy-brought');
      s
        ..moved = const Moved(['lib/cache.dart'], [])
        ..copy = all;
      await press(t, find.text('Put back 1 file'));
      expect(s.calls, contains(equals(['back', 's', 'b', 'q1'])));
      expect(find.textContaining('Put back 1 file.'), findsOneWidget);
      // As usual again: what was chosen was for the last time.
      expect((box(t, 'Makefile').value, box(t, 'gone.txt').value), (false, true));
      // What was chosen was chosen of what was shown: another content has not been.
      box(t, 'Makefile').onChanged!(true);
      box(t, 'lib/cache.dart').onChanged!(false);
      await settled(t);
      expect((box(t, 'Makefile').value, box(t, 'lib/cache.dart').value), (true, false));
      await press(t, find.text('Look again'));
      expect((box(t, 'Makefile').value, box(t, 'lib/cache.dart').value), (true, false));
      s.copy = const CopyChanges(
        made: true,
        files: [
          CopyFile('lib/cache.dart', 'changed', digest: 'd1', there: 'f1', runs: true, diff: some),
          CopyFile('Makefile', 'changed', digest: 'other', there: 'f2', runs: true, diff: some),
        ],
      );
      await press(t, find.text('Look again'));
      expect((box(t, 'Makefile').value, box(t, 'lib/cache.dart').value), (false, false));
    });

    testWidgets('it is brought up to date, and told of clashes by its person', (t) async {
      final s = await shown(t);
      s.renewed = (
        2,
        'The project has changed: both.txt.',
        const [('out/x', 'something is in the way')],
      );
      await press(t, find.text('Bring it up to date'));
      expect(find.textContaining('Took over 2 files.'), findsOneWidget);
      expect(find.textContaining('nothing is sent yet'), findsOneWidget);
      expect(find.textContaining('out/x: something is in the way.'), findsOneWidget);
      expect(s.calls.where((c) => c.first == 'prompt'), isEmpty);
      await t.tap(find.text('Conversation').last);
      await settled(t);
      expect(find.text('The project has changed: both.txt.'), findsOneWidget);
    });

    testWidgets('it is thrown away with a second word, and never while it is worked in', (t) async {
      final s = await shown(t);
      await press(t, find.text('Throw it away'));
      expect(
        find.text(
          'Only it has these, and they would be lost: 7 files, 2 that git ignores, 1 that cannot be '
          'listed.',
        ),
        findsOneWidget,
      );
      await snap(t, 'copy-throw-away');
      await press(t, find.text('Keep it'));
      expect(s.calls.where((c) => c.first == 'throw away'), isEmpty);
      await press(t, find.text('Throw it away'));
      s.copy = const CopyChanges();
      await press(t, find.text('Throw the copy away'));
      expect(s.calls.where((c) => c.first == 'throw away').single, ['throw away', 's', 'b', true]);
      expect(find.textContaining('It is made when you first speak to it.'), findsOneWidget);
      s.copy = const CopyChanges(made: true, ignored: 3);
      await press(t, find.text('Look again'));
      expect(find.text('The folder has all that can be listed.'), findsOneWidget);
      await press(t, find.text('Throw it away'));
      expect(find.textContaining('would be lost: 3 that git ignores.'), findsOneWidget);
      await press(t, find.text('Keep it'));
      s.copy = const CopyChanges(made: true, whole: false, files: files);
      await press(t, find.text('Look again'));
      await press(t, find.text('Throw it away'));
      expect(find.textContaining('would be lost: more than 7 files.'), findsOneWidget);
      await press(t, find.text('Keep it'));
      s.copy = const CopyChanges(made: true, back: ('q1', 2));
      await press(t, find.text('Look again'));
      await press(t, find.text('Throw it away'));
      expect(
        find.text(
          'The folder has all that was changed in it. What was brought in last could not be put back '
          'any more.',
        ),
        findsOneWidget,
      );
      await press(t, find.text('Throw the copy away'));
      expect(s.calls.where((c) => c.first == 'throw away').last.last, isFalse);
      // At work.
      s.copy = all;
      s.current!.apply([
        const Event(95, 95, RunStarted('r9', 'b')),
        const Event(96, 96, AgentStateChanged('b', AgentPhase.running)),
      ]);
      await settled(t);
      await press(t, find.text('Look again'));
      for (final l in ['Throw it away', 'Bring it up to date']) {
        expect(t.widget<AxButton>(find.widgetWithText(AxButton, l)).onPressed, isNull, reason: l);
      }
      expect(find.textContaining('Ada is at work'), findsOneWidget);
      // When it stops it is looked at again; what goes wrong is said.
      s.calls.clear();
      s.current!.apply([
        const Event(97, 97, RunEnded('r9', Outcome.completed, null)),
        const Event(98, 98, AgentStateChanged('b', AgentPhase.done)),
      ]);
      await settled(t);
      expect(s.calls, [
        ['copy', 's', 'b'],
      ]);
      s
        ..failure = const Failed(1, 'internal', 'the copy is elsewhere now')
        ..calls.clear();
      await press(t, find.text('Look again'));
      expect(find.text('the copy is elsewhere now'), findsOneWidget);
      // What went wrong is said, and what there is now is looked at all the same.
      s
        ..failsAt = 'bring'
        ..calls.clear()
        ..copy = const CopyChanges(made: true, files: [CopyFile('now.txt', 'new', digest: 'd')]);
      await press(t, find.textContaining('Bring in 2 files'));
      expect(s.calls.map((c) => c.first), ['bring', 'copy']);
      expect(find.text('the copy is elsewhere now'), findsOneWidget);
      expect(find.text('now.txt'), findsOneWidget);
    });

    testWidgets('is set up so beside others, and by choice', (t) async {
      final s = await shell(t, session: working());
      await press(t, find.bySemanticsLabel('Add an agent to Fix the cache'));
      AxCheck apart() => t.widget<AxCheck>(
        find.byWidgetPredicate((w) => w is AxCheck && w.label == 'Work in a copy of the folder'),
      );
      expect(apart().value, isTrue);
      await snap(t, 'add-agent-copy');
      await t.enterText(field('Name'), 'Ada');
      await press(t, find.bySemanticsLabel('Model: none'));
      await press(t, find.bySemanticsLabel('gpt-5.6').last);
      s.calls.clear();
      await press(t, find.text('Add'));
      expect(s.calls.single.last, 'apart');
      await press(t, find.bySemanticsLabel('New session').first);
      expect(apart().value, isFalse);
      // Who only looks needs none.
      await press(t, find.bySemanticsLabel(RegExp('^Access: ')));
      await press(t, find.bySemanticsLabel('Only look').last);
      expect(find.text('Work in a copy of the folder'), findsNothing);
    });

    testWidgets('fits a small window', (t) async {
      final s = Asked()
        ..copy = const CopyChanges(made: true, files: files, ignored: 2, back: ('q1', 3))
        ..current = (SessionModel('s')
          ..apply(const [
            Event(1, 1, SessionCreated('Fix the cache', '/w')),
            Event(2, 2, AgentAdded('b', 'Ada', x, null, access: Access.work, apart: true)),
          ]));
      addTearDown(s.dispose);
      await mount(t, Shell(s), size: const Size(760, 520), dpr: 1, textScale: 2);
      await settled(t);
      await press(t, find.text('Changes').last);
      expect(t.takeException(), isNull);
      await press(t, find.text('Throw it away'));
      expect(t.takeException(), isNull);
    });
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
        ..connections = const [
          Connection(
            'c1',
            'a-rather-long-name@example.com',
            'chatgpt',
            problem: 'it has ended; sign in again',
          ),
        ]
        ..signin = const Signin('n', 'https://auth.openai.com/x', 'c1');
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
