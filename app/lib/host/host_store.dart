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
  String get endpoint => '$data/run/host.sock';
}

extension _Let<T> on T {
  R let<R>(R Function(T) f) => f(this);
}

enum HostPhase { starting, connected, unreachable }

/// Starts the host if none runs, connects, follows, and reconnects. The client's only process
/// (SI-14), and the only place that talks to the host.
class HostStore extends ChangeNotifier {
  HostStore({required this.paths, required this.binary, this.spawn = true});
  final HostPaths paths;

  /// The `axiom-host` executable, or null if there is none beside the application.
  final String? binary;
  final bool spawn;

  HostPhase phase = HostPhase.starting;
  String? problem;
  List<SessionInfo> sessions = [];
  SessionModel? current;

  HostLink? _link;
  StreamSubscription<Message>? _sub;
  Timer? _retry;
  bool _disposed = false;
  int _attempt = 0;
  final _pending = <Event>[];
  bool _scheduled = false;

  static String? findBinary() {
    final exe = Platform.isWindows ? 'axiom-host.exe' : 'axiom-host';
    final here = File(Platform.resolvedExecutable).parent.path;
    for (final p in ['$here/$exe', '$here/../Resources/$exe']) {
      if (File(p).existsSync()) return p;
    }
    return null;
  }

  Future<void> start() async {
    if (_disposed) return;
    try {
      await _connect();
    } on Object catch (e) {
      if (_disposed) return;
      phase = HostPhase.unreachable;
      problem = '$e';
      notifyListeners();
      _later();
    }
  }

  void _later() {
    _attempt++;
    final ms = (250 << _attempt.clamp(0, 5)).clamp(250, 8000);
    _retry?.cancel();
    _retry = Timer(Duration(milliseconds: ms), () => unawaited(start()));
  }

  Future<HostLink> _open() async =>
      HostLink.connect(endpoint: paths.endpoint, token: await File(paths.token).readAsString());

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
    _attempt = 0;
    _sub = link.messages.listen(_message);
    unawaited(link.closed.then((why) => _lost(link, why)));
    phase = HostPhase.connected;
    problem = null;
    await refresh();
    if (current case final c?) await _follow(c);
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

  void _lost(HostLink link, String why) {
    if (_disposed || !identical(link, _link)) return;
    _link = null;
    unawaited(_sub?.cancel());
    phase = HostPhase.unreachable;
    problem = why;
    notifyListeners();
    _later();
  }

  Future<void> refresh() async {
    final r = await _link!.call('session.list');
    sessions = [for (final j in r! as List<Object?>) SessionInfo.decode(j! as Json)];
    notifyListeners();
  }

  Future<String> create(String title, {String? workspace}) async {
    final r = await _link!.call('session.create', {'title': title, 'workspace': ?workspace});
    await refresh();
    return (r! as Json)['session']! as String;
  }

  Future<void> open(String id) async {
    if (current case final c? when c.id != id) {
      _pending.clear();
      unawaited(_link?.call('session.unfollow', {'session': c.id}).catchError((_) => null));
    }
    if (current?.id != id) current = SessionModel(id);
    notifyListeners();
    if (_link != null) await _follow(current!);
  }

  Future<void> replay(String session, String path, {double speed = 1}) async =>
      _link!.call('replay.start', {'session': session, 'path': path, 'speed': speed});

  Future<void> _follow(SessionModel m) async =>
      _link?.call('session.follow', {'session': m.id, 'after': m.seq});

  void _message(Message m) {
    switch (m) {
      case Events(:final session, :final events) when session == current?.id:
        _pending.addAll(events);
        _schedule();
      case Resync(:final session, :final last) when session == current?.id:
        final c = current!;
        _drain();
        if (last == 0) {
          // The host has another history: begin again rather than mix the two.
          current = SessionModel(c.id);
          notifyListeners();
        }
        unawaited(_follow(current!).catchError((_) {}));
      case _:
    }
  }

  /// One batch per frame, however many messages came.
  void _schedule() {
    if (_scheduled) return;
    _scheduled = true;
    SchedulerBinding.instance
      ..scheduleFrameCallback((_) {
        _scheduled = false;
        _drain();
      })
      ..ensureVisualUpdate();
  }

  void _drain() {
    if (_pending.isEmpty) return;
    final batch = List.of(_pending);
    _pending.clear();
    current?.apply(batch);
  }

  @override
  void dispose() {
    _disposed = true;
    _retry?.cancel();
    unawaited(_sub?.cancel());
    _link?.close();
    super.dispose();
  }
}
