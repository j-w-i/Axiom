import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';

import 'link.dart';
import 'proto.dart';
import 'session_model.dart';

/// Where the host keeps its files. Mirrors `paths.rs`.
class HostPaths {
  HostPaths(this.data);
  static HostPaths? discover([Map<String, String>? env]) {
    final e = env ?? Platform.environment;
    String? v(String k) => (e[k] ?? '').isEmpty ? null : e[k];
    final data =
        v('AXIOM_DATA') ??
        (Platform.isMacOS
            ? v('HOME')?.let((h) => '$h/Library/Application Support/Axiom')
            : Platform.isWindows
            ? v('LOCALAPPDATA')?.let((h) => '$h\\Axiom')
            : (v('XDG_DATA_HOME') ?? v('HOME')?.let((h) => '$h/.local/share'))?.let(
                (h) => '$h/axiom',
              ));
    return data == null ? null : HostPaths(data);
  }

  final String data;
  String get token => '$data/run/token';

  /// Written by the host: the socket's place depends on the length of the directory's name.
  String get endpointFile => '$data/run/endpoint';
}

extension _Let<T> on T {
  R let<R>(R Function(T) f) => f(this);
}

enum HostPhase { starting, connected, unreachable }

/// Starts the host if none runs, connects, follows, and reconnects. The client's only process
/// (SI-14), and the only place that talks to the host.
class HostStore extends ChangeNotifier {
  HostStore({
    required this.paths,
    required this.binary,
    this.spawn = true,
    this.listEvery = const Duration(seconds: 20),
    this.patience = const Duration(seconds: 30),
  });

  /// How long a host that is there may take to list its sessions.
  final Duration patience;

  /// Other windows and the command line make sessions too, and ages go on.
  final Duration listEvery;
  final HostPaths paths;

  /// The `axiom-host` executable, or null if there is none beside the application.
  final String? binary;
  final bool spawn;

  HostPhase phase = HostPhase.starting;
  String? problem;
  List<SessionInfo> sessions = [];

  /// The list holds the last [listed] sessions; there are older ones.
  bool older = false;
  static const listed = 1000;

  /// What the host found wrong with its journal, if it did.
  String? damaged;
  SessionModel? current;

  HostLink? _link;
  StreamSubscription<Message>? _sub;
  Timer? _retry, _drainTimer, _list;
  bool _disposed = false, _starting = false;
  int _attempt = 0;
  final _pending = <Event>[];
  int _at = 0;
  bool _scheduled = false;
  final _restarts = <DateTime>[];

  static String? findBinary() {
    final exe = Platform.isWindows ? 'axiom-host.exe' : 'axiom-host';
    final here = File(Platform.resolvedExecutable).parent.path;
    for (final p in ['$here/$exe', '$here/../Resources/$exe']) {
      if (File(p).existsSync()) return p;
    }
    return null;
  }

  Future<void> start() async {
    if (_disposed || _starting) return;
    _starting = true;
    try {
      await _connect();
      _attempt = 0;
    } on Object catch (e) {
      if (!_disposed) _unreachable('$e');
    } finally {
      _starting = false;
    }
  }

  void _unreachable(String why) {
    // What arrived is true whether or not the connection lives; the next follow starts after it.
    _drainAll();
    _drop();
    phase = HostPhase.unreachable;
    problem = why;
    notifyListeners();
    _attempt++;
    final ms = (250 << _attempt.clamp(0, 5)).clamp(250, 8000);
    _retry?.cancel();
    _retry = Timer(Duration(milliseconds: ms), () => unawaited(start()));
  }

  /// Lets go of the connection, whatever state it is in.
  void _drop() {
    _list?.cancel();
    unawaited(_sub?.cancel());
    _sub = null;
    _link?.close();
    _link = null;
  }

  Future<HostLink> _open() async => HostLink.connect(
    endpoint: await File(paths.endpointFile).readAsString(),
    token: await File(paths.token).readAsString(),
  );

