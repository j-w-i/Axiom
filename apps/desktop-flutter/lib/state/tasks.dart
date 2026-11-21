// Task runs (FL-CONSOLE-001; a port of the Tauri shell's TasksStore): one command at a time per
// workspace, `command: { shell }` verbatim, output applied in `seq` order and split into lines per
// stream, ANSI stripped, capped at 10 000 lines, published once per frame so a chatty task cannot
// stall the UI. The history (20) is the workspace state's `run.history`. Provider tasks
// (`build/tasks`) run through `build/run`; their diagnostics go to Problems.

import 'dart:async';

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

const historyKey = 'run.history';
const maxHistory = 20;
const maxLines = 10000;
const _maxRowsPerFlush = 2000;

enum RowKind { line, status, statusFailed, error }

final class OutputRow {
  const OutputRow(this.id, this.text, this.stream, this.kind);
  final int id;
  final String text;

  /// `stdout`, `stderr`, or null for a status row.
  final StreamKind? stream;
  final RowKind kind;
}

/// "Build: cargo build".
String providerTaskTitle(TaskInfo2 t) {
  final kind = switch (t.kind) {
    TaskKind.build => 'Build',
    TaskKind.run => 'Run',
    TaskKind.test => 'Test',
    TaskKind.clean => 'Clean',
    TaskKind.other => 'Task',
  };
  return '$kind: ${t.label}';
}

final _ansi = RegExp(r'\x1B\[[0-?]*[ -/]*[@-~]|\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)|\x1B[@-Z\\-_]');
final _control = RegExp(r'[\x00-\x08\x0B\x0C\x0E-\x1F]');

String stripAnsi(String s) => s.replaceAll(_ansi, '').replaceAll(_control, '');

String formatDuration(int ms) {
  if (ms < 1000) return '$ms ms';
  final s = ms / 1000;
  if (s < 60) return '${s.toStringAsFixed(1)} s';
  return '${ms ~/ 60000} min ${(ms % 60000) ~/ 1000} s';
}

/// Line assembly for one task's output (pure; unit-tested).
class OutputBuffer {
  int _nextSeq = 0;
  final _early = <int, (StreamKind, String)>{};
  final _partial = {StreamKind.stdout: '', StreamKind.stderr: ''};
  final _pending = <(String, StreamKind?, RowKind)>[];

  void append(int seq, StreamKind stream, String data) {
    if (seq < _nextSeq) return; // a duplicate
    if (seq != _nextSeq) {
      _early[seq] = (stream, data);
      return;
    }
    _apply(stream, data);
    _nextSeq++;
    for (var e = _early.remove(_nextSeq); e != null; e = _early.remove(_nextSeq)) {
      _apply(e.$1, e.$2);
      _nextSeq++;
    }
  }

  static String _lastCr(String line) {
    final cr = line.lastIndexOf('\r');
    return cr >= 0 ? line.substring(cr + 1) : line;
  }

  void _apply(StreamKind stream, String data) {
    final text = _partial[stream]! + stripAnsi(data);
    final parts = text.split('\n');
    _partial[stream] = parts.removeLast();
    for (var line in parts) {
      if (line.endsWith('\r')) line = line.substring(0, line.length - 1);
      _pending.add((_lastCr(line), stream, RowKind.line));
    }
  }

  /// The task ended: trailing partial lines become lines.
  void finish() {
    for (final s in StreamKind.values) {
      final line = _partial[s]!;
      if (line.isEmpty) continue;
      _pending.add((_lastCr(line), s, RowKind.line));
      _partial[s] = '';
    }
  }

  void push(String text, StreamKind? stream, RowKind kind) => _pending.add((text, stream, kind));

  List<(String, StreamKind?, RowKind)> take(int max) {
    final n = max < _pending.length ? max : _pending.length;
    final out = _pending.sublist(0, n);
    _pending.removeRange(0, n);
    return out;
  }

  bool get hasPending => _pending.isNotEmpty;
}

