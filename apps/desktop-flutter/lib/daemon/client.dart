// The shell's daemon client (ADR-0020 §2): supervises one `axiomd` child and speaks the protocol
// over its stdio, with the observable behaviour of crates/desktop-bridge:
// - lifecycle per docs/architecture/processes.md: spawn with piped stdio, `initialize` within
//   10 s, `shutdown` → wait 3 s → close stdin → wait 2 s → kill;
// - every request is answered exactly once: a result, the daemon's error, `timeout` (after which
//   `$/cancel` is sent), or `internal` "Backend stopped";
// - notifications reach listeners in wire order, interleaved with responses as read;
// - a daemon that exits unexpectedly moves the client to `failed` with the stderr tail;
//   `restart` starts a fresh process.

import 'dart:async';
import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';

import 'protocol.g.dart';
import 'protocol_runtime.dart';

/// Where the client is in the daemon's lifecycle.
sealed class DaemonState {
  const DaemonState();
}

final class DaemonStopped extends DaemonState {
  const DaemonStopped();
}

final class DaemonStarting extends DaemonState {
  const DaemonStarting();
}

final class DaemonReady extends DaemonState {
  const DaemonReady({
    required this.serverVersion,
    required this.protocolVersion,
    required this.methods,
  });
  final String serverVersion;
  final String protocolVersion;
  final Set<String> methods;
}

final class DaemonFailed extends DaemonState {
  const DaemonFailed(this.message);
  final String message;
}

/// A request that did not succeed: the daemon's error, or one the client made (timeout,
/// stopped backend). [kind] is the protocol's error kind.
final class DaemonException implements Exception {
  DaemonException(this.kind, this.message, {this.details, this.code = -32603});
  final ErrorKind kind;
  final String message;
  final Object? details;
  final int code;

  @override
  String toString() => 'DaemonException(${kind.wire}): $message';
}

/// Lets a caller cancel a request it is waiting on.
final class CancelToken {
  final _listeners = <void Function()>[];
  bool _cancelled = false;
  bool get isCancelled => _cancelled;

  void cancel() {
    if (_cancelled) return;
    _cancelled = true;
    for (final l in List.of(_listeners)) {
      l();
    }
  }

  void _onCancel(void Function() l) => _cancelled ? l() : _listeners.add(l);
}

final class DaemonConfig {
  DaemonConfig({
    required this.program,
    this.args = const [],
    this.clientName = 'axiom-flutter',
    this.clientVersion = '0.1.0',
    this.initializeTimeout = const Duration(seconds: 10),
    this.requestTimeout = const Duration(seconds: 30),
    this.shutdownAnswerWait = const Duration(seconds: 3),
    this.shutdownExitWait = const Duration(seconds: 2),
    this.environment,
  });

  final String program;
  final List<String> args;
  final String clientName;
  final String clientVersion;
  final Duration initializeTimeout;

  /// Default for requests that do not pass their own; `Duration.zero` means none.
  final Duration requestTimeout;
  final Duration shutdownAnswerWait;
  final Duration shutdownExitWait;
  final Map<String, String>? environment;
}

/// The bridge's line bound (a longer line fails the connection).
const maxLineBytes = 64 * 1024 * 1024;
const _stderrTailLines = 5;

/// Finds axiomd like the bridge does: explicit path, `$AXIOMD`, a sibling of the running
/// executable, then `PATH`.
String? locateDaemon({String? explicit, Map<String, String>? environment, String? executable}) {
  final env = environment ?? Platform.environment;
  final name = Platform.isWindows ? 'axiomd.exe' : 'axiomd';
  bool isFile(String? p) => p != null && p.isNotEmpty && File(p).existsSync();
  if (isFile(explicit)) return explicit;
  if (isFile(env['AXIOMD'])) return env['AXIOMD'];
  final exe = executable ?? Platform.resolvedExecutable;
  final sibling = '${File(exe).parent.path}${Platform.pathSeparator}$name';
  if (isFile(sibling)) return sibling;
  final sep = Platform.isWindows ? ';' : ':';
  for (final dir in (env['PATH'] ?? '').split(sep)) {
    if (dir.isEmpty) continue;
    final p = '$dir${Platform.pathSeparator}$name';
    if (isFile(p)) return p;
  }
  return null;
}

class _Pending {
  _Pending(this.method, this.completer);
  final String method;
  final Completer<Object?> completer;
  Timer? timer;
}