  Future<void> _connect() async {
    HostLink link;
    try {
      link = await _open();
    } on Object {
      final b = binary;
      if (!spawn || b == null) rethrow;
      // Detached: it must outlive this window (PR-14). It exits by itself when idle.
      await Process.start(
        b,
        const ['--idle-exit', '60'],
        environment: {'AXIOM_DATA': paths.data},
        mode: ProcessStartMode.detached,
      );
      link = await _until(_open, const Duration(seconds: 5));
    }
    if (_disposed) return link.close();
    _link = link;
    _sub = link.messages.listen(_message);
    _list?.cancel();
    _list = Timer.periodic(listEvery, (_) {
      if (_link != null && SchedulerBinding.instance.framesEnabled) {
        unawaited(_still(link));
      }
    });
    unawaited(
      link.closed.then((why) {
        if (!_disposed && identical(link, _link)) _unreachable(why);
      }),
    );
    phase = HostPhase.connected;
    problem = null;
    await refresh();
    // Nothing that is shown depends on it.
    unawaited(loadConnections().catchError((_) {}));
    unawaited(loadHarnesses().catchError((_) {}));
    unawaited(loadLists().catchError((_) {}));
    unawaited(loadNeeds());
    if (current case final c? when c.fault == null) await _follow(c);
    notifyListeners();
  }

  static Future<T> _until<T>(Future<T> Function() f, Duration d) async {
    final end = DateTime.now().add(d);
    while (true) {
      try {
        return await f();
      } on Object {
        if (DateTime.now().isAfter(end)) rethrow;
        await Future<void>.delayed(const Duration(milliseconds: 40));
      }
    }
  }

  /// A connection can be dead without an end to it (Windows): a host that does not answer is asked
  /// again on another.
  Future<void> _still(HostLink link) async {
    try {
      await refresh();
    } on LinkClosed catch (e) {
      if (!_disposed && identical(link, _link)) _unreachable(e.why);
    } on Object {
      // It answered; what it said is not this one's business.
    }
  }

  HostLink get _live => _link ?? (throw const LinkClosed('the host is not answering'));

  Future<void> refresh() async {
    final link = _live;
    final r = await link.call('session.list', {'limit': listed}, patience);
    sessions = [for (final j in r! as List<Object?>) SessionInfo.decode(j! as Json)];
    older = sessions.length >= listed;
    notifyListeners();
    // Nothing depends on it, and it may be slow.
    unawaited(_status(link));
  }

  /// The journal is checked in the background after the host starts, so this is asked again.
  Future<void> _status(HostLink link) async {
    Object? s;
    try {
      s = await link.call('host.status');
    } on Object {
      return;
    }
    if (_disposed || !identical(link, _link)) return;
    final d = s is Json ? s['damaged'] : null;
    damaged = d is String ? d : null;
    notifyListeners();
  }

  Future<String> create(String title, {String? workspace}) async {
    final r = await _live.call('session.create', {'title': title, 'workspace': ?workspace});
    await refresh();
    return (r! as Json)['session']! as String;
  }

  Future<void> open(String id) async {
    final link = _link;
    if (current case final c? when c.id != id) {
      _forget();
      if (link != null) unawaited(link.unfollow(c.id).catchError((_) {}));
    }
    if (current?.id != id || current?.fault != null) {
      _forget();
      if (current != null && link != null && current!.id == id) {
        unawaited(link.unfollow(id).catchError((_) {}));
      }
      current = SessionModel(id);
    }
    notifyListeners();
    if (link != null) await _follow(current!);
  }

  /// A folder was asked for in a terminal. It is shown as where a session could begin.
  void Function(String path)? onFolder;

  /// What waits for a person, in all sessions, the longest first.
  List<Waits> needs = const [];
  bool _asksNeeds = false, _needsAgain = false;

  Future<void> loadNeeds() async {
    if (_asksNeeds) {
      _needsAgain = true;
      return;
    }
    _asksNeeds = true;
    try {
      do {
        _needsAgain = false;
        final r = await _live.call('host.waiting');
        if (_disposed) return;
        needs = [for (final j in r! as List<Object?>) Waits.decode(j! as Json)];
        notifyListeners();
      } while (_needsAgain);
    } on Object {
      // It is asked for again with the next connection, or the next change.
    } finally {
      _asksNeeds = false;
    }
  }