class TasksStore extends ChangeNotifier {
  TasksStore(this.client, this.workspace) {
    _offs = [
      client.on(Notifications.taskOutput, (p) {
        if (_taskId.isEmpty && _starting) {
          _earlyEvents.add(p);
          return;
        }
        if (_taskId.isEmpty || p.taskId != _taskId) return;
        _buffer.append(p.seq, p.stream, p.data);
        _scheduleFlush();
      }),
      client.on(Notifications.taskExited, (p) {
        if (_taskId.isEmpty && _starting) {
          _earlyEvents.add(p);
          return;
        }
        if (_taskId.isEmpty || p.taskId != _taskId) return;
        _onExited(p);
      }),
    ];
    workspace.addListener(_onWorkspace);
    client.state.addListener(_onClient);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final List<void Function()> _offs;

  bool running = false;
  String label = '';
  String statusText = '';
  Outcome? lastOutcome;
  List<String> history = const [];
  List<OutputRow> rows = const [];
  int dropped = 0;

  /// Epoch ms the running task started (`task/list`); null when idle.
  int? startedAt;

  /// Rows appended so far, dropped ones included (the transport's rate meter samples it).
  int outputSeq = 0;
  List<TaskInfo2> providerTasks = const [];

  /// The last run was this provider task: Rerun repeats it.
  TaskInfo2? lastProvider;

  /// Called with each provider run's task id as soon as it is known (Problems).
  void Function(String key, String taskId, String label)? onProviderStart;

  String _taskId = '';
  bool _starting = false;
  var _earlyEvents = <Object>[];
  bool _stopRequested = false;
  int _generation = 0;
  var _buffer = OutputBuffer();
  int _nextRowId = 0;
  bool _flushScheduled = false;
  String? _lastWs;

  bool get canRun => !running && workspace.isOpen && client.isReady;

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    if (id == _lastWs) return;
    _lastWs = id;
    _resetState();
    history = const [];
    providerTasks = const [];
    lastProvider = null;
    notifyListeners();
    if (id != null) {
      unawaited(_loadHistory());
      unawaited(loadProviderTasks());
    }
  }

  void _onClient() {
    if (!client.isReady && running) {
      _resetState();
      notifyListeners();
    }
  }

  // ---- output ---------------------------------------------------------------------------------

  void _scheduleFlush() {
    if (_flushScheduled) return;
    _flushScheduled = true;
    SchedulerBinding.instance.scheduleFrameCallback((_) {
      _flushScheduled = false;
      _flush();
      if (_buffer.hasPending) _scheduleFlush();
    });
    SchedulerBinding.instance.ensureVisualUpdate();
  }

  void _flushNow() {
    while (_buffer.hasPending) {
      _flush();
    }
  }

  void _flush() {
    final batch = [
      for (final (text, stream, kind) in _buffer.take(_maxRowsPerFlush))
        OutputRow(_nextRowId++, text, stream, kind),
    ];
    if (batch.isEmpty) return;
    var next = [...rows, ...batch];
    if (next.length > maxLines) {
      dropped += next.length - maxLines;
      next = next.sublist(next.length - maxLines);
    }
    rows = next;
    outputSeq += batch.length;
    notifyListeners();
  }

  void _resetOutput() {
    _buffer = OutputBuffer();
    rows = const [];
    dropped = 0;
  }

  String allText() => rows.map((r) => r.text).join('\n');

  // ---- history --------------------------------------------------------------------------------

  void _pushHistory(String cmd) {
    history = [cmd, ...history.where((h) => h != cmd)].take(maxHistory).toList();
    if (!workspace.isOpen) return;
    unawaited(
      client
          .call(
            Methods.workspaceSetState,
            SetStateParams(workspaceId: workspace.id, values_: {historyKey: history}),
          )
          .then((_) {}, onError: (Object _) {}),
    );
  }

  Future<void> _loadHistory() async {
    final gen = _generation;
    try {
      final r = await client.call(
        Methods.workspaceGetState,
        GetStateParams(workspaceId: workspace.id, keys: const [historyKey]),
      );
      if (gen != _generation) return;
      final raw = r.values_[historyKey];
      final loaded = <String>{
        if (raw is List)
          for (final x in raw)
            if (x is String) x,
      }.take(maxHistory);
      // Anything run before the saved list arrived stays on top.
      history = [...history, ...loaded.where((h) => !history.contains(h))];
      notifyListeners();
    } on DaemonException {
      // No history.
    }
  }

  /// The workspace's provider tasks, for the palette.
  Future<void> loadProviderTasks() async {
    if (!workspace.isOpen) return;
    final gen = _generation;
    try {
      final r = await client.call(Methods.buildTasks, WorkspaceRef(workspaceId: workspace.id));
      if (gen == _generation) {
        providerTasks = r.tasks;
        notifyListeners();
      }
    } on DaemonException {
      // None apply.
    }
  }

  // ---- running --------------------------------------------------------------------------------

  void run(String commandLine) {
    final cmd = commandLine.trim();
    if (cmd.isEmpty || !canRun) return;
    lastProvider = null;
    unawaited(
      _start(() => _startCommand(CommandShell(shell: commandLine), cmd), cmd, record: true),
    );
  }

  /// The last command, or the last provider task when that ran last.
  String get lastLabel => lastProvider != null
      ? providerTaskTitle(lastProvider!)
      : (history.isEmpty ? '' : history.first);

  void rerunLast() {
    final p = lastProvider;
    if (p != null) return runProvider(p);
    if (history.isNotEmpty) run(history.first);
  }

  /// An explicit argv (no shell).
  void runArgv(List<String> argv) {
    if (!canRun) return;
    unawaited(_start(() => _startCommand(CommandArgv(argv: argv), argv.join(' ')), argv.join(' ')));
  }