class _Connection {
  _Connection(this.process, this.generation);
  final Process process;
  final int generation;
  final pending = <int, _Pending>{};
  final stderrTail = <String>[];
  final exited = Completer<int>();
  final lines = Queue<List<int>>();
  bool pumping = false;
  bool stopping = false;
  bool stdinClosed = false;

  void send(Map<String, Object?> message) {
    if (stdinClosed) return;
    process.stdin.add(utf8.encode('${jsonEncode(message)}\n'));
  }

  Future<void> closeStdin() async {
    if (stdinClosed) return;
    stdinClosed = true;
    try {
      await process.stdin.close();
    } on Object {
      // Already gone.
    }
  }
}

/// One supervised daemon and the protocol spoken with it.
class DaemonClient {
  DaemonClient(this.config);

  final DaemonConfig config;
  final ValueNotifier<DaemonState> state = ValueNotifier(const DaemonStopped());

  /// The daemon's stderr, line by line (its log).
  final ValueNotifier<String?> lastLogLine = ValueNotifier(null);
  void Function(String line)? onLog;

  _Connection? _conn;
  int _generation = 0;
  int _nextId = 0;
  final _listeners = <String, List<void Function(Object? params)>>{};

  int? get pid => _conn?.process.pid;
  bool get isReady => state.value is DaemonReady;

  /// Spawns the daemon and performs `initialize`. Completes with the resulting state.
  Future<DaemonState> start() async {
    final generation = ++_generation;
    state.value = const DaemonStarting();
    final Process process;
    try {
      process = await Process.start(
        config.program,
        config.args,
        environment: config.environment,
        mode: ProcessStartMode.normal,
      );
    } on ProcessException catch (e) {
      return _set(
        generation,
        DaemonFailed('Could not start the backend (${config.program}): ${e.message}'),
      );
    }
    final conn = _Connection(process, generation);
    _conn = conn;
    _read(conn);
    process.stderr.transform(utf8.decoder).transform(const LineSplitter()).listen((line) {
      conn.stderrTail.add(line);
      if (conn.stderrTail.length > _stderrTailLines) conn.stderrTail.removeAt(0);
      lastLogLine.value = line;
      onLog?.call(line);
    });
    unawaited(process.exitCode.then((code) => _exited(conn, code)));

    final params = InitializeParams(
      protocolVersion: protocolVersion,
      client: PeerInfo(name: config.clientName, version: config.clientVersion),
      capabilities: const ClientCapabilities(approvals: true),
    );
    try {
      final r = InitializeResult.fromJson(
        await _request(conn, 'initialize', params.toJson(), config.initializeTimeout),
      );
      if (generation != _generation || conn.stopping) return state.value;
      return _set(
        generation,
        DaemonReady(
          serverVersion: r.server.version,
          protocolVersion: r.protocolVersion,
          methods: r.methods.toSet(),
        ),
      );
    } on FormatException catch (e) {
      if (generation != _generation) return state.value;
      conn.stopping = true;
      conn.process.kill();
      _failPending(conn);
      return _set(
        generation,
        DaemonFailed("The backend's answer to 'initialize' was not understood: ${e.message}"),
      );
    } on DaemonException catch (e) {
      if (generation != _generation || conn.exited.isCompleted) return state.value;
      conn.stopping = true;
      process.kill();
      _failPending(conn);
      return _set(
        generation,
        DaemonFailed(
          e.kind == ErrorKind.timeout
              ? "The backend did not answer 'initialize' within ${config.initializeTimeout.inSeconds} seconds."
              : 'The backend refused to initialize: ${e.message}',
        ),
      );
    }
  }

  DaemonState _set(int generation, DaemonState s) {
    if (generation == _generation) state.value = s;
    return state.value;
  }

  /// Calls [method]. Throws [DaemonException] for anything but a result.
  Future<R> call<P, R>(
    Method<P, R> method,
    P params, {
    Duration? timeout,
    CancelToken? cancel,
  }) async {
    final conn = _conn;
    if (conn == null || conn.stopping || conn.exited.isCompleted || state.value is! DaemonReady) {
      throw DaemonException(ErrorKind.internal, 'Backend stopped');
    }
    final json = await _request(
      conn,
      method.name,
      encodeWire(params),
      timeout ?? config.requestTimeout,
      cancel,
    );
    return method.decodeResult(json);
  }