  final _seen = <(String, String, int)>{};

  /// Its person has what an agent came to before them.
  void seen(String session, String agent) {
    // Said again when the host answers once more.
    final link = _link;
    if (link == null) return;
    for (final w in needs) {
      if (w.session != session || w.agent != agent || w.need == Need.asks) continue;
      // Said once; the host says when it is gone.
      if (!_seen.add((session, agent, w.at))) continue;
      unawaited(
        link
            .call('agent.seen', {'session': session, 'agent': agent})
            .then((_) {}, onError: (Object _) => _seen.remove((session, agent, w.at))),
      );
    }
    if (_seen.length > 1000) _seen.clear();
  }

  /// Ways to reach providers, as the host has them.
  List<Connection> connections = const [];
  final _models = <String, List<ModelInfo>>{};

  Future<void> loadConnections() async {
    final r = await _live.call('connection.list');
    connections = [for (final j in r! as List<Object?>) Connection.decode(j! as Json)];
    notifyListeners();
  }

  /// What the host can run agents with beside itself; found without running any of it.
  List<HarnessInfo> harnesses = const [];

  Future<void> loadHarnesses() async {
    final r = await _live.call('harness.list');
    harnesses = [for (final j in r! as List<Object?>) HarnessInfo.decode(j! as Json)];
    notifyListeners();
  }

  /// What has been said of how programs are started.
  List<ProgramInfo> programs = const [];

  Future<void> loadPrograms() async {
    final r = await _live.call('program.list');
    programs = [for (final j in r! as List<Object?>) ProgramInfo.decode(j! as Json)];
    notifyListeners();
  }

  /// Says how a program is started; without [harness], adds one. Secrets go to the host and are not
  /// kept here.
  Future<String> setProgram({
    required List<String> args,
    required List<VarSet> vars,
    String? harness,
    String? account,
    String? name,
    String? path,
  }) async {
    final r = await _live.call('program.set', {
      'harness': ?harness,
      'account': ?account,
      'name': ?name,
      'path': ?path,
      'args': args,
      'vars': [for (final v in vars) v.encode()],
    });
    await loadPrograms();
    await loadHarnesses();
    return (r! as Json)['harness']! as String;
  }

  /// Takes away one its person added, or what was said of another.
  Future<void> removeProgram(String harness, {String? account}) async {
    await _live.call('program.remove', {'harness': harness, 'account': ?account});
    await loadPrograms();
    await loadHarnesses();
  }

  /// What has been changed in a session's folder.
  Future<Changes> changes(String session) async =>
      Changes.decode((await _live.call('workspace.changes', {'session': session}))! as Json);

  /// Much is read for it, and perhaps written.
  static const _long = Duration(minutes: 5);

  /// What an agent has changed in its own copy of the folder.
  Future<CopyChanges> copyChanges(String session, String agent) async => CopyChanges.decode(
    (await _live.call('copy.changes', {'session': session, 'agent': agent}, _long))! as Json,
  );

  /// Brings changes in, each as it was shown.
  Future<Moved> bringIn(String session, String agent, List<CopyFile> files) async => Moved.decode(
    (await _live.call('copy.bring', {
          'session': session,
          'agent': agent,
          'files': [for (final f in files) f.encode()],
        }, _long))!
        as Json,
    'brought',
  );

  /// Puts back what was there before the last that was brought in, as it was named.
  Future<Moved> putBack(String session, String agent, String back) async => Moved.decode(
    (await _live.call('copy.back', {'session': session, 'agent': agent, 'back': back}, _long))!
        as Json,
    'back',
  );

  /// Takes over what has changed in the folder: how many files, what its agent should be told of
  /// those both have changed, and what was not taken over, with why.
  Future<(int, String?, List<(String, String)>)> renewCopy(String session, String agent) async {
    final r = await _live.call('copy.renew', {'session': session, 'agent': agent}, _long);
    final j = r! as Json;
    return (j['taken']! as int, j['say'] as String?, Moved.leftOf(j));
  }

