// Problems are sorted once per change, not on every read (AUD-072).

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/lsp.dart';
import 'package:axiom_desktop/state/problems.dart';
import 'package:axiom_desktop/state/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('all() returns the same sorted list until the problems change; counts follow', () {
    final client = DaemonClient(DaemonConfig(program: 'axiomd-not-started'));
    final ws = WorkspaceStore(client);
    final lsp = LspStore(client, ws);
    final p = ProblemsStore(lsp, client: client, workspace: ws);
    addTearDown(p.dispose);
    final a = p.all();
    expect(identical(p.all(), a), isTrue);
    expect(p.count(Severity.error), 0);
    p.begin('cargo/check', 'task-1', 'cargo check');
    expect(identical(p.all(), a), isFalse, reason: 'a change drops the cached list');
    expect(() => p.all().add(a.isEmpty ? _d : a.first), throwsUnsupportedError);
  });
}

const _d = Diagnostic(source: 's', severity: Severity.error, message: 'm');