  /// A provider task (`build/run`): the same output, transport and Stop as a command; its
  /// diagnostics go to Problems. Not in the command history.
  void runProvider(TaskInfo2 t) {
    if (!canRun) return;
    lastProvider = t;
    final key = '${t.providerId}/${t.id}';
    unawaited(
      _start(() async {
        final r = await client.call(
          Methods.buildRun,
          RunParams(workspaceId: workspace.id, providerId: t.providerId, task: t.id),
        );
        onProviderStart?.call(key, r.taskId, providerTaskTitle(t));
        return r.taskId;
      }, t.label),
    );
  }

  Future<String> _startCommand(Command command, String label) async {
    final r = await client.call(
      Methods.taskStart,
      StartParams(workspaceId: workspace.id, command: command, label: label),
    );
    return r.taskId;
  }

  Future<void> _start(Future<String> Function() launch, String what, {bool record = false}) async {
    _resetOutput();
    _earlyEvents = [];
    _starting = true;
    _stopRequested = false;
    running = true;
    label = what;
    statusText = 'Running: $what';
    lastOutcome = null;
    startedAt = null;
    if (record) _pushHistory(what);
    notifyListeners();
    final gen = _generation;
    try {
      final id = await launch();
      if (gen != _generation) return;
      _starting = false;
      _taskId = id;
      final early = _earlyEvents;
      _earlyEvents = [];
      for (final e in early) {
        if (e is OutputParams && e.taskId == id) {
          _buffer.append(e.seq, e.stream, e.data);
          _scheduleFlush();
        } else if (e is ExitedParams && e.taskId == id) {
          _onExited(e);
          return;
        }
      }
      if (_stopRequested) stop();
      unawaited(_loadStartedAt(id, gen));
    } on DaemonException catch (e) {
      if (gen != _generation) return;
      _starting = false;
      final d = e.details is Map ? e.details! as Map : const <String, Object?>{};
      final text = e.kind == ErrorKind.spawnFailed
          ? 'Could not start ${d['program'] ?? what}: ${d['osError'] ?? e.message}'
          : 'Could not run the command: ${e.message}';
      _buffer.push(text, StreamKind.stderr, RowKind.error);
      _flushNow();
      running = false;
      label = '';
      statusText = 'Failed to start';
      startedAt = null;
      notifyListeners();
    }
  }

  Future<void> _loadStartedAt(String id, int gen) async {
    try {
      final r = await client.call(Methods.taskList, ListTasksParams(workspaceId: workspace.id));
      final info = r.tasks.where((t) => t.taskId == id).firstOrNull;
      if (info != null && gen == _generation && _taskId == id && running) {
        startedAt = info.startedMs;
        notifyListeners();
      }
    } on DaemonException {
      // No elapsed time for this run.
    }
  }

  void stop() {
    if (!running) return;
    _stopRequested = true;
    if (_taskId.isEmpty) return; // still starting: cancelled once the id is known
    unawaited(
      client.call(Methods.taskCancel, TaskRef(taskId: _taskId)).then((_) {}, onError: (_) {}),
    );
  }

  void clear() {
    _resetOutput();
    if (!running) {
      statusText = '';
      lastOutcome = null;
    }
    notifyListeners();
  }

  void _onExited(ExitedParams p) {
    _buffer.finish();
    final ok = p.outcome == Outcome.succeeded;
    final d = formatDuration(p.durationMs);
    final text = ok
        ? '✔ Succeeded in $d'
        : p.outcome == Outcome.cancelled
        ? '■ Cancelled'
        : p.exitCode != null
        ? '✖ Failed with exit code ${p.exitCode} in $d'
        : p.signal != null
        ? '✖ Failed (signal ${p.signal}) in $d'
        : '✖ Failed in $d';
    _buffer.push(text, null, ok ? RowKind.status : RowKind.statusFailed);
    _flushNow();
    final was = label;
    _taskId = '';
    _stopRequested = false;
    running = false;
    label = '';
    startedAt = null;
    lastOutcome = p.outcome;
    statusText = switch (p.outcome) {
      Outcome.succeeded => '$was: succeeded',
      Outcome.cancelled => '$was: cancelled',
      Outcome.failed => p.exitCode != null ? '$was: failed (${p.exitCode})' : '$was: failed',
    };
    notifyListeners();
  }

  void _resetState() {
    ++_generation;
    _taskId = '';
    _starting = false;
    _earlyEvents = [];
    _stopRequested = false;
    _resetOutput();
    running = false;
    label = '';
    statusText = '';
    lastOutcome = null;
    startedAt = null;
  }

  @override
  void dispose() {
    for (final off in _offs) {
      off();
    }
    workspace.removeListener(_onWorkspace);
    client.state.removeListener(_onClient);
    super.dispose();
  }
}