  /// Calls a method by name with raw JSON params (for `lsp/request` pass-through and tests).
  Future<Object?> callRaw(String method, Object? params, {Duration? timeout, CancelToken? cancel}) {
    final conn = _conn;
    if (conn == null || conn.stopping || conn.exited.isCompleted || state.value is! DaemonReady) {
      return Future.error(DaemonException(ErrorKind.internal, 'Backend stopped'));
    }
    return _request(conn, method, params, timeout ?? config.requestTimeout, cancel);
  }

  /// Sends a notification.
  void notify(String method, Object? params) {
    _conn?.send({'jsonrpc': '2.0', 'method': method, 'params': ?params});
  }

  /// Listens to [type]; the returned function removes the listener.
  void Function() on<P>(NotificationType<P> type, void Function(P params) listener) {
    void raw(Object? p) => listener(type.decodeParams(p));
    return onRaw(type.name, raw);
  }

  /// Listens to a notification by name with its raw params.
  void Function() onRaw(String method, void Function(Object? params) listener) {
    final list = _listeners.putIfAbsent(method, () => []);
    list.add(listener);
    return () => list.remove(listener);
  }

  Future<Object?> _request(
    _Connection conn,
    String method,
    Object? params,
    Duration timeout, [
    CancelToken? cancel,
  ]) {
    final id = ++_nextId;
    final p = _Pending(method, Completer<Object?>());
    conn.pending[id] = p;
    if (timeout > Duration.zero) {
      p.timer = Timer(timeout, () {
        if (conn.pending.remove(id) == null) return;
        conn.send({
          'jsonrpc': '2.0',
          'method': r'$/cancel',
          'params': {'id': id},
        });
        p.completer.completeError(
          DaemonException(
            ErrorKind.timeout,
            '$method did not answer within ${timeout.inSeconds} s',
          ),
        );
      });
    }
    cancel?._onCancel(() {
      if (!conn.pending.containsKey(id)) return;
      conn.send({
        'jsonrpc': '2.0',
        'method': r'$/cancel',
        'params': {'id': id},
      });
    });
    conn.send({'jsonrpc': '2.0', 'id': id, 'method': method, 'params': ?params});
    return p.completer.future;
  }

  /// Splits stdout into lines and hands them to [_pump] in order.
  void _read(_Connection conn) {
    final buffer = BytesBuilder(copy: false);
    conn.process.stdout.listen((chunk) {
      var start = 0;
      for (var i = 0; i < chunk.length; i++) {
        if (chunk[i] != 10) continue;
        buffer.add(chunk.sublist(start, i));
        start = i + 1;
        final line = buffer.takeBytes();
        if (line.isNotEmpty) conn.lines.add(line);
      }
      if (start < chunk.length) buffer.add(chunk.sublist(start));
      if (buffer.length > maxLineBytes) {
        buffer.clear();
        _protocolFailure(
          conn,
          'The backend sent a line over ${maxLineBytes ~/ (1024 * 1024)} MiB.',
        );
      }
      if (!conn.pumping) unawaited(_pump(conn));
    }, cancelOnError: true);
  }

  /// Handles lines one at a time. After a response, the event loop turns once before the next
  /// line, so code awaiting that response runs before any later notification is dispatched:
  /// callers observe responses and notifications in wire order (ipc.md), as the bridge's single
  /// ordered event channel guarantees.
  Future<void> _pump(_Connection conn) async {
    conn.pumping = true;
    try {
      while (conn.lines.isNotEmpty) {
        final bytes = conn.lines.removeFirst();
        // A large line (a file's text) is decoded off the UI thread; the pump waits for it, so
        // the order of lines is kept (audit finding 8).
        final wasResponse = bytes.length >= _offThreadBytes
            ? _message(conn, await _decodeOffThread(bytes))
            : _line(conn, bytes);
        if (wasResponse && conn.lines.isNotEmpty) await Future<void>.delayed(Duration.zero);
      }
    } finally {
      conn.pumping = false;
    }
  }

  /// Lines from this size are decoded on another isolate.
  static const _offThreadBytes = 256 * 1024;

  /// The decoded line, or [_notJson] when it is not JSON.
  static Future<Object?> _decodeOffThread(List<int> bytes) async {
    try {
      return await Isolate.run(() => jsonDecode(utf8.decode(bytes, allowMalformed: false)));
    } on Object {
      return _notJson;
    }
  }

