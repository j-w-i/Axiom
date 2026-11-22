// The console region (FL-EDIT-002 brings Problems; FL-CONSOLE-001 adds Output and the Terminal):
// a recessed well under the editor, its height and tab remembered, Mod+J toggles it.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../editor/lsp/lsp_editor.dart';
import '../state/problems.dart';
import 'output.dart';
import 'terminal_view.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/pressable.dart';
import '../ui/surfaces.dart';
import '../ui/tabs.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class ConsoleRegion extends StatelessWidget {
  const ConsoleRegion({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return ListenableBuilder(
      listenable: Listenable.merge([app.prefs, app.problems, app.tasks, app.terminals]),
      builder: (context, _) {
        final tab = app.prefs.string('console.tab', 'problems');
        final errors = app.problems.count(Severity.error);
        final warnings = app.problems.count(Severity.warning);
        return Semantics(
          label: 'Console',
          container: true,
          // Tauri's console: a flat panel with a 30 px head and a seam under it.
          child: ColoredBox(
            color: c.panel,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(
                  height: 30,
                  child: ColoredBox(
                    color: c.panel,
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(Metrics.s2, 0, Metrics.s2, 0),
                      child: Row(
                        children: [
                          AxTabs(
                            label: 'Console views',
                            height: 30,
                            tabs: [
                              TabItem(
                                id: 'output',
                                label: 'Output',
                                trailing: app.tasks.running
                                    ? const StatusDot(DotState.running)
                                    : null,
                              ),
                              TabItem(
                                id: 'problems',
                                label: 'Problems',
                                trailing: errors + warnings == 0
                                    ? null
                                    : AxBadge(errors + warnings, attention: errors > 0),
                              ),
                              TabItem(
                                id: 'terminal',
                                label: 'Terminal',
                                trailing: app.terminals.terminals.isEmpty
                                    ? null
                                    : AxBadge(app.terminals.terminals.length, attention: false),
                              ),
                            ],
                            selected: tab,
                            onSelect: (id) => app.prefs.set('console.tab', id),
                          ),
                          const Spacer(),
                          AxIconButton(
                            icon: AxiomIcons.close,
                            label: 'Hide the console',
                            shortcut: 'Mod+J',
                            size: 26,
                            onPressed: () => app.consoleVisible = false,
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
                const Hairline(),
                Expanded(
                  child: switch (tab) {
                    'output' => const OutputView(),
                    'terminal' => const TerminalPane(),
                    _ => const ProblemsView(),
                  },
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class ProblemsView extends StatelessWidget {
  const ProblemsView({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return ListenableBuilder(
      listenable: app.problems,
      builder: (context, _) {
        final list = app.problems.all();
        if (list.isEmpty) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s5),
            child: Text('No problems in the open files.', style: t.type.small),
          );
        }
        return Semantics(
          label: 'Problems, ${list.length}',
          container: true,
          child: ListView.builder(
            itemExtent: 24,
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s2, vertical: Metrics.s1),
            itemCount: list.length,
            itemBuilder: (context, i) {
              final d = list[i];
              final loc = d.location;
              final dot = switch (d.severity) {
                Severity.error => DotState.error,
                Severity.warning => DotState.warn,
                _ => DotState.running,
              };
              return AxListRow(
                label: problemLabel(d),
                hint: loc == null ? null : 'Open ${loc.file}:${loc.line} — Enter',
                onPressed: loc == null
                    ? null
                    : () => unawaited(
                        app.documents.open(loc.file, line: loc.line, column: loc.column ?? 1),
                      ),
                child: Row(
                  children: [
                    const SizedBox(width: 6),
                    StatusDot(dot, ring: c.recessed),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        d.message,
                        style: t.type.body.copyWith(fontSize: 12.5),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Text(d.source, style: t.type.tiny),
                    const SizedBox(width: 10),
                    if (loc != null)
                      Text(
                        '${loc.file}:${loc.line}${loc.column == null ? '' : ':${loc.column}'}',
                        style: t.type.numeric.copyWith(fontSize: 11),
                      ),
                    const SizedBox(width: 6),
                  ],
                ),
              );
            },
          ),
        );
      },
    );
  }
}

/// The status bar's language-server segment for the active document.
class LspStatus extends StatelessWidget {
  const LspStatus({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    return ListenableBuilder(
      listenable: Listenable.merge([app.lsp, app.documents]),
      builder: (context, _) {
        final doc = app.documents.active;
        if (doc == null) return const SizedBox.shrink();
        final server = app.lsp.serverFor(lspLanguageId(doc.path));
        if (server == null) return const SizedBox.shrink();
        final progress = app.lsp.progress[server.serverId];
        final (dot, word) = switch (server.state) {
          ServerState.ready => (DotState.ok, 'ready'),
          ServerState.starting => (DotState.running, 'starting'),
          ServerState.idle => (DotState.off, 'idle'),
          ServerState.failed => (DotState.error, 'failed'),
          ServerState.missing => (DotState.warn, 'not installed'),
          _ => (DotState.off, server.state.wire),
        };
        final label = '${server.serverId}, $word${progress == null ? '' : ', $progress'}';
        return Pressable(
          label: label,
          hint: server.state == ServerState.missing
              ? (server.installHint ?? '${server.serverId} is not installed')
              : (server.state == ServerState.failed ? 'Restart ${server.serverId}' : label),
          onPressed: server.state == ServerState.failed
              ? () => unawaited(app.lsp.restart(server.serverId))
              : () {},
          builder: (context, s) => Row(
            children: [
              StatusDot(dot),
              const SizedBox(width: 4),
              Text(server.serverId, style: t.type.tiny),
              if (progress != null) ...[
                const SizedBox(width: 6),
                Text(progress, style: t.type.tiny, overflow: TextOverflow.ellipsis),
              ],
            ],
          ),
        );
      },
    );
  }
}
