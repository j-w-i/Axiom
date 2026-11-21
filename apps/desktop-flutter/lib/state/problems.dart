import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'lsp.dart';
import 'workspace.dart';

/// Problems from every source: the language servers, and provider tasks' diagnostics
/// (`build/diagnostics`; each run of a task replaces its last run's).
class ProblemsStore extends ChangeNotifier {
  ProblemsStore(this.lsp, {DaemonClient? client, WorkspaceStore? workspace})
    : _workspace = workspace {
    lsp.addListener(_changed);
    _off = client?.on(Notifications.buildDiagnostics, (p) {
      final prev = _byTask[p.taskId] ?? const [];
      _byTask[p.taskId] = [...prev, ...p.diagnostics];
      if (_runs.values.any((r) => r.taskId == p.taskId)) notifyListeners();
    });
    workspace?.addListener(_onWorkspace);
  }

  final LspStore lsp;
  final WorkspaceStore? _workspace;
  void Function()? _off;
  int _seen = -1;

  /// provider/task → its latest run.
  final _runs = <String, ({String taskId, String label})>{};
  final _byTask = <String, List<Diagnostic>>{};
  bool _wasOpen = false;

  void _onWorkspace() {
    final open = _workspace?.isOpen ?? false;
    if (_wasOpen && !open) {
      _runs.clear();
      _byTask.clear();
      notifyListeners();
    }
    _wasOpen = open;
  }

  /// A new run of [key] (provider/task) started as [taskId]: its problems replace the last run's.
  void begin(String key, String taskId, String label) {
    _runs[key] = (taskId: taskId, label: label);
    // The replaced run's diagnostics, and any of tasks that are no run's, go.
    final live = {for (final r in _runs.values) r.taskId};
    _byTask.removeWhere((id, _) => !live.contains(id));
    notifyListeners();
  }

  List<Diagnostic> get _build => [for (final r in _runs.values) ...?_byTask[r.taskId]];

  void _changed() {
    if (lsp.diagnosticsVersion == _seen) return;
    _seen = lsp.diagnosticsVersion;
    notifyListeners();
  }

  /// Errors first, then warnings, then the rest; by file and line within each.
  List<Diagnostic> all() {
    final list = [..._build, ...lsp.problems()];
    int rank(Severity s) => switch (s) {
      Severity.error => 0,
      Severity.warning => 1,
      Severity.info => 2,
      Severity.hint => 3,
    };
    list.sort((a, b) {
      final r = rank(a.severity).compareTo(rank(b.severity));
      if (r != 0) return r;
      final f = (a.location?.file ?? '').compareTo(b.location?.file ?? '');
      if (f != 0) return f;
      return (a.location?.line ?? 0).compareTo(b.location?.line ?? 0);
    });
    return list;
  }

  int count(Severity s) => all().where((d) => d.severity == s).length;

  @override
  void dispose() {
    _off?.call();
    _workspace?.removeListener(_onWorkspace);
    lsp.removeListener(_changed);
    super.dispose();
  }
}

/// The accessible name of a Problems row: `Error: message at file:line:col, from source`.
String problemLabel(Diagnostic d) {
  final kind = switch (d.severity) {
    Severity.error => 'Error',
    Severity.warning => 'Warning',
    Severity.info => 'Info',
    Severity.hint => 'Hint',
  };
  final where = d.location == null
      ? ''
      : ' at ${d.location!.file}:${d.location!.line}${d.location!.column == null ? '' : ':${d.location!.column}'}';
  final code = d.code == null ? '' : ' (${d.code})';
  return '$kind: ${d.message}$code$where, from ${d.source}';
}
