import 'dart:async';
import 'dart:ui' show Tristate;

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/context_gauge.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/patterns/models.dart';
import 'package:axiom/patterns/session_panel.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/shell/changes.dart';
import 'package:axiom/shell/drafts.dart';
import 'package:axiom/shell/model_picker.dart';
import 'package:axiom/shell/offers.dart';
import 'package:axiom/shell/outside.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/new_session.dart';
import 'package:axiom/shell/start.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/commands.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/editable.dart';
import 'package:axiom/ui/icons.dart';
import 'package:axiom/ui/marks.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:axiom/ui/press.dart';
import 'package:axiom/ui/status.dart';
import 'package:axiom/ui/surface.dart';
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
  Map<String, List<ModelInfo>> byAccount = const {};
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
    bool plan = false,
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
    if (plan) 'plan',
    if (x.speed case final v?) 'speed $v',
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
  @override
  void seen(String session, String agent) => calls.add(['seen', session, agent]);
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

  BranchInfo place = const BranchInfo(apart: false);
  @override
  Future<BranchInfo> lookBranch(String session, String agent, String item) =>
      _do(['look branch', session, agent, item], place);
  @override
  Future<String> branch(
    String session,
    String agent,
    String item,
    String name, {
    bool files = false,
  }) async {
    await _do(['branch', session, agent, item, name, if (files) 'files'], null);
    // As the host says it before it answers.
    current?.apply([
      Event(
        current!.seq + 1,
        1,
        AgentAdded(
          'a_new',
          name,
          current!.agents[agent]!.execution,
          null,
          access: Access.ask,
          apart: place.apart,
          from: Branched(agent, item),
        ),
      ),
    ]);
    return 'a_new';
  }

  /// Of each prompt.
  final keys = <String?>[];
  @override
  Future<void> answer(String session, RequestOpened r, Answer a, {String? words}) =>
      _do(['answer', session, r.request, r.digest, a, ?words], null);
  @override
  Future<void> cancel(String session, String run) => _do(['cancel', session, run], null);
  @override
  Future<void> setAgent(
    String session,
    String agent, {
    Execution? execution,
    Access? access,
    bool? plan,
  }) => _do([
    'set',
    session,
    agent,
    execution?.model,
    execution?.connection,
    ?execution?.environment,
    ?execution?.effort,
    access,
    if (plan ?? false) 'plan',
    if (execution?.speed case final v?) 'speed $v',
  ], null);
  @override
  Future<void> acceptPlan(
    String session,
    String agent,
    String item,
    Access access, {
    String? text,
  }) => _do(['accept', session, agent, item, access, ?text], null);
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
  Future<void> loadPrograms() async {}
  @override
  Future<String> setProgram({
    required List<String> args,
    required List<VarSet> vars,
    String? harness,
    String? account,
    String? name,
    String? path,
  }) => _do([
    'program',
    harness,
    account,
    name,
    path,
    args,
    for (final v in vars) (v.name, v.value, v.secret),
  ], harness ?? 'p_1');
  @override
  Future<void> removeProgram(String harness, {String? account}) =>
      _do(['remove program', harness, account], null);

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
  Future<void> loadLists() async {}

  /// What is said of one that is in no list.
  ModelInfo? told;
  ModelInfo? _toldNow;
  @override
  Future<void> about(String connection, String model) async {
    await _do(['about', connection, model], null);
    _toldNow = told;
    notifyListeners();
  }

  @override
  ModelInfo? aboutOf(String connection, String model) => _toldNow?.id == model ? _toldNow : null;
  @override
  List<ModelInfo>? modelsOf(String connection) => _seen[connection];
  @override
  Future<List<ModelInfo>> models(String connection) async {
    final r = await _do(['models', connection], byAccount[connection] ?? known);
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
  Finder field(String l) => find.descendant(
    of: find.byWidgetPredicate(
      (w) => w is AxField && w.label == l || w is AxEditable && w.label == l,
    ),
    matching: find.byType(EditableText),
  );
  Future<void> press(WidgetTester t, Finder f) async {
    await t.ensureVisible(f);
    await t.tap(f);
    await settled(t);
  }

  /// Where it is to work, as it is written.
  Future<void> folder(WidgetTester t, String p) async {
    await press(t, find.bySemanticsLabel(RegExp('^Folder: ')));
    await t.enterText(field('Folder'), p);
    await t.testTextInput.receiveAction(TextInputAction.done);
    await settled(t);
  }

  Future<void> leave(WidgetTester t) async {
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
  }

  /// Opens what it runs on; of the advanced, the one part.
  Future<void> part(WidgetTester t, String label) async {
    if (find.byType(ModelPicker).evaluate().isEmpty) {
      await press(t, find.byType(ExecutionKey));
    }
    if (label != 'Harness') return;
    final head = find.bySemanticsLabel(RegExp('^$label: '));
    if (t.getSemantics(head).flagsCollection.isExpanded != Tristate.isTrue) await press(t, head);
  }

  /// A model that is in no list.
  Future<void> named(WidgetTester t, String model) async {
    await part(t, 'Model');
    await t.enterText(field('Search models'), model);
    await settled(t);
    await t.pump(AxMotion.settle);
    await press(t, find.bySemanticsLabel('Use $model'));
    await leave(t);
  }

  /// Chooses a part of what it runs on.
  Future<void> runs(WidgetTester t, String label, String what) async {
    await part(t, label);
    await press(t, find.bySemanticsLabel(what).last);
    await leave(t);
  }

  Future<Asked> shell(
    WidgetTester t, {
    SessionModel? session,
    bool accounts = true,
    Size size = const Size(1100, 760),
    bool harnesses = false,
  }) async {
    final s = Asked()
      ..current = session
      ..connections = accounts ? const [key, Connection('c2', 'Here', 'lm-studio')] : const [];
    addTearDown(s.dispose);
    final prefs = ValueNotifier(Prefs(harnesses: harnesses));
    addTearDown(prefs.dispose);
    await mount(t, PrefsScope(prefs, child: Shell(s)), size: size, dpr: 1);
    return s;
  }

  testWidgets('work is begun: what, where, who, and how much it may do', (t) async {
    final s = await shell(t);
    await t.tap(find.bySemanticsLabel('New session').first);
    await settled(t);
    expect(s.calls, [
      ['models', 'c1'],
    ]);
    AxButton start() => t.widget<AxButton>(
      find.byWidgetPredicate((w) => w is AxButton && w.label == 'Start session'),
    );
    expect(start().onPressed, isNull, reason: 'nothing is filled in');
    await t.enterText(field('Task'), 'Make the cache forget.\nAll of it.');
    await folder(t, '/home/me/project');
    await settled(t);
    expect(start().onPressed, isNull, reason: 'no model');
    await runs(t, 'Model', 'GPT-5.6');
    await t.ensureVisible(find.bySemanticsLabel('Start session'));
    await snap(t, 'start');
    await t.tap(find.bySemanticsLabel('Start session'));
    await settled(t);
    expect(s.calls.skip(1), [
      ['create', 'Make the cache forget. All of it.', '/home/me/project'],
      ['add', 's_new', 'Lead', 'c1', 'gpt-5.6', Access.ask],
      ['open', 's_new'],
      ['prompt', 's_new', 'a_new', 'Make the cache forget.\nAll of it.'],
    ]);
    // What is open now is shown.
    s
      ..current = working()
      ..notifyListeners();
    await settled(t);
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
    await folder(t, '/none');
    expect(find.bySemanticsLabel(RegExp('^Instructions')), findsNothing);
    await folder(t, '/p');
    await press(t, find.bySemanticsLabel('Instructions: No instructions'));
    expect(find.textContaining('Use tabs'), findsNothing);
    await t.tap(find.text('Follow AGENTS.md'));
    await settled(t);
    expect(find.textContaining('Use tabs'), findsOneWidget, reason: 'what is followed is shown');
    await snap(t, 'instructions');
    await leave(t);
    expect(find.bySemanticsLabel('Instructions: AGENTS.md'), findsOneWidget);
    // Another folder's are other words.
    await folder(t, '/q');
    await press(t, find.bySemanticsLabel('Instructions: No instructions'));
    expect(t.widget<AxCheck>(check('Follow')).value, isFalse);
    await leave(t);
    await folder(t, '/p');
    await press(t, find.bySemanticsLabel('Instructions: No instructions'));
    await t.tap(find.text('Follow AGENTS.md'));
    await settled(t);
    await leave(t);
    await t.enterText(field('Task'), 'x');
    await runs(t, 'Model', 'GPT-5.6');
    await t.ensureVisible(find.bySemanticsLabel('Start session'));
    await t.tap(find.bySemanticsLabel('Start session'));
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
    // The last one is where it begins.
    await press(t, find.bySemanticsLabel('Folder: /a/one'));
    await snap(t, 'start-folder');
    await t.enterText(field('Folder'), 'tw');
    await settled(t);
    expect(find.bySemanticsLabel('one'), findsNothing);
    await press(t, find.bySemanticsLabel('two'));
    expect(find.bySemanticsLabel('Folder: /b/two'), findsOneWidget);
    expect(wholePath('/x/~/y'), '/x/~/y');
    expect(wholePath('~x'), '~x');
    expect(wholePath('~/x'), isNot(startsWith('~')));
    expect(wholePath('~/x'), endsWith('/x'));
  });

  testWidgets('what fails is said, keeps what was written, and is not made twice', (t) async {
    final s = await shell(t);
    await t.tap(find.bySemanticsLabel('New session').first);
    await settled(t);
    await t.enterText(field('Task'), 'x');
    await folder(t, '/w');
    await named(t, 'zz-own');
    s.failure = const Failed(1, 'bad_request', '/w: not a folder');
    await t.ensureVisible(find.bySemanticsLabel('Start session'));
    await t.tap(find.bySemanticsLabel('Start session'));
    await settled(t);
    expect(find.textContaining('not a folder'), findsOneWidget);
    expect(find.bySemanticsLabel('Folder: /w'), findsOneWidget);
    expect(find.text('x'), findsOneWidget);
    s
      ..failure = null
      ..calls.clear();
    await t.ensureVisible(find.bySemanticsLabel('Start session'));
    await t.tap(find.bySemanticsLabel('Start session'));
    await settled(t);
    expect(s.calls.map((c) => c.first), ['create', 'add', 'open', 'prompt']);
  });

  testWidgets('a question is answered once, with what it showed', (t) async {
    final s = await shell(t, session: working(asks: true));
    s.known = const [ModelInfo('claude-opus-5-5', name: 'Claude Opus 5.5 Preview')];
    await t.runAsync(() => s.models('c1'));
    s.calls.clear();
    await settled(t);
    expect(
      find.descendant(of: find.byType(AgentRow), matching: find.textContaining('Opus 5.5 Preview')),
      findsOneWidget,
    );
    expect(find.byType(RequestBar), findsOneWidget);
    expect(find.text('1 more waiting'), findsOneWidget);
    expect(find.text('cargo test'), findsWidgets);
    await settled(t);
    final row = find.byType(AgentRow);
    expect(
      find.descendant(of: row, matching: find.textContaining('Asks to run a command')),
      findsOneWidget,
    );
    expect(find.descendant(of: row, matching: find.text('shell')), findsNothing);
    expect(
      t
          .widgetList<Text>(
            find.descendant(of: find.byType(RequestBar), matching: find.text('cargo test')),
          )
          .single
          .style!
          .color,
      AxColors.dark.ink1,
    );
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
    await t.tap(find.text('Allow for this session'));
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
    await t.tap(find.bySemanticsLabel('Stop'));
    await settled(t);
    expect(s.calls.last, ['cancel', 's', 'r']);
    s.current!.apply([
      const Event(50, 50, RunEnded('r', Outcome.failed, 'the provider declined')),
      const Event(51, 51, AgentStateChanged('a', AgentPhase.failed)),
    ]);
    await settled(t);
    expect(find.bySemanticsLabel('Stop'), findsNothing);
    expect(find.textContaining('declined'), findsOneWidget);
  });

  testWidgets('what it runs on is changed, with its cost said first', (t) async {
    final s = await shell(t, session: working());
    await t.runAsync(() => s.models('c1'));
    s.calls.clear();
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    await t.tap(find.bySemanticsLabel('GPT-5.6').last);
    await settled(t);
    expect(s.calls, isEmpty);
    expect(find.textContaining('has to be read again'), findsOneWidget);
    await t.pump(AxMotion.settle);
    await t.tap(find.text('Change to GPT-5.6'));
    await settled(t);
    expect(s.calls, [
      ['set', 's', 'a', 'gpt-5.6', 'c1', null],
    ]);
    await leave(t);
    await t.tap(find.bySemanticsLabel('Access: Ask first'));
    await settled(t);
    await t.tap(find.bySemanticsLabel(RegExp('Work without asking')).last);
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
    expect(find.textContaining('Haiku 5.5', findRichText: true), findsWidgets);
    // An answer that broke off says nothing of it.
    s.current!.apply([
      Event(62, 62, UsageReported('a', Usage.decode({'input': 1}), Known.reported, cut: true)),
    ]);
    expect(s.current!.agents['a']!.context, .42);
    // What was summarised is as full as nobody has said yet.
    s.current!.apply([
      Event(63, 63, UsageReported('a', Usage.decode({'input': 1}), Known.reported, aside: true)),
    ]);
    expect(s.current!.agents['a']!.context, null);
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
    // It is named by its files, and not by its text.
    expect(find.text('a.txt, b.txt'), findsOneWidget);
    expect(find.text('…'), findsNothing);
    await t.tap(find.text('Edited'));
    await settled(t);
    expect(find.byType(DiffLines), findsNWidgets(2));
    expect(find.text('b.txt'), findsOneWidget);
    expect(find.textContaining('two', findRichText: true), findsOneWidget);
    expect(find.textContaining(r'\u{202E}', findRichText: true), findsOneWidget);
    expect(
      find.descendant(of: find.byType(AgentRow), matching: find.textContaining('Edited 2 files')),
      findsOneWidget,
    );
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
    expect(find.bySemanticsLabel('Stop'), findsNothing);
    expect(find.textContaining('cargo bench'), findsOneWidget, reason: 'what was asked is told');
  });

  testWidgets('accounts are added and removed, and a key is No preview', (t) async {
    final s = await shell(t, accounts: false);
    await t.tap(find.bySemanticsLabel('Accounts').first);
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
    await t.tap(find.bySemanticsLabel('Accounts').first);
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

  testWidgets('a key for Bedrock has a region', (t) async {
    final s = await shell(t, accounts: false);
    await t.tap(find.bySemanticsLabel('Accounts').first);
    await settled(t);
    Finder field(String l) => find.descendant(
      of: find.byWidgetPredicate((w) => w is AxField && w.label == l),
      matching: find.byType(EditableText),
    );
    expect(field('Region'), findsNothing);
    await t.tap(find.bySemanticsLabel(RegExp('^Add: ')));
    await settled(t);
    await t.tap(find.bySemanticsLabel('Amazon Bedrock').last);
    await settled(t);
    expect(field('Address'), findsNothing);
    expect(
      find.text('An API key from the Amazon Bedrock console. It goes to the system\'s store.'),
      findsOneWidget,
    );
    Future<void> add() async {
      await t.tap(find.widgetWithText(AxButton, 'Add'));
      await settled(t);
    }

    // Each of the three is needed.
    const all = {'Name': 'Work', 'Region': 'eu-west-2', 'Key': 'sk-secret'};
    for (final without in all.keys) {
      for (final MapEntry(:key, :value) in all.entries) {
        await t.enterText(field(key), key == without ? ' ' : value);
      }
      await settled(t);
      await add();
      expect(s.calls, isEmpty, reason: without);
    }
    await t.enterText(field('Key'), 'sk-secret');
    await settled(t);
    await snap(t, 'accounts-bedrock');
    await add();
    expect(s.calls.single, [
      'connect',
      'Work',
      'bedrock',
      'https://bedrock-runtime.eu-west-2.amazonaws.com',
      'sk-secret',
    ]);
    expect(t.widget<EditableText>(field('Region')).controller.text, isEmpty);
    expect(t.widget<EditableText>(field('Key')).controller.text, isEmpty);
    expect(
      find.textContaining('Amazon Bedrock · https://bedrock-runtime.eu-west-2'),
      findsOneWidget,
    );
    expect(markOf('bedrock', null), ServiceMark.bedrock);
    // Whose model it is says more than where it runs.
    expect(markOf('bedrock', 'claude-opus-5-5'), ServiceMark.claude);
    // Not the next account's.
    await t.tap(find.bySemanticsLabel(RegExp('^Add: ')));
    await settled(t);
    await t.tap(find.bySemanticsLabel('Anthropic API').last);
    await settled(t);
    expect(field('Region'), findsNothing);
  });

  testWidgets('fits a small window at twice the text', (t) async {
    final s = Asked()
      ..current = working(asks: true)
      ..connections = const [key];
    addTearDown(s.dispose);
    await mount(t, Shell(s), size: const Size(760, 560), dpr: 1, textScale: 1.6);
    await settled(t);
    expect(t.takeException(), isNull);
    for (final l in [RegExp(r'^New session$'), RegExp('^Accounts')]) {
      await t.tap(find.bySemanticsLabel(l).first);
      await settled(t);
      expect(t.takeException(), isNull, reason: '$l');
    }
  });

  /// Everything but the start itself.
  Future<void> fill(WidgetTester t, String say) async {
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('Task'), say);
    await folder(t, '/w');
    await settled(t);
    await runs(t, 'Model', 'GPT-5.6');
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
      await t.tap(find.bySemanticsLabel('Accounts').first);
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
      expect(find.text('Continue in your browser'), findsOneWidget);
      final again = t.widget<AxButton>(find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      expect(again.onPressed, isNull, reason: 'one at a time');
      await snap(t, 'accounts-plan-waits');
      await press(t, find.widgetWithText(AxButton, 'Open again'));
      expect((opened.length, s.calls.length), (2, 1));
      String? copied;
      t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (m) async {
        if (m.method == 'Clipboard.setData') copied = (m.arguments as Map)['text'] as String;
        return null;
      });
      await press(t, find.widgetWithText(AxButton, 'Copy link'));
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
      expect(find.text('Continue in your browser'), findsNothing);
      // What cannot be opened from here is opened by hand.
      can = false;
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      expect(find.text('Open the link in your browser'), findsOneWidget);
      s.toldOfSignin({'state': 'failed', 'problem': 'the sign-in was not allowed'});
      await settled(t);
      expect(find.text('the sign-in was not allowed'), findsOneWidget);
      await press(t, find.widgetWithText(AxButton, 'Close'));
      expect(find.text('You are not signed in'), findsNothing);
      // Another page, and back: it is on still.
      await press(t, find.widgetWithText(AxButton, 'Continue with ChatGPT'));
      await press(t, find.bySemanticsLabel('New session').first);
      await press(t, find.bySemanticsLabel('Accounts').first);
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
      expect(find.text('Continue in your browser'), findsOneWidget);
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
      // Seen as failed wherever the agent is, and said once.
      expect(find.textContaining('Stopped with an error'), findsOneWidget);
      expect(find.textContaining('You have reached your usage limit'), findsOneWidget);
      expect(
        t.widgetList<AxLamp>(find.byType(AxLamp)).where((l) => l.state == AgentState.failed),
        hasLength(2),
      );
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
    final s = await shell(t, accounts: false, harnesses: true);
    s.harnesses = harnesses;
    await press(t, find.bySemanticsLabel('New session').first);
    expect(find.text('Add an account or server'), findsOneWidget);
    await part(t, 'Harness');
    expect(find.text('not on this computer'), findsOneWidget, reason: 'Codex is not there');
    expect(find.bySemanticsLabel('Qwen Code'), findsNothing);
    await t.tap(find.text('Codex'), warnIfMissed: false);
    await settled(t);
    expect(find.bySemanticsLabel('Harness: Axiom'), findsOneWidget, reason: 'nothing happens');
    await runs(t, 'Harness', 'Claude Code');
    // No account is needed, and what it has no mode for is not offered.
    expect(find.text('Add an account or server'), findsNothing);
    expect(find.bySemanticsLabel('Access: Read only'), findsOneWidget, reason: 'the least');
    await press(t, find.bySemanticsLabel('Access: Read only'));
    expect(find.text('Claude Code has no mode for it'), findsOneWidget);
    await press(t, find.bySemanticsLabel('Work without asking').last);
    await t.enterText(field('Task'), 'Do it.');
    await folder(t, '/w');
    await settled(t);
    await snap(t, 'start-harness');
    await press(t, find.bySemanticsLabel('Start session'));
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

  group('programs', () {
    Future<Asked> page(WidgetTester t, {List<ProgramInfo> said = const []}) async {
      final s = await shell(t, size: const Size(1100, 1500));
      s
        ..harnesses = harnesses
        ..programs = said;
      await press(t, find.bySemanticsLabel('Accounts').first);
      s.calls.clear();
      return s;
    }

    testWidgets('one of their own is added', (t) async {
      final s = await page(t);
      await press(t, find.bySemanticsLabel(RegExp('^Add a program')));
      await press(t, find.bySemanticsLabel('One of your own').last);
      expect(find.textContaining('it can only ask first'), findsOneWidget);
      AxButton add() => t.widget(find.widgetWithText(AxButton, 'Add program'));
      expect(add().onPressed, isNull, reason: 'it has neither name nor place');
      await t.enterText(field('Program name'), 'Kiro');
      files = (at) async => '/opt/kiro/bin/kiro';
      await press(t, find.text('Choose…'));
      await t.enterText(field('Arguments'), 'acp\n\n --quiet \n');
      await settled(t);
      await snap(t, 'programs-own');
      s.failure = const Failed(1, 'bad_request', 'another program has this name');
      await press(t, find.widgetWithText(AxButton, 'Add program'));
      expect(find.text('another program has this name'), findsOneWidget);
      s.failure = null;
      await press(t, find.widgetWithText(AxButton, 'Add program'));
      expect(s.calls.last, [
        'program',
        null,
        null,
        'Kiro',
        '/opt/kiro/bin/kiro',
        ['acp', '--quiet'],
      ]);
      expect(field('Program name'), findsNothing, reason: 'it is done');
    });

    testWidgets('how a known one is started, with what stays secret', (t) async {
      final s = await page(
        t,
        said: const [
          ProgramInfo(
            'claude',
            vars: [
              VarInfo('AWS_REGION', value: 'eu-west-1'),
              VarInfo('X_TOKEN'),
            ],
          ),
        ],
      );
      expect(find.textContaining('2 variables'), findsOneWidget);
      await press(t, find.widgetWithText(AxButton, 'Edit').first);
      expect(field('Program name'), findsNothing, reason: 'it has its own');
      await press(t, find.widgetWithText(AxButton, 'Add a variable'));
      await t.enterText(field('Variable').last, 'AWS_BEARER_TOKEN_BEDROCK');
      await settled(t);
      // Its name says what it is: it is not read over a shoulder.
      expect(t.widget<EditableText>(field('Value').last).obscureText, isTrue);
      expect(t.widget<AxCheck>(find.byType(AxCheck).last).onChanged, isNull);
      await t.enterText(field('Value').last, 'abcdefgh');
      await press(t, find.widgetWithText(AxButton, 'Add a variable'));
      await t.enterText(field('Variable').last, 'MODE');
      await t.enterText(field('Value').last, 'x');
      await settled(t);
      await snap(t, 'programs-known');
      await press(t, find.widgetWithText(AxButton, 'Save'));
      expect(s.calls.last, [
        'program',
        'claude',
        null,
        null,
        null,
        <String>[],
        ('AWS_REGION', 'eu-west-1', false),
        // Not touched, so kept.
        ('X_TOKEN', null, true),
        ('AWS_BEARER_TOKEN_BEDROCK', 'abcdefgh', true),
        ('MODE', 'x', false),
      ]);
    });

    testWidgets('of one account, and back to how it was', (t) async {
      final s = await page(
        t,
        said: const [
          ProgramInfo('claude', account: 'c9', args: ['--max-turns=9']),
        ],
      );
      s.connections = [key, work];
      s.notifyListeners();
      await settled(t);
      await press(t, find.widgetWithText(AxButton, 'Edit').first);
      expect(find.widgetWithText(AxButton, 'Reset'), findsNothing);
      await press(t, find.bySemanticsLabel('For: Every account'));
      await press(t, find.bySemanticsLabel(work.name).last);
      expect(find.text('--max-turns=9'), findsOneWidget);
      await press(t, find.widgetWithText(AxButton, 'Reset'));
      expect(s.calls.last, ['remove program', 'claude', 'c9']);
    });

    testWidgets('one of their own is removed when that is meant', (t) async {
      final s = await page(t);
      s.harnesses = const [
        HarnessInfo('p_1', 'Kiro', program: '/opt/kiro', access: [Access.ask], own: true),
      ];
      s.notifyListeners();
      await settled(t);
      await press(t, find.widgetWithText(AxButton, 'Remove').last);
      expect(s.calls, isEmpty);
      await press(t, find.widgetWithText(AxButton, 'Keep'));
      await press(t, find.widgetWithText(AxButton, 'Remove').last);
      await press(t, find.widgetWithText(AxButton, 'Remove Kiro'));
      expect(s.calls.last, ['remove program', 'p_1', null]);
      // What it was is still said where it was used.
      s.harnesses = const [];
      expect(s.harnessName('p_1'), 'a program that was removed');
    });
  });

  testWidgets('a program can have several accounts', (t) async {
    final s = await shell(t);
    s.harnesses = harnesses;
    await press(t, find.bySemanticsLabel('Accounts').first);
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
    s
      ..connections = [key, work]
      ..lists = const [
        ModelList('claude', null, 1, [ModelInfo('m1', name: 'Everywhere')]),
        ModelList('claude', 'c9', 1, [ModelInfo('m2', name: 'Only there')]),
      ];
    s.notifyListeners();
    await settled(t);
    expect(find.text(work.signIn!), findsOneWidget);
    String? copied;
    t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (m) async {
      if (m.method == 'Clipboard.setData') copied = (m.arguments as Map)['text'] as String;
      return null;
    });
    await press(t, find.text('Copy'));
    expect(copied, work.signIn);
    expect(find.text('Copied'), findsOneWidget);
    await snap(t, 'accounts-program');
    await press(t, find.widgetWithText(AxButton, 'Remove').last);
    expect(find.textContaining('does not sign it out'), findsOneWidget);
    await press(t, find.text('Keep'));
    // It is one to choose for its program, and not for Axiom's own loop.
    await press(t, find.bySemanticsLabel('New session').first);
    await part(t, 'Model');
    // The program twice, for each of its accounts, and the other program.
    expect(
      find.bySemanticsLabel(RegExp('^(Claude Code|Claude Code, Work|Goose)\$')),
      findsNWidgets(3),
    );
    await press(t, find.bySemanticsLabel('Claude Code, Work'));
    expect(find.bySemanticsLabel('Only there'), findsOneWidget, reason: 'it has said so itself');
    await press(t, find.bySemanticsLabel('Default model'));
    await leave(t);
    await t.enterText(field('Task'), 'Do it.');
    await folder(t, '/w');
    await settled(t);
    s.calls.clear();
    await press(t, find.bySemanticsLabel('Start session'));
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
    await press(
      t,
      find
          .descendant(of: find.byType(ModelPicker), matching: find.bySemanticsLabel('Claude Code'))
          .first,
    );
    await t.pump(AxMotion.settle);
    await press(t, find.bySemanticsLabel('Default model').last);
    expect(find.textContaining('begins anew there'), findsOneWidget);
    await t.pump(AxMotion.settle);
    await press(t, find.text('Change to Default model'));
    expect(s.calls, [
      ['set', 's', 'a', null, null, null],
    ]);
  });

  testWidgets('what cannot ignore a folder\'s settings needs a yes to them', (t) async {
    final s = await shell(t, harnesses: true);
    s.harnesses = harnesses;
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('Task'), 'Do it.');
    await folder(t, '/w');
    await runs(t, 'Harness', 'Goose');
    AxButton start() => t.widget<AxButton>(
      find.byWidgetPredicate((w) => w is AxButton && w.label == 'Start session'),
    );
    expect(start().onPressed, isNull);
    expect(find.textContaining('Goose cannot ignore'), findsOneWidget, reason: 'it is said why');
    Future<void> allow() async {
      await press(t, find.bySemanticsLabel('Folder settings: No folder settings'));
      await press(t, check('Use this folder'));
      await leave(t);
    }

    await named(t, 'own-model');
    expect(find.bySemanticsLabel(RegExp('^Runs on own-model, Goose')), findsOneWidget);
    await allow();
    expect(start().onPressed, isNotNull);
    // Another has not been said yes for.
    await runs(t, 'Harness', 'Claude Code');
    expect(find.bySemanticsLabel('Folder settings: No folder settings'), findsOneWidget);
    expect(find.bySemanticsLabel(RegExp('^Runs on Default model, Claude')), findsOneWidget);
    await runs(t, 'Harness', 'Goose');
    await allow();
    await press(t, find.bySemanticsLabel('Start session'));
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
    final s = await shell(t, session: working(), harnesses: true);
    s
      ..harnesses = harnesses
      ..words = const {
        '/w': [Instructions('AGENTS.md', 'd1', 'Use tabs.')],
      };
    await press(t, find.bySemanticsLabel('Add an agent to Fix the cache'));
    expect(find.text('New agent'), findsOneWidget);
    expect(
      find.bySemanticsLabel(RegExp('^Folder: ')),
      findsNothing,
      reason: 'it is the session\'s',
    );
    expect(find.bySemanticsLabel('Instructions: No instructions'), findsOneWidget);
    await t.enterText(field('Name'), 'Reviewer');
    await runs(t, 'Harness', 'Claude Code');
    await snap(t, 'add-agent');
    s.calls.clear();
    // It need not be told anything yet. While it is set up the other goes on.
    await press(t, find.bySemanticsLabel('Add agent'));
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
    expect(find.text('Message Reviewer'), findsOneWidget);
    expect(find.text('No messages yet.'), findsOneWidget);
    expect(find.byType(TranscriptView), findsNothing);
    await snap(t, 'two-agents');
    // What is selected is one raised slab, whoever it belongs to.
    expect(
      t
          .widgetList<Surface>(
            find.descendant(of: find.byType(AgentRow), matching: find.byType(Surface)),
          )
          .map((x) => x.spec),
      [AxColors.dark.slab],
    );
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(find.text('Default model'), findsWidgets);
    expect(s.calls.length, 1, reason: 'there is no account whose models could be asked for');
  });

  test('what a tool is doing is said in words', () {
    final m = SessionModel('s')
      ..apply([
        const Event(1, 1, SessionCreated('T', '/w')),
        const Event(2, 2, AgentAdded('a', 'A', x, null)),
      ]);
    var n = 2;
    String words(String tool, Object? args, {bool open = false, Object? result}) {
      final id = 'i${++n}';
      m.apply([
        Event(n, n, ItemStarted(id, 'r', 'a', ItemKind.tool, tool, args)),
        if (!open) Event(++n, n, ItemEnded(id, Outcome.completed, '', result)),
      ]);
      return toolWords(m.agents['a']!.items.last);
    }

    for (final (tool, args, open, said) in [
      ('shell', {'command': 'cargo test -q'}, true, 'Running cargo test -q'),
      ('Bash', {'command': 'cargo test -q'}, false, 'Ran cargo test -q'),
      ('read', {'path': 'lib/src/cache.dart'}, false, 'Read cache.dart'),
      ('Edit', {'file_path': '/w/lib/cache.dart'}, true, 'Editing cache.dart'),
      ('grep', {'pattern': 'a/b c'}, false, 'Searched a/b c'),
      ('fetch', {'url': 'https://example.com/a/b'}, false, 'Fetched b'),
      ('agent', {'do': 'tell', 'name': 'Ada'}, true, 'Working with another agent'),
      ('patch', {'patch': '*** Begin'}, true, 'Editing'),
      ('weird_tool', {'x': 'y'}, false, 'Weird_tool y'),
      ('list', <String, Object?>{}, false, 'Searched'),
    ]) {
      expect(words(tool, args, open: open), said, reason: tool);
    }
    const diff = '--- a.txt\n+++ a.txt\n@@ -1 +1 @@\n-a\n+b\n';
    expect(words('patch', {'patch': 'x'}, result: {'diff': diff}), 'Edited 1 file');
    expect(
      words(
        'apply_patch',
        {'patch': 'x'},
        result: {'diff': diff + diff.replaceAll('a.txt', 'b.txt')},
      ),
      'Edited 2 files',
    );
    expect(words('shell', {'command': 'x' * 400}).length, lessThan(60));
  });

  testWidgets('a no can say why, and is never a yes by that', (t) async {
    final s = await shell(t, session: working(asks: true));
    await settled(t);
    await press(t, find.text('Deny with reason'));
    expect(find.text('Allow once'), findsNothing);
    await t.enterText(field('Reason'), 'use the Makefile');
    await snap(t, 'why-not');
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    expect(find.text('Allow once'), findsOneWidget);
    await press(t, find.text('Deny with reason'));
    expect(s.calls, isEmpty);
    await t.enterText(field('Reason'), 'use the Makefile');
    await press(t, find.text('Deny'));
    expect(s.calls, [
      ['answer', 's', 'q1', 'd-q1', Answer.deny, 'use the Makefile'],
    ]);
    // The next question is another: nothing of it has been said.
    expect(field('Reason'), findsNothing);
    await press(t, find.text('Deny with reason'));
    expect(t.widget<EditableText>(field('Reason')).controller.text, isEmpty);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    expect(s.calls.last, ['answer', 's', 'q2', 'd-q2', Answer.deny]);
  });

  testWidgets('how hard it tries is chosen where that can be said', (t) async {
    final s = await shell(t, session: working());
    s.known = const [
      ModelInfo('claude-opus-5-5', efforts: ['low', 'medium', 'high']),
      ModelInfo('plain'),
    ];
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    await press(t, find.bySemanticsLabel('High').last);
    expect(s.calls.last, ['set', 's', 'a', 'claude-opus-5-5', 'c1', 'high', null]);
    // A new one, and with a model that takes none.
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    await press(t, find.bySemanticsLabel('New session').first);
    await t.enterText(field('Task'), 'Do it.');
    await folder(t, '/w');
    // As was worked last, except how much it may do.
    expect(find.bySemanticsLabel(RegExp('^Runs on Opus 5.5')), findsOneWidget);
    expect(find.bySemanticsLabel('Access: Ask first'), findsOneWidget);
    await runs(t, 'Effort', 'Low');
    await press(t, find.bySemanticsLabel('Start session'));
    expect(s.calls.firstWhere((c) => c.first == 'add'), contains('effort low'));
    await press(t, find.bySemanticsLabel('New session').first);
    await runs(t, 'Model', 'plain');
    await part(t, 'Model');
    expect(find.text('This model takes none.'), findsOneWidget);
    await leave(t);
    await t.enterText(field('Task'), 'Do more.');
    await settled(t);
    s.calls.clear();
    await press(t, find.bySemanticsLabel('Start session'));
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
    await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
    await settled(t);
    await press(t, find.bySemanticsLabel(RegExp('^lib/cache.dart')));
    await press(t, find.textContaining('fresh', findRichText: true));
    expect(
      find.byWidgetPredicate((w) => w is AxField && w.label == 'About line 5'),
      findsOneWidget,
    );
    await t.enterText(field('About line 5'), 'why not const?');
    await snap(t, 'remark');
    // Looking again loses nothing; also when the line is not there any more.
    await press(t, find.bySemanticsLabel('Refresh'));
    expect(t.widget<EditableText>(field('About line 5')).controller.text, 'why not const?');
    s.now = now('other');
    await press(t, find.bySemanticsLabel('Refresh'));
    expect(t.widget<EditableText>(field('About line 5')).controller.text, 'why not const?');
    await press(t, find.text('Add to message'));
    expect(find.text('In your message'), findsOneWidget);
    expect(s.calls.where((c) => c.first == 'prompt'), isEmpty);
    // By the keyboard, about the file.
    await press(t, find.bySemanticsLabel('Comment on lib/cache.dart'));
    await t.enterText(field('About the file'), 'and test it');
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await settled(t);
    await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
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
      expect(find.text('Needs you'), findsOneWidget);
      expect(find.text('2'), findsOneWidget);
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
      await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
      await settled(t);
      await t.tap(find.bySemanticsLabel('Accounts').first);
      await settled(t);
      s.calls.clear();
      await press(t, find.text('Lead asks to run a command'));
      expect(s.calls.where((c) => c.first == 'open'), isEmpty, reason: 'it is open already');
      expect(find.byType(AccountsPage), findsNothing);
      expect(find.text('Message Lead. Sent before its next step'), findsWidgets);
      expect(find.byType(RequestBar), findsOneWidget);
      // In another session: that is opened, and its agent is the one that is shown.
      s.calls.clear();
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
      expect(find.text('Message Ada'), findsOneWidget);
      expect(find.text('Needs you'), findsOneWidget);
      s
        ..needs = const []
        ..notifyListeners();
      await settled(t);
      expect(find.textContaining('Needs you'), findsNothing);
    });

    testWidgets('what agents came to stays until it has been looked at', (t) async {
      addTearDown(() => t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed));
      final s = await shell(t, session: working());
      const done = Waits(
        's',
        'Fix the cache',
        'a',
        'Lead',
        '',
        '',
        'All is done.',
        1,
        need: Need.finished,
      );
      const other = Waits('s2', 'Other', 'b', 'Ada', '', '', 'no such key', 2, need: Need.failed);
      const spent = Waits('s3', 'Third', 'c', 'Cleo', '', '', '', 3, need: Need.limit);
      // Nobody looks at the window.
      for (final l in [AppLifecycleState.inactive, AppLifecycleState.hidden]) {
        t.binding.handleAppLifecycleStateChanged(l);
      }
      s
        ..needs = const [other, spent, done]
        ..notifyListeners();
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await settled(t);
      expect(s.calls, isEmpty);
      expect(find.text('Needs you'), findsOneWidget);
      expect(find.text('2'), findsOneWidget);
      expect(find.text('Finished'), findsOneWidget);
      for (final l in ['Ada failed', 'Cleo has reached a limit', 'Lead has finished']) {
        expect(find.text(l), findsOneWidget, reason: l);
      }
      expect(find.bySemanticsLabel('Cleo has reached a limit, in Third'), findsOneWidget);
      await snap(t, 'needs-unseen');
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await settled(t);
      // The one that is before them; the others not.
      expect(s.calls.toSet().map((c) => c.join(' ')), {'seen s a'});
      // A page in front of it: it is not looked at.
      s
        ..calls.clear()
        ..needs = const []
        ..notifyListeners();
      await settled(t);
      await t.tap(find.bySemanticsLabel('Accounts').first);
      await settled(t);
      s
        ..needs = const [done]
        ..notifyListeners();
      await settled(t);
      expect(s.calls, isEmpty);
      await press(t, find.text('Lead has finished'));
      expect(s.calls.first, ['seen', 's', 'a']);
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
      expect(find.text('Message Cleo'), findsOneWidget);
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
      final s = await shell(t, session: working(), size: const Size(1179, 1200));
      s.copy = copy;
      s.current!.apply([
        const Event(90, 90, AgentStateChanged('a', AgentPhase.done)),
        const Event(91, 91, AgentAdded('b', 'Ada', x, null, access: Access.work, apart: true)),
      ]);
      await settled(t);
      await t.tap(find.bySemanticsLabel(RegExp('^Ada')).first);
      await settled(t);
      await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
      await settled(t);
      return s;
    }

    AxCheck box(WidgetTester t, String f) =>
        t.widget<AxCheck>(find.byWidgetPredicate((w) => w is AxCheck && w.label == 'Apply $f'));

    testWidgets('its person sees what it changed and brings in what they choose', (t) async {
      final s = await shown(t);
      expect(s.calls, [
        ['copy', 's', 'b'],
      ]);
      expect(find.text('7 files in Ada\'s copy'), findsOneWidget);
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
        'Never applied: a shell runs what is written there.',
        'Also changed in your folder. Update from folder first.',
        'Runs when someone builds, tests or installs.',
        'Also changed by Ben.',
        'No preview: too large, or not text.',
      ]) {
        expect(find.text(words), findsOneWidget, reason: words);
      }
      expect(find.textContaining('Ada has not seen yours'), findsOneWidget);
      expect(find.textContaining('was here', findRichText: true), findsWidgets);
      // What needs a look has the amber glyph; its words are words.
      Iterable<AxGlyph> warnings() => t
          .widgetList<AxGlyph>(
            find.descendant(of: find.byType(CopyView), matching: find.byType(AxGlyph)),
          )
          .where((g) => g.icon == AxIcon.warn);
      expect(warnings(), hasLength(5));
      expect(warnings().every((g) => g.color == AxColors.dark.attention), isTrue);
      expect(
        t
            .widgetList<Text>(
              find.descendant(of: find.byType(CopyView), matching: find.byType(Text)),
            )
            .where((x) => x.style?.color == AxColors.dark.attentionText),
        isEmpty,
      );
      await snap(t, 'copy');
      // The last line is under the edge.
      final list = find.descendant(of: find.byType(CopyView), matching: find.byType(ListView));
      await t.drag(list, const Offset(0, -400));
      await settled(t);
      const last =
          'Not listed: 2 that git ignores and only the copy has. 1 could not be copied or listed.';
      expect(find.text(last), findsOneWidget);
      await t.drag(list, const Offset(0, 400));
      await settled(t);
      await press(t, find.bySemanticsLabel('Apply Makefile'));
      await press(t, find.bySemanticsLabel('Apply gone.txt'));
      await press(t, find.bySemanticsLabel('Apply both.txt'));
      s
        ..calls.clear()
        ..moved = const Moved(
          ['lib/cache.dart'],
          [('Makefile', 'it is not what was shown any more')],
        )
        ..copy = const CopyChanges(made: true, back: ('q1', 1));
      await press(t, find.text('Apply 2 files'));
      expect(s.calls, [
        ['bring', ('lib/cache.dart', 'd1'), ('Makefile', 'd2')],
        ['copy', 's', 'b'],
      ]);
      expect(find.textContaining('Applied 1 file.'), findsOneWidget);
      expect(find.textContaining('Makefile: it is not what was shown any more.'), findsOneWidget);
      expect(find.text('No differences from the folder.'), findsOneWidget);
      await snap(t, 'copy-brought');
      s
        ..moved = const Moved(['lib/cache.dart'], [])
        ..copy = all;
      await press(t, find.text('Undo apply (1 file)'));
      expect(s.calls, contains(equals(['back', 's', 'b', 'q1'])));
      expect(find.textContaining('Reverted 1 file.'), findsOneWidget);
      // As usual again: what was chosen was for the last time.
      expect((box(t, 'Makefile').value, box(t, 'gone.txt').value), (false, true));
      // What was chosen was chosen of what was shown: another content has not been.
      box(t, 'Makefile').onChanged!(true);
      box(t, 'lib/cache.dart').onChanged!(false);
      await settled(t);
      expect((box(t, 'Makefile').value, box(t, 'lib/cache.dart').value), (true, false));
      await press(t, find.bySemanticsLabel('Refresh'));
      expect((box(t, 'Makefile').value, box(t, 'lib/cache.dart').value), (true, false));
      s.copy = const CopyChanges(
        made: true,
        files: [
          CopyFile('lib/cache.dart', 'changed', digest: 'd1', there: 'f1', runs: true, diff: some),
          CopyFile('Makefile', 'changed', digest: 'other', there: 'f2', runs: true, diff: some),
        ],
      );
      await press(t, find.bySemanticsLabel('Refresh'));
      expect((box(t, 'Makefile').value, box(t, 'lib/cache.dart').value), (false, false));
    });

    testWidgets('it is brought up to date, and told of clashes by its person', (t) async {
      final s = await shown(t);
      s.renewed = (
        2,
        'The project has changed: both.txt.',
        const [('out/x', 'something is in the way')],
      );
      await press(t, find.text('Update from folder'));
      expect(find.textContaining('Updated 2 files from the folder.'), findsOneWidget);
      expect(find.textContaining('it is not sent yet'), findsOneWidget);
      expect(find.textContaining('out/x: something is in the way.'), findsOneWidget);
      expect(s.calls.where((c) => c.first == 'prompt'), isEmpty);
      await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
      await settled(t);
      expect(find.text('The project has changed: both.txt.'), findsOneWidget);
    });

    testWidgets('it is thrown away with a second word, and never while it is worked in', (t) async {
      final s = await shown(t);
      await press(t, find.text('Discard copy…'));
      expect(
        find.text(
          'These are only in the copy and would be lost: 7 files, 2 that git ignores, 1 that cannot be '
          'listed.',
        ),
        findsOneWidget,
      );
      await snap(t, 'copy-throw-away');
      await press(t, find.text('Keep'));
      expect(s.calls.where((c) => c.first == 'throw away'), isEmpty);
      await press(t, find.text('Discard copy…'));
      s.copy = const CopyChanges();
      await press(t, find.text('Discard copy'));
      expect(s.calls.where((c) => c.first == 'throw away').single, ['throw away', 's', 'b', true]);
      expect(find.textContaining('It is made with the first message.'), findsOneWidget);
      s.copy = const CopyChanges(made: true, ignored: 3);
      await press(t, find.bySemanticsLabel('Refresh'));
      expect(find.text('No differences that can be listed.'), findsOneWidget);
      await press(t, find.text('Discard copy…'));
      expect(find.textContaining('would be lost: 3 that git ignores.'), findsOneWidget);
      await press(t, find.text('Keep'));
      s.copy = const CopyChanges(made: true, whole: false, files: files);
      await press(t, find.bySemanticsLabel('Refresh'));
      await press(t, find.text('Discard copy…'));
      expect(find.textContaining('would be lost: more than 7 files.'), findsOneWidget);
      await press(t, find.text('Keep'));
      s.copy = const CopyChanges(made: true, back: ('q1', 2));
      await press(t, find.bySemanticsLabel('Refresh'));
      await press(t, find.text('Discard copy…'));
      expect(
        find.text(
          'The folder already has all of its changes. The last apply could no longer be undone.',
        ),
        findsOneWidget,
      );
      await press(t, find.text('Discard copy'));
      expect(s.calls.where((c) => c.first == 'throw away').last.last, isFalse);
      // At work.
      s.copy = all;
      s.current!.apply([
        const Event(95, 95, RunStarted('r9', 'b')),
        const Event(96, 96, AgentStateChanged('b', AgentPhase.running)),
      ]);
      await settled(t);
      await press(t, find.bySemanticsLabel('Refresh'));
      for (final l in ['Discard copy…', 'Update from folder']) {
        expect(t.widget<AxButton>(find.widgetWithText(AxButton, l)).onPressed, isNull, reason: l);
      }
      expect(find.textContaining('Ada is still working'), findsOneWidget);
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
      await press(t, find.bySemanticsLabel('Refresh'));
      expect(find.text('the copy is elsewhere now'), findsOneWidget);
      // What went wrong is said, and what there is now is looked at all the same.
      s
        ..failsAt = 'bring'
        ..calls.clear()
        ..copy = const CopyChanges(made: true, files: [CopyFile('now.txt', 'new', digest: 'd')]);
      await press(t, find.textContaining('Apply 2 files'));
      expect(s.calls.map((c) => c.first), ['bring', 'copy']);
      expect(find.text('the copy is elsewhere now'), findsOneWidget);
      expect(find.text('now.txt'), findsOneWidget);
    });

    testWidgets('is set up so beside others, and by choice', (t) async {
      final s = await shell(t, session: working());
      await press(t, find.bySemanticsLabel('Add an agent to Fix the cache'));
      final apart = find.bySemanticsLabel('Works in: In a separate copy');
      expect(apart, findsOneWidget);
      await snap(t, 'add-agent-copy');
      await t.enterText(field('Name'), 'Ada');
      await runs(t, 'Model', 'GPT-5.6');
      s.calls.clear();
      await press(t, find.bySemanticsLabel('Add agent'));
      expect(s.calls.single.last, 'apart');
      await press(t, find.bySemanticsLabel('New session').first);
      expect(apart, findsNothing);
      expect(find.bySemanticsLabel('Works in: In the folder'), findsOneWidget);
      // Who only looks needs none.
      await press(t, find.bySemanticsLabel(RegExp('^Access: ')));
      await press(t, find.bySemanticsLabel('Read only').last);
      expect(find.bySemanticsLabel(RegExp('^Works in')), findsNothing);
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
      await press(t, find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
      expect(t.takeException(), isNull);
      await press(t, find.text('Discard copy…'));
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
    await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
    await settled(t);
    expect(find.text('trunk'), findsOneWidget);
    expect(find.text('4 changed files'), findsOneWidget);
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
    // What has changed in one is read where it is listed.
    expect(find.textContaining('fresh', findRichText: true), findsNothing);
    await press(t, find.bySemanticsLabel('gone.txt, removed, 0 added, 1 removed'));
    await press(t, find.bySemanticsLabel('new dir/é.txt, new, 1 added, 0 removed'));
    await press(t, find.bySemanticsLabel('lib/cache.dart, changed, 1 added, 1 removed'));
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
    await t.tap(find.bySemanticsLabel('Refresh'));
    await settled(t);
    expect(find.text('there is no sandbox on this system'), findsOneWidget);
    await t.tap(find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
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

  testWidgets('what another agent asks is asked where that agent is', (t) async {
    final s = await shell(t, session: working());
    s.current!.apply(const [
      Event(91, 91, AgentAdded('b', 'Ada', x, null, access: Access.ask)),
      Event(
        92,
        92,
        RequestOpened(
          'q9',
          'b',
          'asks to run a command',
          'make it',
          'in .',
          Answerable.live,
          digest: 'd-q9',
          session: true,
        ),
      ),
    ]);
    await settled(t);
    // Not over another's words: a yes is given to the one that asked.
    expect(find.byType(RequestBar), findsNothing);
    expect(find.text('Ada is waiting for you'), findsOneWidget);
    expect(find.text('Allow once'), findsNothing);
    await snap(t, 'another-asks');
    await press(t, find.text('Review'));
    expect(find.text('Ada is waiting for you'), findsNothing);
    expect(find.byType(RequestBar), findsOneWidget);
    await press(t, find.text('Allow once'));
    expect(s.calls.last, ['answer', 's', 'q9', 'd-q9', Answer.once]);
  });

  testWidgets('a folder is chosen in the system\'s own window, if one is chosen', (t) async {
    final asked = <String?>[];
    String? answer = '/chosen';
    final was = folders;
    folders = (at) async {
      asked.add(at);
      return answer;
    };
    addTearDown(() => folders = was);
    await shell(t);
    await press(t, find.bySemanticsLabel('New session').first);
    Future<void> look() async {
      await press(t, find.bySemanticsLabel(RegExp('^Folder: ')));
      await press(t, find.text('Choose…'));
    }

    await look();
    expect(find.bySemanticsLabel('Folder: /chosen'), findsOneWidget);
    answer = null;
    await look();
    expect(find.bySemanticsLabel('Folder: /chosen'), findsOneWidget);
    expect(asked, [null, '/chosen']);
    folders = (_) => throw PlatformException(code: 'x');
    await look();
    expect(t.takeException(), isNull);
  });

  group('models', () {
    const other = Connection('c7', 'Work', 'openrouter', secret: true);
    const opus = ModelInfo(
      'claude-opus-5-5',
      name: 'Claude Opus 5.5',
      context: 1000000,
      efforts: ['low', 'xhigh'],
      speeds: ['fast'],
    );
    final lists = [
      const ModelList('axiom', 'c1', 5, [opus, ModelInfo('claude-old')]),
      const ModelList('axiom', 'c7', 5, [
        ModelInfo('a/b', name: 'A: B', efforts: ['none', 'max']),
      ]),
      // Of an account that is not there any more, and a program that is not.
      const ModelList('axiom', 'c0', 5, [ModelInfo('gone')]),
      const ModelList('codex', null, 5, [ModelInfo('gpt-gone')]),
      const ModelList('claude', null, 0, [
        ModelInfo('opus', name: 'Opus', efforts: ['max'], speeds: ['fast']),
      ]),
    ];
    Future<(Asked, ValueNotifier<Prefs>)> begin(
      WidgetTester t, {
      SessionModel? session,
      Size size = const Size(1100, 1400),
      double textScale = 1,
    }) async {
      final prefs = ValueNotifier(const Prefs());
      addTearDown(prefs.dispose);
      final s = Asked()
        ..current = session
        ..connections = const [key, other]
        ..harnesses = harnesses
        ..lists = lists
        ..byAccount = {'c1': lists[0].models, 'c7': lists[1].models};
      addTearDown(s.dispose);
      await mount(
        t,
        PrefsScope(prefs, child: Shell(s)),
        size: size,
        dpr: 1,
        textScale: textScale,
      );
      return (s, prefs);
    }

    testWidgets('are one list over all accounts and programs', (t) async {
      final (s, prefs) = await begin(t);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      // Those of one at a time; the others are a press away, or a word.
      for (final there in [
        'Opus 5.5',
        'claude-old',
        'Mine, Anthropic API',
        'Work, OpenRouter',
        'Goose',
      ]) {
        expect(find.bySemanticsLabel(there), findsOneWidget, reason: there);
      }
      for (final not in ['A: B', 'gone', 'GPT-gone', 'gpt-gone']) {
        expect(find.bySemanticsLabel(not), findsNothing, reason: not);
      }
      expect(find.textContaining('1M', findRichText: true), findsOneWidget);
      // Nothing to write a name into beside the search.
      expect(find.byType(AxField), findsNothing);
      expect(find.bySemanticsLabel(RegExp('^Harness: ')), findsNothing, reason: 'it follows');
      await snap(t, 'models-offered');
      Tristate mine() =>
          t.getSemantics(find.bySemanticsLabel('Mine, Anthropic API')).flagsCollection.isSelected;
      expect(mine(), Tristate.isTrue);
      await t.enterText(field('Search models'), 'work b');
      await settled(t);
      expect(mine(), isNot(Tristate.isTrue), reason: 'it is looked for everywhere');
      expect(find.bySemanticsLabel('Opus 5.5'), findsNothing);
      await t.pump(AxMotion.settle);
      await press(t, find.bySemanticsLabel('A: B').last);
      // As its provider calls it.
      expect(find.bySemanticsLabel('None'), findsOneWidget);
      await press(t, find.bySemanticsLabel('Max').last);
      await snap(t, 'models-chosen');
      await leave(t);
      expect(find.bySemanticsLabel(RegExp('Runs on A: B, Work, Max')), findsOneWidget);
      await t.enterText(field('Task'), 'go');
      await folder(t, '/w');
      await press(t, find.bySemanticsLabel('Start session'));
      expect(s.calls.where((c) => c.first == 'add').single, [
        'add',
        's_new',
        'Lead',
        'c7',
        'effort max',
        'a/b',
        Access.ask,
      ]);
      expect(prefs.value.used, ['axiom\nc7\na/b']);
    });

    testWidgets('a model of a program brings its program', (t) async {
      final (s, _) = await begin(t);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      await press(t, find.bySemanticsLabel('Claude Code'));
      await press(t, find.bySemanticsLabel('Opus').last);
      expect(find.bySemanticsLabel('Low'), findsNothing, reason: 'it does not say so');
      // Fast has a price, which is said before it is chosen.
      await press(t, find.bySemanticsLabel('Fast').last);
      expect(find.textContaining('usage credits'), findsOneWidget);
      await snap(t, 'models-asked');
      await t.pump(AxMotion.settle);
      await press(t, find.text('Keep it'));
      await press(t, find.bySemanticsLabel('Fast').last);
      await t.pump(AxMotion.settle);
      await press(t, find.text('Change to Fast'));
      await leave(t);
      expect(find.bySemanticsLabel(RegExp('Runs on Opus, Claude Code, Fast')), findsOneWidget);
      // The one that is chosen already: nothing changes, and that is that.
      await part(t, 'Model');
      await press(t, find.bySemanticsLabel('Opus').last);
      expect(find.byType(ModelPicker), findsNothing);
      await t.enterText(field('Task'), 'go');
      await folder(t, '/w');
      await press(t, find.bySemanticsLabel('Start session'));
      expect(s.calls.where((c) => c.first == 'add').single, contains('speed fast'));
    });

    testWidgets('what one can be told is told it, and not another', (t) async {
      final (s, _) = await begin(t);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      await press(t, find.bySemanticsLabel('Opus 5.5').last);
      await press(t, find.bySemanticsLabel('Extra high').last);
      // Every model thinks as much as it needs: nobody is asked.
      expect(find.bySemanticsLabel('Thinking'), findsNothing);
      await snap(t, 'models-told');
      await leave(t);
      await t.enterText(field('Task'), 'go');
      await folder(t, '/w');
      await press(t, find.bySemanticsLabel('Start session'));
      expect(s.calls.where((c) => c.first == 'add').single, contains('effort xhigh'));
      s.calls.clear();
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      await press(t, find.bySemanticsLabel('Extra high').last);
      await press(t, find.bySemanticsLabel('claude-old').last);
      expect(find.text('This model takes none.'), findsOneWidget);
      await leave(t);
      await t.enterText(field('Task'), 'again');
      await settled(t);
      await press(t, find.bySemanticsLabel('Start session'));
      final add = s.calls.where((c) => c.first == 'add').single;
      expect(add.any((x) => '$x'.startsWith('effort')), isFalse);
    });

    testWidgets('one that is in no list is asked about, and nobody else is', (t) async {
      final (s, _) = await begin(t);
      s.told = const ModelInfo('zz-mine', efforts: ['high']);
      await press(t, find.bySemanticsLabel('New session').first);
      await named(t, 'zz-mine');
      await t.pump(const Duration(seconds: 1));
      await settled(t);
      expect(s.calls.last, ['about', 'c1', 'zz-mine']);
      await part(t, 'Model');
      expect(find.bySemanticsLabel('High'), findsOneWidget);
    });

    testWidgets('an agent goes on with one of another account', (t) async {
      final (s, prefs) = await begin(t, session: working());
      s.current!.apply([
        Event(
          s.current!.seq + 1,
          9,
          const AgentChanged(
            'a',
            Execution(
              harness: 'axiom',
              provider: 'anthropic',
              model: 'claude-opus-5-5',
              connection: 'c1',
              effort: 'xhigh',
              speed: 'fast',
            ),
            Access.ask,
          ),
        ),
      ]);
      await settled(t);
      await part(t, 'Model');
      expect(find.bySemanticsLabel('Claude Code'), findsNothing, reason: 'it keeps its harness');
      await press(t, find.bySemanticsLabel('Work, OpenRouter'));
      await t.pump(AxMotion.settle);
      await press(t, find.bySemanticsLabel('A: B').last);
      expect(find.textContaining('read again'), findsOneWidget);
      await t.pump(AxMotion.settle);
      await press(t, find.text('Change to A: B'));
      // What cannot be said to the new one is not said to it.
      expect(s.calls.last, ['set', 's', 'a', 'a/b', 'c7', null]);
      expect(prefs.value.used, ['axiom\nc7\na/b']);
    });

    testWidgets('come as their person has them', (t) async {
      final (s, prefs) = await begin(t);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      // A star, by hand or by key.
      await press(t, find.bySemanticsLabel('Work, OpenRouter'));
      await t.pump(AxMotion.settle);
      Finder row(String name) =>
          find.ancestor(of: find.text(name), matching: find.byType(Pressable));
      await press(
        t,
        find.descendant(
          of: row('A: B').first,
          matching: find.byWidgetPredicate((w) => w is AxButton),
        ),
      );
      expect(prefs.value.favourites, ['axiom\nc7\na/b']);
      await press(t, find.bySemanticsLabel('Favourites'));
      expect(find.bySemanticsLabel('A: B'), findsOneWidget);
      expect(find.bySemanticsLabel('Opus 5.5'), findsNothing);
      await snap(t, 'models-favourite');
      await t.enterText(field('Search models'), 'opus');
      await settled(t);
      expect(find.bySemanticsLabel('Use opus'), findsNothing, reason: 'there is one');
      await t.enterText(field('Search models'), '');
      await settled(t);
      // To where it is said which are offered.
      await press(t, find.bySemanticsLabel('Manage models'));
      expect(find.text('Models'), findsOneWidget);
      expect(find.textContaining('as documented'), findsOneWidget);
      Future<void> set(String model, String how) async {
        await press(t, find.bySemanticsLabel(RegExp('^$model: ')));
        await press(t, find.bySemanticsLabel(how).last);
      }

      await set('A: B', 'Offered');
      await set('claude-old', 'Hidden');
      await snap(t, 'models-set');
      expect(prefs.value.favourites, isEmpty);
      expect(prefs.value.hidden, ['axiom\nc1\nclaude-old']);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      expect(find.bySemanticsLabel('claude-old'), findsNothing);
      // By the keyboard alone: F gives a star, a number takes one.
      await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await settled(t);
      await t.sendKeyEvent(LogicalKeyboardKey.keyF);
      await settled(t);
      expect(prefs.value.favourites, ['axiom\nc1\nclaude-opus-5-5']);
      await press(t, find.bySemanticsLabel('Work, OpenRouter'));
      final key = Command.isMac ? LogicalKeyboardKey.metaLeft : LogicalKeyboardKey.controlLeft;
      await t.sendKeyDownEvent(key);
      await t.sendKeyEvent(LogicalKeyboardKey.digit1);
      await t.sendKeyUpEvent(key);
      await settled(t);
      expect(find.bySemanticsLabel(RegExp('^Runs on A: B, Work')), findsOneWidget);
      // Enter takes the first that fits, and that is that.
      await t.enterText(field('Search models'), 'opus 5');
      await settled(t);
      await t.testTextInput.receiveAction(TextInputAction.done);
      await settled(t);
      expect(find.bySemanticsLabel(RegExp('^Runs on Opus 5.5, Mine')), findsOneWidget);
      expect(find.byType(ModelPicker), findsNothing);
      await press(t, find.bySemanticsLabel('Accounts').first);
      s.calls.clear();
      await press(t, find.widgetWithText(AxButton, 'Refresh'));
      expect(s.calls, [
        ['models', 'c1'],
        ['models', 'c7'],
      ]);
    });

    for (final (size, scale) in const [
      (Size(760, 520), 1.0),
      (Size(760, 520), 2.0),
      (Size(440, 520), 1.0),
    ]) {
      testWidgets('are chosen in $size at $scale', (t) async {
        final (s, prefs) = await begin(t, session: working(), size: size, textScale: scale);
        prefs.value = const Prefs(harnesses: true);
        s.current!.apply([
          Event(
            s.current!.seq + 1,
            9,
            const AgentChanged(
              'a',
              Execution(harness: 'axiom', model: 'claude-opus-5-5', connection: 'c1'),
              Access.ask,
            ),
          ),
        ]);
        await settled(t);
        await part(t, 'Model');
        expect(t.takeException(), isNull, reason: 'nothing overflows');
        await snap(t, 'models-${size.width.round()}-$scale');
        await press(t, find.bySemanticsLabel('Extra high').last);
        expect(s.calls.last, ['set', 's', 'a', 'claude-opus-5-5', 'c1', 'xhigh', null]);
      });
    }

    testWidgets('the harness is a choice only for those who want it', (t) async {
      final (s, prefs) = await begin(t);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Model');
      expect(find.bySemanticsLabel(RegExp('^Harness: ')), findsNothing);
      await leave(t);
      await press(t, find.bySemanticsLabel('Settings').first);
      expect(find.text('Advanced'), findsOneWidget);
      await press(t, find.bySemanticsLabel('Choose the harness').last);
      expect(prefs.value.harnesses, isTrue);
      await press(t, find.bySemanticsLabel('New session').first);
      await part(t, 'Harness');
      await snap(t, 'models-advanced');
      await press(t, find.bySemanticsLabel('Goose').last);
      await leave(t);
      expect(find.bySemanticsLabel(RegExp('Runs on Default model, Goose')), findsOneWidget);
      expect(s.calls.where((c) => c.first == 'add'), isEmpty);
    });

    test('the last used, then the rest', () {
      Offer o(String id) => Offer('axiom', 'c', null, ModelInfo(id));
      final all = [
        for (final id in ['a', 'b', 'c', 'd', 'e']) o(id),
      ];
      String k(String id) => offerKey('axiom', 'c', id);
      final p = Prefs(used: [k('c'), k('b')], hidden: [k('a'), k('e')]);
      List<String> ids(String? chosen) => [
        for (final o in arranged(all, p, chosen: chosen)) o.model.id,
      ];
      expect(ids(null), ['c', 'b', 'd']);
      expect(ids(k('a')), ['c', 'b', 'a', 'd'], reason: 'what is chosen is there');
      expect([for (final o in arranged(all, const Prefs())) o.model.id], ['a', 'b', 'c', 'd', 'e']);
      expect(contextName(1000000), '1M');
      expect(contextName(1048576), '1M');
      expect(contextName(1500000), '1.5M');
      expect(contextName(200000), '200K');
    });
  });

  group('a plan first', () {
    testWidgets('is asked for when it is begun', (t) async {
      final s = await shell(t);
      s.harnesses = harnesses;
      await press(t, find.bySemanticsLabel('New session').first);
      await t.enterText(field('Task'), 'Think it over.');
      await folder(t, '/home/me/project');
      await runs(t, 'Model', 'GPT-5.6');
      await press(t, find.bySemanticsLabel('Access: Ask first'));
      await snap(t, 'plan-access');
      await press(t, find.bySemanticsLabel('Plan first').last);
      expect(find.bySemanticsLabel('Access: Plan first'), findsOneWidget);
      await press(t, find.bySemanticsLabel(RegExp('^Works in')));
      await press(t, find.bySemanticsLabel('In a separate copy').last);
      await press(t, find.bySemanticsLabel('Start session'));
      expect(s.calls.where((c) => c.first == 'add').single, [
        'add',
        's_new',
        'Lead',
        'c1',
        'gpt-5.6',
        Access.read,
        'apart',
        'plan',
      ]);
    });

    testWidgets('and not of a program with its own tools', (t) async {
      final s = await shell(t);
      s.harnesses = harnesses;
      await press(t, find.bySemanticsLabel('New session').first);
      await press(t, find.bySemanticsLabel('Access: Ask first'));
      await press(t, find.bySemanticsLabel('Plan first').last);
      await part(t, 'Model');
      await press(t, find.bySemanticsLabel('Goose'));
      await press(t, find.bySemanticsLabel('Default model').last);
      await leave(t);
      expect(find.bySemanticsLabel('Access: Plan first'), findsNothing);
      await press(t, find.bySemanticsLabel(RegExp('^Access: ')));
      expect(find.bySemanticsLabel('Plan first'), findsNothing);
    });

    Future<Asked> proposed(WidgetTester t) async {
      final s = await shell(t, session: working(access: Access.read));
      final m = s.current!, run = m.agents['a']!.run!;
      var n = m.seq + 1;
      Event e(Body b) => Event(n, n++, b);
      m.apply([
        e(const AgentChanged('a', x, Access.read, plan: true)),
        e(ItemStarted('p9', run, 'a', ItemKind.plan, '', null)),
        e(const ItemEnded('p9', Outcome.completed, '## First\n\n1. Make `n.txt`.', null)),
      ]);
      await settled(t);
      expect(
        find.textContaining('Plan ready', findRichText: true),
        findsNothing,
        reason: 'it is still at work',
      );
      m.apply([e(RunEnded(run, Outcome.completed, null))]);
      await settled(t);
      await t.pump(const Duration(seconds: 1));
      return s;
    }

    testWidgets('is shown whole and accepted as it is', (t) async {
      final s = await proposed(t);
      expect(find.textContaining('Plan ready', findRichText: true), findsOneWidget);
      expect(find.text('Plan'), findsOneWidget);
      expect(find.textContaining('First', findRichText: true), findsWidgets);
      expect(find.text('What should be different'), findsOneWidget);
      await snap(t, 'plan-ready');
      await press(t, find.bySemanticsLabel('Then: Ask first'));
      await press(t, find.bySemanticsLabel('Work without asking').last);
      await press(t, find.widgetWithText(AxButton, 'Accept'));
      expect(s.calls.last, ['accept', 's', 'a', 'p9', Access.work]);
    });

    testWidgets('or as its person has it', (t) async {
      final s = await proposed(t);
      await press(t, find.widgetWithText(AxButton, 'Edit plan'));
      await t.pump(const Duration(seconds: 1));
      expect(find.widgetWithText(AxButton, 'Accept'), findsOneWidget, reason: 'it is the same');
      await t.enterText(field('Plan'), '  ');
      await settled(t);
      AxButton accept() =>
          t.widget(find.byWidgetPredicate((w) => w is AxButton && w.label.startsWith('Accept')));
      expect(accept().onPressed, isNull);
      await t.enterText(field('Plan'), '## Mine');
      await settled(t);
      await snap(t, 'plan-edited');
      s.failure = const Failed(1, 'bad_request', 'this is not the plan');
      await press(t, find.widgetWithText(AxButton, 'Accept as edited'));
      expect(find.text('this is not the plan'), findsOneWidget);
      s.failure = null;
      await press(t, find.widgetWithText(AxButton, 'Accept as edited'));
      expect(s.calls.last, ['accept', 's', 'a', 'p9', Access.ask, '## Mine']);
      // What was written is given up by a press, and not otherwise.
      await press(t, find.widgetWithText(AxButton, 'Discard edits'));
      expect(field('Plan'), findsNothing);
    });

    testWidgets('waits until something else is said', (t) async {
      final s = await proposed(t);
      final m = s.current!;
      var n = m.seq + 1;
      m.apply([
        Event(n, n++, const RunStarted('r8', 'a')),
        Event(n, n++, const ItemStarted('w9', 'r8', 'a', ItemKind.prompt, '', null)),
        Event(n, n++, const ItemEnded('w9', Outcome.completed, 'another way', null)),
        Event(n, n++, const RunEnded('r8', Outcome.completed, null)),
      ]);
      await settled(t);
      expect(find.textContaining('Plan ready', findRichText: true), findsNothing);
      // Who may do something else is not asked for one any more.
      await press(t, find.bySemanticsLabel('Access: Plan first'));
      await press(t, find.bySemanticsLabel('Ask first').last);
      expect(s.calls.last, ['set', 's', 'a', null, null, Access.ask]);
      m.apply([Event(n, n++, const AgentChanged('a', x, Access.ask))]);
      await settled(t);
      await press(t, find.bySemanticsLabel('Access: Ask first'));
      await press(t, find.bySemanticsLabel('Plan first').last);
      expect(s.calls.last, ['set', 's', 'a', null, null, null, 'plan']);
    });
  });

  testWidgets('what an agent says it means to do ticks off beside it', (t) async {
    final s = Asked()
      ..current = working()
      ..connections = const [key];
    addTearDown(s.dispose);
    await mount(t, Shell(s), size: const Size(1280, 820), dpr: 2);
    await settled(t);
    expect(
      find.bySemanticsLabel('Show the panel'),
      findsOneWidget,
      reason: 'there is nothing in it',
    );
    var n = 70;
    Future<void> says(List<Todo> steps, {String? text}) async {
      s.current!.apply([Event(n++, n, TodoSaid('a', steps, text: text))]);
      await settled(t);
    }

    await says(const [
      Todo('Read the cache', TodoState.done),
      Todo('Make it \u202eforget', TodoState.doing),
      Todo('Test it', TodoState.todo),
    ]);
    expect(find.bySemanticsLabel('Hide the panel'), findsOneWidget);
    expect(find.text('1 of 3'), findsOneWidget);
    expect(find.bySemanticsLabel('Read the cache, done'), findsOneWidget);
    expect(find.bySemanticsLabel('Test it, to do'), findsOneWidget);
    expect(find.bySemanticsLabel(RegExp('forget, active')), findsOneWidget);
    expect(find.textContaining('\u202e'), findsNothing, reason: 'what hides is shown');
    await snap(t, 'panel-plan');
    // Nobody works on it: it is not being done, whatever was said.
    final run = s.current!.agents['a']!.run!;
    s.current!.apply([Event(n++, n, RunEnded(run, Outcome.completed, null))]);
    await settled(t);
    expect(find.bySemanticsLabel(RegExp('forget, to do')), findsOneWidget);
    await press(t, find.text('Plan'));
    expect(find.bySemanticsLabel('Test it, to do'), findsNothing);
    // Words without steps are a plan too, and none at all is none.
    await says(const [], text: 'First this, then that.');
    await press(t, find.text('Plan'));
    expect(find.textContaining('First this'), findsOneWidget);
    await says(const []);
    expect(find.text('Plan'), findsNothing);
  });

  testWidgets('what has changed is beside the conversation, where there is room', (t) async {
    final s = Asked()
      ..current = working()
      ..connections = const [key]
      ..now = const Changes(
        repository: true,
        branch: 'trunk',
        files: [Changed('lib/a.dart', 'changed')],
        diff: '--- lib/a.dart\n+++ lib/a.dart\n@@ -1 +1,2 @@\n-old\n+fresh\n+more\n',
      );
    addTearDown(s.dispose);
    await mount(t, Shell(s), size: const Size(1280, 820), dpr: 2);
    await settled(t);
    // It is there without being asked for, slim; what is in a file is read in it, and then it is wide.
    expect(s.calls, [
      ['changes', 's'],
    ]);
    expect(find.bySemanticsLabel('Hide the panel'), findsOneWidget);
    expect(find.text('1 changed file'), findsOneWidget);
    expect(find.textContaining('fresh', findRichText: true), findsNothing);
    await snap(t, 'panel');
    double wide() => t.getSize(find.byType(ChangesView)).width;
    final slim = wide();
    await press(t, find.bySemanticsLabel(RegExp('^lib/a.dart')));
    await snap(t, 'changes-beside');
    expect(wide(), greaterThan(slim + 60));
    expect(find.textContaining('fresh', findRichText: true), findsOneWidget);
    // Both are there: what is said about a line is written where it can be seen.
    expect(find.byType(TranscriptView), findsOneWidget);
    expect(find.text('Message Lead. Sent before its next step'), findsWidgets);
    await press(t, find.bySemanticsLabel('Comment on lib/a.dart'));
    await t.enterText(field('About the file'), 'why?');
    await press(t, find.text('Add to message'));
    expect(find.text('On lib/a.dart:\nwhy?'), findsOneWidget);
    // The room between them is the handle, for the hand and for the keyboard.
    final was = wide();
    await t.drag(find.bySemanticsLabel('Width of the panel'), const Offset(-60, 0));
    await t.pump();
    expect(wide(), greaterThan(was + 20));
    final dragged = wide();
    bool held() {
      var found = false;
      FocusManager.instance.primaryFocus?.context?.visitAncestorElements((e) {
        final w = e.widget;
        found = w is Semantics && w.properties.label == 'Width of the panel';
        return !found;
      });
      return found;
    }

    for (var k = 0; k < 60 && !held(); k++) {
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
    }
    expect(held(), isTrue);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await t.pump();
    expect(wide(), closeTo(dragged - 24, 1));
    // Neither is squeezed away.
    await t.drag(find.bySemanticsLabel('Width of the panel'), const Offset(-2000, 0));
    await t.pump();
    expect(t.getSize(find.byType(TranscriptView)).width, greaterThanOrEqualTo(359));
    await t.drag(find.bySemanticsLabel('Width of the panel'), const Offset(2000, 0));
    await t.pump();
    expect(wide(), closeTo(360, 1));
    await press(t, find.bySemanticsLabel(RegExp('^lib/a.dart')));
    expect(wide(), closeTo(slim, 1), reason: 'slim again when nothing is read');
    // Its parts can be put away; how much has changed is said still.
    await press(t, find.bySemanticsLabel('1 changed file'));
    expect(find.bySemanticsLabel(RegExp('^lib/a.dart')), findsNothing);
    expect(find.text('+2'), findsOneWidget);
    await press(t, find.bySemanticsLabel('1 changed file'));
    expect(find.bySemanticsLabel(RegExp('^lib/a.dart')), findsOneWidget);
    // All of it, away and back, at its own side; also where the sidebar is put away.
    await press(t, find.bySemanticsLabel('Hide the panel'));
    expect(find.byType(ChangesView), findsNothing);
    await press(t, find.bySemanticsLabel('Hide the sidebar'));
    s.calls.clear();
    await press(t, find.bySemanticsLabel('Show the panel'));
    expect(find.byType(ChangesView), findsOneWidget);
    expect(s.calls, [
      ['changes', 's'],
    ], reason: 'who asks sees what there is now');
  });

  testWidgets('where there is no room for both, what has changed stands in front', (t) async {
    for (final size in const [Size(600, 600), Size(860, 600)]) {
      final s = Asked()
        ..current = working(asks: true)
        ..connections = const [key]
        ..now = const Changes(repository: true, files: [Changed('a', 'new')]);
      await mount(t, Shell(s, key: ValueKey(size)), size: size, dpr: 1);
      await settled(t);
      await t.enterText(find.byType(EditableText), 'half a thought');
      await press(t, find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
      expect(find.byType(ChangesView), findsOneWidget);
      // Not pressed, read or reached behind it.
      expect(find.text('Allow once').hitTestable(), findsNothing);
      for (var k = 0; k < 30; k++) {
        await t.sendKeyEvent(LogicalKeyboardKey.tab);
        await t.pump();
        final at = FocusManager.instance.primaryFocus?.context;
        expect(at?.findAncestorWidgetOfExactType<RequestBar>(), isNull);
      }
      await press(t, find.bySemanticsLabel(RegExp('^(Show|Hide) the panel')));
      expect(find.text('half a thought'), findsOneWidget);
      expect(find.text('Allow once').hitTestable(), findsOneWidget);
      s.dispose();
    }
  });

  testWidgets('what is being written to an agent is there when its person comes back', (t) async {
    final s = await shell(t, session: working());
    final words = find.byType(EditableText);
    await t.enterText(words, 'Half a thought');
    await press(t, find.bySemanticsLabel('Accounts').first);
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
    await press(t, find.bySemanticsLabel('Accounts').first);
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
    await t.enterText(field('Task'), 'precious words');
    await folder(t, '/w');
    await settled(t);
    await press(t, find.text('Add an account or server'));
    s.connections = const [key];
    await press(t, find.text('Done'));
    if (find.byType(StartPage).evaluate().isEmpty) {
      await press(t, find.bySemanticsLabel('New session').first);
    }
    expect(find.text('precious words'), findsOneWidget);
    expect(find.bySemanticsLabel('Folder: /w'), findsOneWidget);
  });

  testWidgets('starting again makes nothing twice, and takes what was changed', (t) async {
    final s = await shell(t);
    await fill(t, 'Do it.');
    s
      ..calls.clear()
      ..failsAt = 'prompt'
      ..failure = const LinkClosed('no answer');
    await press(t, find.bySemanticsLabel('Start session'));
    await press(t, find.bySemanticsLabel('Start session'));
    expect(s.calls.map((c) => c.first), ['create', 'add', 'open', 'prompt', 'open', 'prompt']);
    expect(s.keys[0], s.keys[1]);
    s.calls.clear();
    await press(t, find.bySemanticsLabel('Access: Ask first'));
    await press(t, find.bySemanticsLabel('Work without asking').last);
    s.failure = null;
    await press(t, find.bySemanticsLabel('Start session'));
    expect(s.calls, [
      ['set', 's_new', 'a_new', 'gpt-5.6', 'c1', Access.work],
      ['open', 's_new'],
      ['prompt', 's_new', 'a_new', 'Do it.'],
    ]);
    // The next is another.
    s.calls.clear();
    await press(t, find.bySemanticsLabel('New session').first);
    expect(find.text('Do it.'), findsNothing);
    await t.enterText(field('Task'), 'Do it.');
    await settled(t);
    await press(t, find.bySemanticsLabel('Start session'));
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
    await press(t, find.bySemanticsLabel('Start session'));
    await press(t, find.bySemanticsLabel('Accounts').first);
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
    await t.enterText(field('Task'), 'other words');
    await settled(t);
    await press(t, find.bySemanticsLabel('Start session'));
    await press(t, find.bySemanticsLabel('Accounts').first);
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
    await part(t, 'Model');
    await t.enterText(field('Search models'), 'my-own');
    gate.complete();
    await settled(t);
    expect(find.bySemanticsLabel('Use my-own'), findsOneWidget);
    expect(t.widget<EditableText>(field('Search models')).controller.text, 'my-own');
  });

  testWidgets('a provider that lists nothing: its model is written', (t) async {
    final s = await shell(t);
    s.known = const [];
    await press(t, find.bySemanticsLabel('New session').first);
    await part(t, 'Model');
    await press(t, find.bySemanticsLabel('Here, LM Studio on this machine'));
    s.calls.clear();
    await press(t, find.bySemanticsLabel('List its models'));
    expect(s.calls, [
      ['models', 'c2'],
    ]);
    expect(find.textContaining('It lists none'), findsOneWidget);
    await leave(t);
    await named(t, 'zz-mine');
    expect(find.bySemanticsLabel(RegExp('^Runs on zz-mine')), findsOneWidget);
    expect(find.byType(AccessKey), findsOneWidget);
  });

  testWidgets('models are asked for when somebody looks, once', (t) async {
    final s = await shell(t, session: working());
    expect(s.calls, isEmpty);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(s.calls, [
      ['models', 'c1'],
    ]);
    expect(find.text('GPT-5.6'), findsWidgets);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await settled(t);
    await t.tap(find.byType(ExecutionKey));
    await settled(t);
    expect(s.calls.length, 1);
  });

  testWidgets('a stop that fails says so', (t) async {
    final s = await shell(t, session: working());
    s.failure = const LinkClosed('the host is gone');
    await t.tap(find.bySemanticsLabel('Stop'));
    await settled(t);
    expect(find.textContaining('It could not be stopped: the host is gone'), findsOneWidget);
    s.failure = null;
    await t.tap(find.bySemanticsLabel('Stop'));
    await settled(t);
    expect(find.textContaining('It could not be stopped'), findsNothing);
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
      for (final l in [RegExp(r'^New session$'), RegExp('^Accounts')]) {
        await t.tap(find.bySemanticsLabel(l).first);
        await settled(t);
        expect(t.takeException(), isNull, reason: '$l');
      }
    });
  }
}
