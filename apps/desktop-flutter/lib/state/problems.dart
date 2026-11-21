import 'package:flutter/foundation.dart';

import '../daemon/protocol.g.dart';
import 'lsp.dart';

/// Problems from every source: the language servers now, builds and tasks (FL-CONSOLE-001).
class ProblemsStore extends ChangeNotifier {
  ProblemsStore(this.lsp) {
    lsp.addListener(_changed);
  }

  final LspStore lsp;
  List<Diagnostic> _build = const [];
  int _seen = -1;

  /// Build and task diagnostics (replaced per run).
  void setBuild(List<Diagnostic> list) {
    _build = list;
    notifyListeners();
  }

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