  /// Throws the copy away; [lose]: with what was changed in it.
  Future<void> removeCopy(String session, String agent, {required bool lose}) =>
      _live.call('copy.remove', {'session': session, 'agent': agent, 'lose': lose}, _long);

  /// Accounts that Axiom's own loop can use: not those of programs.
  List<Connection> get own => [
    for (final c in connections)
      if (c.signIn == null) c,
  ];

  /// Accounts of a program, beside its first.
  List<Connection> accountsOf(String harness) {
    final p = harnesses.where((h) => h.id == harness).firstOrNull?.accounts;
    return [
      for (final c in connections)
        if (p != null && c.provider == p) c,
    ];
  }

  /// As a person knows it.
  String harnessName(String id) => id == 'axiom'
      ? 'Axiom'
      : harnesses.where((h) => h.id == id).firstOrNull?.name ??
            (id.startsWith('p_') ? 'a program that was removed' : id);

  /// The secret goes to the host and is not kept here.
  Future<String> connect(String name, String provider, {String? url, String? secret}) async {
    final r = await _live.call('connection.add', {
      'name': name,
      'provider': provider,
      'url': ?url,
      'secret': ?secret,
    });
    await loadConnections();
    return (r! as Json)['connection']! as String;
  }

  /// Whether it is signed out of there too, for an account of a plan.
  Future<bool?> disconnect(String id) async {
    final r = await _live.call('connection.remove', {'connection': id});
    _models.remove(id);
    await loadConnections();
    return (r! as Json)['ended'] as bool?;
  }

  /// The sign-in that is on, or how the last one ended until somebody has seen it.
  Signin? signin;
  Timer? _asksSignin;

  /// Begins one; [again]: to an account there is. What is opened in the browser is its `url`.
  Future<Signin> beginSignin({String? again}) async {
    await cancelSignin();
    final r = (await _live.call('signin.begin', {'connection': ?again}))! as Json;
    final s = signin = Signin(r['signin']! as String, r['url']! as String, again);
    _asksSignin = Timer.periodic(const Duration(seconds: 1), (_) => unawaited(_askSignin(s.id)));
    notifyListeners();
    return s;
  }

  Future<void> _askSignin(String id) async {
    final s = signin;
    if (s == null || s.id != id || !s.waits) return;
    Signin now;
    try {
      now = s.told((await _live.call('signin.state', {'signin': id}))! as Json);
    } on Failed catch (e) {
      now = s.told({'state': 'failed', 'problem': e.message});
    } on LinkClosed {
      // It is asked again when the host answers.
      return;
    }
    if (_disposed || signin?.id != id) return;
    if (now.waits && now.finishing == s.finishing) return;
    signin = now;
    if (!now.waits) _asksSignin?.cancel();
    notifyListeners();
    if (now.connection case final c?) {
      _models.remove(c);
      try {
        await loadConnections();
      } on Object {
        // The list is asked for again with the next connection.
      }
    }
  }

  Future<void> cancelSignin() async {
    _asksSignin?.cancel();
    final s = signin;
    signin = null;
    notifyListeners();
    if (s == null || !s.waits) return;
    try {
      await _live.call('signin.cancel', {'signin': s.id});
    } on Object {
      // It ends by itself.
    }
  }

  /// What was last heard of a connection's models; null before that.
  List<ModelInfo>? modelsOf(String connection) => _models[connection];

  /// What providers and programs have said of their models, as the host has kept it.
  List<ModelList> lists = const [];

  Future<void> loadLists() async {
    final r = await _live.call('model.known');
    lists = [for (final j in r! as List<Object?>) ModelList.decode(j! as Json)];
    notifyListeners();
  }

  /// What is known of models that are in no list, by account and name.
  final _about = <(String, String), ModelInfo?>{};
  static const _mostAbout = 200;
  ModelInfo? aboutOf(String connection, String model) => _about[(connection, model)];