  static const _notJson = Object();

  /// Handles one line; true when it answered a request.
  bool _line(_Connection conn, List<int> bytes) {
    Object? msg;
    try {
      msg = jsonDecode(utf8.decode(bytes, allowMalformed: false));
    } on FormatException {
      msg = _notJson;
    }
    return _message(conn, msg);
  }

  /// Handles one decoded line; true when it answered a request.
  bool _message(_Connection conn, Object? msg) {
    if (identical(msg, _notJson)) {
      _protocolFailure(conn, 'The backend sent a line that is not JSON.');
      return false;
    }
    if (msg is! Map) return false;
    final m = msg.cast<String, Object?>();
    final id = m['id'];
    if (m.containsKey('method') && id == null) {
      final method = m['method']! as String;
      for (final l in List.of(_listeners[method] ?? const <void Function(Object?)>[])) {
        try {
          l(m['params']);
        } on Object catch (e, st) {
          FlutterError.reportError(
            FlutterErrorDetails(exception: e, stack: st, library: 'daemon client'),
          );
        }
      }
      return false;
    }
    if (id is! int) return false; // a server-to-client request: none in 0.4
    final p = conn.pending.remove(id);
    if (p == null) return false; // timed out already
    p.timer?.cancel();
    if (m.containsKey('error')) {
      final e = asMap(m['error']);
      final data = e['data'] is Map ? asMap(e['data']) : const <String, Object?>{};
      ErrorKind kind;
      try {
        kind = ErrorKind.fromJson(data['kind']);
      } on FormatException {
        kind = ErrorKind.unknown;
      }
      p.completer.completeError(
        DaemonException(
          kind,
          e['message'] as String? ?? 'error',
          details: data['details'],
          code: (e['code'] as num?)?.toInt() ?? -32603,
        ),
      );
    } else {
      p.completer.complete(m['result']);
    }
    return true;
  }

  void _protocolFailure(_Connection conn, String message) {
    if (conn.stopping) return;
    conn.stopping = true;
    conn.process.kill();
    _failPending(conn);
    _set(conn.generation, DaemonFailed(message));
  }

  void _exited(_Connection conn, int code) {
    conn.exited.complete(code);
    _failPending(conn);
    if (conn.stopping) return;
    conn.stopping = true;
    final tail = conn.stderrTail.isEmpty ? '' : '\n${conn.stderrTail.join('\n')}';
    _set(conn.generation, DaemonFailed('The backend exited unexpectedly (code $code).$tail'));
  }

  void _failPending(_Connection conn) {
    final all = conn.pending.values.toList();
    conn.pending.clear();
    for (final p in all) {
      p.timer?.cancel();
      if (!p.completer.isCompleted) {
        p.completer.completeError(DaemonException(ErrorKind.internal, 'Backend stopped'));
      }
    }
  }

  /// processes.md §Daemon lifecycle step 5; bounded.
  Future<void> shutdown() async {
    final conn = _conn;
    _conn = null;
    if (conn == null) {
      state.value = const DaemonStopped();
      return;
    }
    final wasReady = state.value is DaemonReady;
    conn.stopping = true;
    if (wasReady && !conn.exited.isCompleted) {
      try {
        await _request(conn, 'shutdown', const <String, Object?>{}, config.shutdownAnswerWait);
      } on DaemonException {
        // Timed out or stopped: close stdin regardless.
      }
    }
    await conn.closeStdin();
    try {
      await conn.exited.future.timeout(config.shutdownExitWait);
    } on TimeoutException {
      conn.process.kill(ProcessSignal.sigkill);
      await conn.exited.future.timeout(const Duration(seconds: 1), onTimeout: () => -1);
    }
    _failPending(conn);
    _set(conn.generation, const DaemonStopped());
  }

  /// Replaces the daemon with a fresh process; requests to the old one fail "Backend stopped".
  Future<DaemonState> restart() async {
    final old = _conn;
    _conn = null;
    if (old != null) {
      old.stopping = true;
      _failPending(old);
      unawaited(old.closeStdin());
      unawaited(
        old.exited.future.timeout(
          config.shutdownExitWait,
          onTimeout: () {
            old.process.kill(ProcessSignal.sigkill);
            return -1;
          },
        ),
      );
    }
    return start();
  }
}
