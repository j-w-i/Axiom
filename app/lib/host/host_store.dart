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
  });

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
        unawaited(refresh().catchError((_) {}));
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

  HostLink get _live => _link ?? (throw const LinkClosed('the host is not answering'));

  Future<void> refresh() async {
    final link = _live;
    final r = await link.call('session.list', {'limit': listed});
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

  /// Ways to reach providers, as the host has them.
  List<Connection> connections = const [];
  final _models = <String, List<ModelInfo>>{};

  Future<void> loadConnections() async {
    final r = await _live.call('connection.list');
    connections = [for (final j in r! as List<Object?>) Connection.decode(j! as Json)];
    notifyListeners();
  }

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

  Future<void> disconnect(String id) async {
    await _live.call('connection.remove', {'connection': id});
    _models.remove(id);
    await loadConnections();
  }

  /// What was last heard of a connection's models; null before that.
  List<ModelInfo>? modelsOf(String connection) => _models[connection];

  Future<List<ModelInfo>> models(String connection) async {
    final r = await _live.call('model.list', {'connection': connection});
    final all = [for (final j in r! as List<Object?>) ModelInfo.decode(j! as Json)];
    _models[connection] = all;
    notifyListeners();
    return all;
  }

  Future<String> addAgent(String session, String name, Execution x, Access access) async {
    final r = await _live.call('agent.add', {
      'session': session,
      'name': name,
      'execution': x.encode(),
      'access': access.name,
    });
    return (r! as Json)['agent']! as String;
  }

  Future<void> setAgent(String session, String agent, {Execution? execution, Access? access}) =>
      _live.call('agent.set', {
        'session': session,
        'agent': agent,
        'execution': ?execution?.encode(),
        'access': ?access?.name,
      });

  Future<void> prompt(String session, String agent, String text) =>
      _live.call('agent.prompt', {'session': session, 'agent': agent, 'text': text});

  Future<void> answer(String session, RequestOpened r, Answer a) => _live.call('request.answer', {
    'session': session,
    'request': r.request,
    'answer': a.name,
    'digest': r.digest,
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
    _drop();
    super.dispose();
  }
}