  /// Asks the host once what can be said to a model; it asks nobody.
  Future<void> about(String connection, String model) async {
    final key = (connection, model);
    if (model.isEmpty || _about.containsKey(key)) return;
    if (_about.length >= _mostAbout) _about.clear();
    _about[key] = null;
    final r = await _live.call('model.about', {'connection': connection, 'model': model});
    _about[key] = ModelInfo.decode(r! as Json);
    notifyListeners();
  }

  Future<List<ModelInfo>> models(String connection) async {
    final r = await _live.call('model.list', {'connection': connection});
    final all = [for (final j in r! as List<Object?>) ModelInfo.decode(j! as Json)];
    _models[connection] = all;
    notifyListeners();
    unawaited(loadLists().catchError((Object _) {}));
    return all;
  }

  /// What the host keeps everything to.
  Future<List<HostLimit>> limits() async {
    final r = await _live.call('host.limits');
    return [for (final j in r! as List<Object?>) HostLimit.decode(j! as Json)];
  }

  /// What a folder would have agents told; for a person to read.
  Future<List<Instructions>> look(String path) async {
    final r = await _live.call('workspace.look', {'path': path});
    return [for (final j in r! as List<Object?>) Instructions.decode(j! as Json)];
  }

  Future<String> addAgent(
    String session,
    String name,
    Execution x,
    Access access, {
    List<Instructions> instructions = const [],
    bool trust = false,
    bool apart = false,
    bool plan = false,
  }) async {
    final r = await _live.call('agent.add', {
      if (apart) 'apart': true,
      if (plan) 'plan': true,
      'session': session,
      'name': name,
      'execution': x.encode(),
      'access': access.name,
      'trust': trust,
      'instructions': [for (final i in instructions) i.encode()],
    });
    return (r! as Json)['agent']! as String;
  }

  /// [plan]: whether it is to hand in a plan first; then it only reads.
  Future<void> setAgent(
    String session,
    String agent, {
    Execution? execution,
    Access? access,
    bool? plan,
  }) => _live.call('agent.set', {
    'session': session,
    'agent': agent,
    'execution': ?execution?.encode(),
    'access': ?access?.name,
    'plan': ?plan,
  });

  /// Accepts the plan an agent has handed in last, as it is or as [text] has it. It may do what
  /// [access] says then, and begins.
  Future<void> acceptPlan(
    String session,
    String agent,
    String item,
    Access access, {
    String? text,
  }) => _live.call('plan.accept', {
    'session': session,
    'agent': agent,
    'item': item,
    'access': access.name,
    'text': ?text,
  });

  /// Whether a conversation can be branched at an item.
  Future<BranchInfo> lookBranch(String session, String agent, String item) async {
    final r = await _live.call('branch.look', {'session': session, 'agent': agent, 'item': item});
    return BranchInfo.decode(r! as Json);
  }

  /// Adds an agent that begins with another's conversation as it was at an item; with the files of its
  /// copy as they were, if so.
  Future<String> branch(
    String session,
    String agent,
    String item,
    String name, {
    bool files = false,
  }) async {
    final r = await _live.call('agent.branch', {
      'session': session,
      'agent': agent,
      'item': item,
      'name': name,
      'files': files,
      // A copy is made.
    }, _long);
    return (r! as Json)['agent']! as String;
  }

  /// Summarises an agent's conversation now, to make room in it.
  Future<void> compact(String session, String agent) async {
    await _live.call('agent.compact', {'session': session, 'agent': agent});
  }

  /// [key] names what is said: again under the same name, it is heard once.
  Future<void> prompt(String session, String agent, String text, {String? key}) =>
      _live.call('agent.prompt', {
        'session': session,
        'agent': agent,
        'text': text,
        'key': ?key,
        // A copy of the folder may have to be made first.
      }, _long);

  /// [words]: with a no, why.
  Future<void> answer(String session, RequestOpened r, Answer a, {String? words}) =>
      _live.call('request.answer', {
        'session': session,
        'request': r.request,
        'answer': a.name,
        'digest': r.digest,
        'words': ?words,
      });

  Future<void> cancel(String session, String run) =>
      _live.call('run.cancel', {'session': session, 'run': run});

  Future<void> replay(String session, String path, {double speed = 1}) async =>
      _live.call('replay.start', {'session': session, 'path': path, 'speed': speed});

  Future<void> _follow(SessionModel m) async {
    final link = _link;
    try {
      await link?.follow(m.id, m.seq);
    } on Failed catch (e) {
      // The connection is well; this session is not.
      if (identical(m, current)) m.failed(e.message);
    } on LinkClosed catch (e) {
      // Not answered in time, on a connection that is otherwise alive.
      if (identical(m, current) && identical(link, _link) && link != null) m.failed(e.why);
    }
  }

  void _message(Message m) {
    final c = current;
    switch (m) {
      case Events(:final session, :final events):
        if (session != c?.id) return _link?.consumed(events.length);
        _pending.addAll(events);
        _schedule();
      case Resync(:final session, :final last) when session == c?.id:
        // Falling behind now and then is normal, and goes on at a slower pace each time. A history
        // that keeps changing is not, and is said.
        final now = DateTime.now();
        _restarts
          ..removeWhere((t) => now.difference(t) > const Duration(seconds: 10))
          ..add(now);
        if (last > 0) {
          _drainAll();
          Timer(Duration(milliseconds: 20 << _restarts.length.clamp(0, 6)), () {
            if (!_disposed && identical(current, c)) unawaited(_follow(c!));
          });
          return;
        }
        if (_restarts.length > 3) {
          c!.failed('the host and this window disagree about the session');
          return;
        }
        // The host has another history: begin again rather than mix the two.
        _forget();
        current = SessionModel(c!.id);
        notifyListeners();
        unawaited(_follow(current!));
      case WaitingChanged():
        unawaited(loadNeeds());
      case FolderOpened(:final path):
        onFolder?.call(path);
      case Fault(:final session, :final words) when session == c?.id:
        c!.failed(words);
      case _:
    }
  }

  void _forget() {
    _link?.consumed(_pending.length - _at);
    _pending.clear();
    _at = 0;
  }

  /// Waiting events, for the tests.
  @visibleForTesting
  int get waiting => _pending.length - _at;

  /// One batch per frame, however many messages came; on a timer while no frames are drawn.
  void _schedule() {
    if (_scheduled || _disposed) return;
    _scheduled = true;
    final b = SchedulerBinding.instance;
    if (b.framesEnabled) {
      b
        ..scheduleFrameCallback((_) => _drain())
        ..ensureVisualUpdate();
      // A window that is hidden while this waits must not hold it.
      _drainTimer = Timer(const Duration(milliseconds: 250), _drain);
    } else {
      _drainTimer = Timer(const Duration(milliseconds: 100), _drain);
    }
  }

  /// Applies everything that waits, whatever it costs: before a connection is let go.
  void _drainAll() {
    if (_at < _pending.length) current?.apply(_pending.sublist(_at));
    _forget();
  }

  static const _slice = 256, _sliceText = 64 * 1024, _budget = Duration(milliseconds: 4);

  static int _weight(Event e) => switch (e.body) {
    ItemDelta(:final text) || ItemEnded(:final text) => text.length,
    _ => 64,
  };

  void _drain() {
    if (!_scheduled || _disposed) return;
    _drainTimer?.cancel();
    _scheduled = false;
    final c = current;
    final sw = Stopwatch()..start();
    final from = _at;
    while (_at < _pending.length && sw.elapsed < _budget) {
      var to = _at, text = 0;
      while (to < _pending.length && to - _at < _slice && text < _sliceText) {
        text += _weight(_pending[to++]);
      }
      c?.apply(_pending.sublist(_at, to));
      _at = to;
    }
    _link?.consumed(_at - from);
    if (_at == _pending.length) {
      _pending.clear();
      _at = 0;
    } else {
      _schedule();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _retry?.cancel();
    _drainTimer?.cancel();
    _asksSignin?.cancel();
    _drop();
    super.dispose();
  }
}
